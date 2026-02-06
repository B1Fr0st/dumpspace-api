use proc_macro::TokenStream;
use std::collections::HashMap;
use std::io::Read;
use std::sync::OnceLock;

use quote::quote;
use serde_derive::{Deserialize, Serialize};
use syn::parse::{Parse, ParseStream};
use syn::{parse_macro_input, LitInt, LitStr, Token};

// ── Data structures ──────────────────────────────────────────────────

#[derive(Deserialize, Serialize, Clone)]
struct OffsetEntry {
    offset: i64,
    size: i64,
    is_bit: bool,
    bit_offset: i32,
}

#[derive(Deserialize, Serialize)]
struct CachedData {
    game_hash: String,
    uploaded: u64,
    class_member_map: HashMap<String, OffsetEntry>,
    class_size_map: HashMap<String, i32>,
    offset_map: HashMap<String, u64>,
    enum_name_map: HashMap<String, String>,
}

#[derive(Deserialize)]
struct GameList {
    games: Vec<Game>,
}

#[derive(Deserialize)]
struct Game {
    hash: String,
    engine: String,
    location: String,
    uploaded: u64,
}

#[derive(Deserialize)]
struct BlobInfo {
    data: Vec<HashMap<String, serde_json::Value>>,
    #[allow(dead_code)]
    updated_at: String,
    version: u64,
}

#[derive(Deserialize)]
struct OffsetBlob {
    data: Vec<Vec<serde_json::Value>>,
}

// ── Compile-time cache ───────────────────────────────────────────────

static GAME_HASH: OnceLock<String> = OnceLock::new();
static DATA: OnceLock<CachedData> = OnceLock::new();

fn get_data() -> &'static CachedData {
    DATA.get_or_init(|| {
        let game_hash = GAME_HASH.get().expect(
            "dumpspace: call setup!(\"game_hash\") at the top of your crate before using offset macros"
        );
        load_or_download(game_hash)
    })
}

fn cache_path(game_hash: &str) -> std::path::PathBuf {
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    std::path::Path::new(&manifest_dir)
        .join(".dsapi")
        .join(format!("{}.json", game_hash))
}

fn fetch_game_list() -> GameList {
    reqwest::blocking::get("https://dumpspace.spuckwaffel.com/Games/GameList.json")
        .expect("Failed to fetch dumpspace game list")
        .json()
        .expect("Failed to parse game list JSON")
}

fn load_or_download(game_hash: &str) -> CachedData {
    let path = cache_path(game_hash);

    let game_list = fetch_game_list();
    let game = game_list
        .games
        .iter()
        .find(|g| g.hash == game_hash)
        .unwrap_or_else(|| {
            panic!("Game hash '{}' not found in dumpspace game list", game_hash)
        });

    // Check if cache is still fresh
    if let Some(cached) = try_load_cache(&path, game_hash) {
        if cached.uploaded >= game.uploaded {
            return cached;
        }
    }

    let data = download(game);

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string(&data) {
        let _ = std::fs::write(&path, json);
    }

    data
}

fn try_load_cache(path: &std::path::Path, game_hash: &str) -> Option<CachedData> {
    let contents = std::fs::read_to_string(path).ok()?;
    let data: CachedData = serde_json::from_str(&contents).ok()?;
    if data.game_hash == game_hash {
        Some(data)
    } else {
        None
    }
}

// ── Download & parse ─────────────────────────────────────────────────

fn download_gz(url: &str) -> String {
    let response = reqwest::blocking::get(url)
        .unwrap_or_else(|e| panic!("Failed to fetch {}: {}", url, e));
    if !response.status().is_success() {
        panic!("Request to {} failed with status {}", url, response.status());
    }
    let mut decoder = flate2::read::GzDecoder::new(response);
    let mut s = String::new();
    decoder
        .read_to_string(&mut s)
        .unwrap_or_else(|e| panic!("Failed to decompress {}: {}", url, e));
    s
}

fn parse_class_info(blob: &BlobInfo, data: &mut CachedData) {
    for class in &blob.data {
        for (class_name, value) in class {
            let members: Vec<HashMap<String, serde_json::Value>> =
                serde_json::from_value(value.clone()).unwrap();

            for member in members {
                let key = member.keys().next().unwrap().clone();
                assert!(member.keys().len() == 1);

                if key == "__MDKClassSize" {
                    data.class_size_map.insert(
                        class_name.clone(),
                        member.get("__MDKClassSize").unwrap().as_i64().unwrap() as i32,
                    );
                    continue;
                }
                if key == "__InheritInfo" {
                    continue;
                }

                let arr = member.get(&key).unwrap().as_array().unwrap();
                let offset = arr[1].as_i64().unwrap();
                let size = arr[2].as_i64().unwrap();

                let is_bit = if blob.version == 10201 {
                    arr.len() == 4
                } else if blob.version == 10202 {
                    arr.len() == 5
                } else {
                    panic!("Unknown blob version: {}", blob.version);
                };

                let (bit_offset, member_key) = if is_bit {
                    if blob.version == 10201 {
                        (
                            arr[3].as_i64().unwrap() as i32,
                            format!("{}{}", class_name, &key[..key.len() - 4]),
                        )
                    } else {
                        (
                            arr[4].as_i64().unwrap() as i32,
                            format!("{}{}", class_name, key),
                        )
                    }
                } else {
                    (0, format!("{}{}", class_name, key))
                };

                data.class_member_map.insert(
                    member_key,
                    OffsetEntry {
                        offset,
                        size,
                        is_bit,
                        bit_offset,
                    },
                );
            }
        }
    }
}

fn download(game: &Game) -> CachedData {
    let engine = &game.engine;
    let location = &game.location;

    let mut data = CachedData {
        game_hash: game.hash.clone(),
        uploaded: game.uploaded,
        class_member_map: HashMap::new(),
        class_size_map: HashMap::new(),
        offset_map: HashMap::new(),
        enum_name_map: HashMap::new(),
    };

    let format_url = |json_type: &str| -> String {
        format!(
            "https://dumpspace.spuckwaffel.com/Games/{}/{}/{}.json.gz",
            engine, location, json_type
        )
    };

    // Classes
    let json = download_gz(&format_url("ClassesInfo"));
    let blob: BlobInfo = serde_json::from_str(&json).expect("Failed to parse ClassesInfo");
    parse_class_info(&blob, &mut data);

    // Structs (same schema as classes)
    let json = download_gz(&format_url("StructsInfo"));
    let blob: BlobInfo = serde_json::from_str(&json).expect("Failed to parse StructsInfo");
    parse_class_info(&blob, &mut data);

    // Enums
    let json = download_gz(&format_url("EnumsInfo"));
    let blob: BlobInfo = serde_json::from_str(&json).expect("Failed to parse EnumsInfo");
    for enum_info in &blob.data {
        for (enum_name, value) in enum_info {
            let entries = &value.as_array().unwrap()[0];
            for entry in entries.as_array().unwrap() {
                let obj = entry.as_object().unwrap();
                let name = obj.keys().next().unwrap().clone();
                let val = obj.get(&name).unwrap().as_i64().unwrap();
                data.enum_name_map
                    .insert(format!("{}{}", enum_name, val), name);
            }
        }
    }

    // Global offsets
    let json = download_gz(&format_url("OffsetsInfo"));
    let blob: OffsetBlob = serde_json::from_str(&json).expect("Failed to parse OffsetsInfo");
    for entry in &blob.data {
        data.offset_map.insert(
            entry[0].as_str().unwrap().to_string(),
            entry[1].as_u64().unwrap(),
        );
    }

    data
}

// ── Macro input parsing ──────────────────────────────────────────────

struct TwoStrings {
    first: LitStr,
    second: LitStr,
}

impl Parse for TwoStrings {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let first = input.parse()?;
        input.parse::<Token![,]>()?;
        let second = input.parse()?;
        Ok(Self { first, second })
    }
}

struct OneString {
    value: LitStr,
}

impl Parse for OneString {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(Self {
            value: input.parse()?,
        })
    }
}

struct EnumArgs {
    name: LitStr,
    value: LitInt,
}

impl Parse for EnumArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name = input.parse()?;
        input.parse::<Token![,]>()?;
        let value = input.parse()?;
        Ok(Self { name, value })
    }
}

// ── Proc macros ──────────────────────────────────────────────────────

/// Configures the dumpspace game hash for compile-time offset resolution.
///
/// Must be called once at the top of your crate root, before any other
/// dumpspace macros. Downloads and caches offset data on first build;
/// subsequent builds read from the local `.dsapi/` cache.
///
/// # Example
/// ```ignore
/// use dumpspace_macros::*;
///
/// setup!("6b77eceb"); // Fortnite
///
/// fn main() {
///     let off = offset!("UWorld", "OwningGameInstance");
/// }
/// ```
///
/// To force a re-download (e.g. after a game update):
/// ```sh
/// DSAPI_FORCE_REFRESH=1 cargo build
/// ```
#[proc_macro]
pub fn setup(input: TokenStream) -> TokenStream {
    let hash = parse_macro_input!(input as LitStr);
    let _ = GAME_HASH.set(hash.value());
    // Eagerly load data now so download errors surface early.
    let _ = get_data();
    quote! {}.into()
}

/// Resolves a class member offset at compile time.
///
/// Expands to a `usize` literal. Produces a compile error if the
/// class/member pair is not found in the dumpspace data.
///
/// # Example
/// ```ignore
/// let owning_game_instance = offset!("UWorld", "OwningGameInstance");
/// ```
#[proc_macro]
pub fn offset(input: TokenStream) -> TokenStream {
    let TwoStrings { first, second } = parse_macro_input!(input as TwoStrings);
    let class = first.value();
    let member = second.value();

    let data = get_data();
    let key = format!("{}{}", class, member);
    let entry = data.class_member_map.get(&key).unwrap_or_else(|| {
        panic!(
            "dumpspace: offset \"{}::{}\" not found",
            class, member
        )
    });

    let val = entry.offset as usize;
    quote! { #val }.into()
}

/// Resolves a class size at compile time.
///
/// Expands to a `usize` literal.
///
/// # Example
/// ```ignore
/// let size = class_size!("AActor");
/// ```
#[proc_macro]
pub fn class_size(input: TokenStream) -> TokenStream {
    let OneString { value } = parse_macro_input!(input as OneString);
    let class = value.value();

    let data = get_data();
    let size = data
        .class_size_map
        .get(&class)
        .unwrap_or_else(|| panic!("dumpspace: class size for \"{}\" not found", class));

    let val = *size as usize;
    quote! { #val }.into()
}

/// Resolves a global offset (e.g. `OFFSET_GWORLD`) at compile time.
///
/// Expands to a `u64` literal.
///
/// # Example
/// ```ignore
/// let gworld = global_offset!("OFFSET_GWORLD");
/// ```
#[proc_macro]
pub fn global_offset(input: TokenStream) -> TokenStream {
    let OneString { value } = parse_macro_input!(input as OneString);
    let name = value.value();

    let data = get_data();
    let off = data
        .offset_map
        .get(&name)
        .unwrap_or_else(|| panic!("dumpspace: global offset \"{}\" not found", name));

    let val = *off;
    quote! { #val }.into()
}

/// Resolves an enum value name at compile time.
///
/// Expands to a `&'static str` literal.
///
/// # Example
/// ```ignore
/// let name = enum_name!("EFortRarity", 1); // "EFortRarity__Uncommon"
/// ```
#[proc_macro]
pub fn enum_name(input: TokenStream) -> TokenStream {
    let EnumArgs { name, value } = parse_macro_input!(input as EnumArgs);
    let enum_name = name.value();
    let enum_val: i64 = value.base10_parse().unwrap();

    let data = get_data();
    let key = format!("{}{}", enum_name, enum_val);
    let result = data.enum_name_map.get(&key).unwrap_or_else(|| {
        panic!(
            "dumpspace: enum value \"{}::{}\" not found",
            enum_name, enum_val
        )
    });

    quote! { #result }.into()
}
