use std::{collections::HashMap, io::Read};

use reqwest;
use serde_derive::Deserialize;



#[derive(Debug)]
/// The main struct for the Dumpspace API, which provides methods to interact with the Dumpspace data.
/// It must be initialized with a specific game ID (hash) and then must call `download_content` to fetch and parse the data.
/// # Example:
/// ```
/// use dumpspace_api::DSAPI;
/// let game_id = "6b77eceb"; // Example game ID, replace with actual game hash
/// let mut dsapi = DSAPI::new(game_id);
/// dsapi.download_content().unwrap(); // Download and parse the content (if this fails you're screwed anyways so might as well unwrap)
/// println!("{:?}", dsapi.get_member_offset("UWorld", "OwningGameInstance"));
/// println!("{:?}", dsapi.get_enum_name("EFortRarity", 4));
/// println!("0x{:x?}", dsapi.get_class_size("AActor").unwrap());
/// println!("0x{:x?}", dsapi.get_offset("OFFSET_GWORLD").unwrap());
/// ```
pub struct DSAPI {
    game_list: GameList,
    class_member_map: HashMap<String, OffsetInfo>,
    class_size_map: HashMap<String, i32>,
    function_offset_map: HashMap<String, u64>,
    enum_name_map: HashMap<String, String>,
    offset_map: HashMap<String, u64>,

    pub engine: String,
    pub location: String,

}

impl DSAPI {
    /// Creates a new instance of `DSAPI` for a specific game identified by its hash.
    /// This function initializes the game list and sets the engine and location based on the provided game ID.
    /// Game ID can be found in the url of a dumpspace game page, and will be a query argument called `hash`.
    pub fn new(game_id: &str) -> Self {
        let mut ret = DSAPI {
            game_list: GameList::init().expect("Failed to initialize game list"),
            class_member_map: HashMap::new(),
            class_size_map: HashMap::new(),
            function_offset_map: HashMap::new(),
            enum_name_map: HashMap::new(),
            offset_map: HashMap::new(),
            engine: String::new(),
            location: String::new(),
        };
        ret.engine = ret.game_list.get_game_by_hash(game_id)
            .expect("Game not found")
            .engine
            .clone();
        ret.location = ret.game_list.get_game_by_hash(game_id)
            .expect("Game not found")
            .location
            .clone();
        ret
    }
    /// Downloads and parses the content from the dumpspace API.
    /// This function fetches various JSON blobs containing class, struct, enum, and function information,
    /// and populates the internal maps with this data.
    pub fn download_content(&mut self) -> Result<(), String> {
        fn parse_class_info(classes_info: &BlobInfo, dsapi: &mut DSAPI) {
            for class in &classes_info.data {

                for (key, value) in class {
                    let class_name = key;
                    let value: Vec<HashMap<String, serde_json::Value>> = serde_json::from_str(&value.to_string()).unwrap();
                    for value in value {
                        let key = value.keys().next().unwrap().as_str();
                        assert!(value.keys().len() == 1);
                        if key == "__MDKClassSize" {
                            dsapi.class_size_map.insert(class_name.clone(), value.get("__MDKClassSize").unwrap().as_i64().unwrap() as i32);
                            continue;
                        }
                        if key == "__InheritInfo" {
                            continue;
                        }

                        let mut info = OffsetInfo::new();
                        let value_data = value.get(key).unwrap().as_array().unwrap();
                        info.offset = value_data[1].as_i64().unwrap();
                        info.size = value_data[2].as_i64().unwrap();

                        if classes_info.version == 10201 {
                            info.is_bit = value_data.len() == 4;
                        } else if classes_info.version == 10202 {
                            info.is_bit = value_data.len() == 5;
                        } else {
                            panic!("Unknown version: {}", classes_info.version);
                        }
                        info.valid = true;

                        if info.is_bit {
                            
                            if classes_info.version == 10201 {
                                info.bit_offset = value_data[3].as_i64().unwrap() as i32;
                                dsapi.class_member_map.insert(class_name.clone() + &key[..key.len()-4], info);
                            } else if classes_info.version == 10202 {
                                info.bit_offset = value_data[4].as_i64().unwrap() as i32;
                                dsapi.class_member_map.insert(class_name.clone() + key, info);
                                //class_member_map insertion
                            } else {
                                panic!("Unknown version: {}", classes_info.version);
                            }
                        } else {
                            dsapi.class_member_map.insert(class_name.clone() + key, info);
                        }
                        
                    }
                }
            }
        }
        fn download_url(url: &str) -> Result<String, String> {
            let response = reqwest::blocking::get(url)
                .map_err(|e| format!("Failed to fetch URL {}: {}", url, e))?;
            if response.status().is_success() {
                let mut d = flate2::read::GzDecoder::new(response);
                let mut s = String::new();
                d.read_to_string(&mut s).map_err(|e| format!("Failed to read decompressed data: {}", e))?;
                Ok(s)
            } else {
                Err(format!("Request failed with status: {}", response.status()))
            }
        }
        let engine = self.engine.clone();
        let location = self.location.clone();
        let format_url = |json_type: &str| -> String {
            format!("https://dumpspace.spuckwaffel.com/Games/{}/{}/{}.json.gz", engine, location, json_type)
        };





        let url = format_url("ClassesInfo");
        let resp = download_url(&url)
            .expect("Failed to download classes info");
        let classes_info = serde_json::from_str::<BlobInfo>(&resp)
            .expect("Failed to parse classes info");
        parse_class_info(&classes_info, self);


        let url = format_url("StructsInfo");
        let resp = download_url(&url)
            .expect("Failed to download structs info"); 
        let structs_info = serde_json::from_str::<BlobInfo>(&resp)
            .expect("Failed to parse structs info");
        parse_class_info(&structs_info, self);


        let url = format_url("EnumsInfo");
        let resp = download_url(&url)
            .expect("Failed to download enums info");
        let enums_info = serde_json::from_str::<BlobInfo>(&resp)
            .expect("Failed to parse enums info");

        for enum_info in &enums_info.data {
            for (key, value) in enum_info {
                let enum_name = key;
                let value = &value.as_array().unwrap()[0];
                for entry in value.as_array().unwrap() {
                    let entry: serde_json::Map<String, serde_json::Value> = entry.as_object().unwrap().clone();
                    let enum_value_name = entry.keys().next().unwrap();
                    assert!(entry.keys().len() == 1);
                    let enum_value = entry.get(enum_value_name).unwrap().as_i64().unwrap();
                    self.enum_name_map.insert(enum_name.to_owned() + &enum_value.to_string().clone(), enum_value_name.clone());
                }
            }
        }


        // let url = format_url("FunctionsInfo");
        // let resp = download_url(&url)
        //     .expect("Failed to download functions info"); 
        // let functions_info = serde_json::from_str::<BlobInfo>(&resp)
        //     .expect("Failed to parse functions info");
        // for function in &functions_info.data {
            
        //     for (key, value) in function {
        //         dbg!(key, value);
        //         let function_name = key;
        //         let value = value.as_array().unwrap()[2].as_u64().unwrap();
        //         self.function_offset_map.insert(function_name.clone() + &function_name, value);
        //     }
        // }


        let url = format_url("OffsetsInfo");
        let resp = download_url(&url)
            .expect("Failed to download offsets info"); 
        let offsets_info = serde_json::from_str::<OffsetBlob>(&resp)
            .expect("Failed to parse offsets info");
        
        for offset in &offsets_info.data {
            self.offset_map.insert(offset[0].as_str().unwrap().to_string(), offset[1].as_u64().unwrap());
        }




        Ok(())
    }
    /// Returns the offset info for a class member as an `Option<OffsetInfo>`.
    pub fn get_member_offset(&self, class_name: &str, member_name: &str) -> Option<OffsetInfo> {
        self.class_member_map.get(&(class_name.to_string() + member_name)).cloned()
    }
    /// Returns the size of a class as an `Option<i32>`.
    /// Returns `None` if the class is not found.
    pub fn get_class_size(&self, class_name: &str) -> Option<i32> {
        self.class_size_map.get(class_name).cloned()
    }
    /// Returns the offset of a function as an `Option<u64>`.
    /// Returns `None` if the function is not found.
    /// Note: Functions are not currently implemented.
    #[allow(dead_code)] //removeme
    fn get_function_offset(&self, function_class: &str, function_name: &str) -> Option<u64> {
        self.function_offset_map.get(&(function_class.to_string() + function_name)).cloned()
    }
    /// Returns the name of an enum value as an `Option<String>`.
    /// Returns `None` if the enum name or value is not found.
    pub fn get_enum_name(&self, enum_name: &str, enum_value: i64) -> Option<String> {
        self.enum_name_map.get(&(enum_name.to_string() + &enum_value.to_string())).cloned()
    }
    /// Returns the offset of a specific offset name as an `Option<u64>`.
    /// Returns `None` if the offset name is not found.
    pub fn get_offset(&self, offset_name: &str) -> Option<u64> {
        self.offset_map.get(offset_name).cloned()
    }
    /// Returns the offset info for a class member with an .unwrap() and cast to usize.
    /// This function will panic if the member is not found.
    /// # Safety: This function assumes that the member exists and will panic if it does not.
    /// This should be fine to use in practice, as the code should only panic if the member is misspelled or does not exist.
    pub fn get_member_offset_unchecked(&self, class_name: &str, member_name: &str) -> usize {
        self.class_member_map.get(&(class_name.to_string() + member_name)).cloned().unwrap().offset as usize
    }
}


#[derive(Deserialize, Debug)]
pub struct GameList {
    pub games: Vec<Game>
}


#[derive(Deserialize, Debug)]
pub struct Game {
    pub hash: String,
    pub name: String,
    pub engine: String,
    pub location: String,
    pub uploaded: u64, // Unix timestamp
    pub uploader: Uploader
}

#[derive(Deserialize, Debug)]
pub struct Uploader {
    pub name: String,
    pub link: String,
}
#[derive(Deserialize, Debug, Clone)]
pub struct OffsetInfo {
    pub offset: i64,
    pub size: i64,
    pub is_bit: bool,
    pub bit_offset: i32,
    pub valid: bool,
}

impl OffsetInfo {
    pub fn new() -> Self {
        OffsetInfo {
            offset: 0,
            size: 0,
            is_bit: false,
            bit_offset: 0,
            valid: false,
        }
    }
}

// converting bool() operation from c++
impl Into<bool> for OffsetInfo {
    fn into(self) -> bool {
        self.valid
    }
}


#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct BlobInfo {
    data: Vec<HashMap<String, serde_json::Value>>,
    updated_at: String, // Unix timestamp
    version: u64, // Version number
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct OffsetBlob {
    credit: HashMap<String, String>,
    data: Vec<Vec<serde_json::Value>>, //fucking hate this
    updated_at: String, // Unix timestamp
    version: u64, // Version number
}
impl GameList {
    pub fn init() -> Result<Self, String> {
        let url = "https://dumpspace.spuckwaffel.com/Games/GameList.json";

        let response = reqwest::blocking::get(url)
            .map_err(|e| format!("Failed to fetch game list: {}", e))?;

        if response.status().is_success() {
            let text = response.text().map_err(|e| format!("Failed to read response text: {}", e))?;
            serde_json::from_str(&text).map_err(|e| format!("Failed to parse JSON: {}", e))
        } else {
            Err(format!("Request failed with status: {}", response.status()))
        }
    }
    pub fn get_game_by_hash(&self, hash: &str) -> Option<&Game> {
        self.games.iter().find(|game| game.hash == hash)
    }
    pub fn get_game_by_name(&self, name: &str) -> Option<&Game> {
        self.games.iter().find(|game| game.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    static mut LOCAL_DSAPI: std::sync::LazyLock<DSAPI> = std::sync::LazyLock::new(||{let mut res = DSAPI::new("6b77eceb");res.download_content().unwrap();return res;}); //fortnite

    #[test]
    fn test_new_dsapi() {
        let dsapi = DSAPI::new("6b77eceb");
        assert_eq!(dsapi.engine, "Unreal-Engine-5");
        assert_eq!(dsapi.location, "Fortnite");
    }

    #[test]
    fn test_get_member_offset_some() {
        let dsapi = unsafe{ (&raw const LOCAL_DSAPI).as_ref().unwrap() };
        let info = dsapi.get_member_offset("UWorld", "OwningGameInstance");
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.offset, 0x228);
        assert_eq!(info.size, 8);
        assert!(info.valid);
    }

    #[test]
    fn test_get_member_offset_none() {
        let dsapi = unsafe{ (&raw const LOCAL_DSAPI).as_ref().unwrap() };
        assert!(dsapi.get_member_offset("NoClass", "NoMember").is_none());
    }

    #[test]
    fn test_get_class_size_some() {
        let dsapi = unsafe{ (&raw const LOCAL_DSAPI).as_ref().unwrap() };
        assert_eq!(dsapi.get_class_size("UWorld"), Some(2536));
    }

    #[test]
    fn test_get_class_size_none() {
        let dsapi = unsafe{ (&raw const LOCAL_DSAPI).as_ref().unwrap() };
        assert_eq!(dsapi.get_class_size("NoClass"), None);
    }

    #[test]
    #[allow(unreachable_code)] //removeme
    fn test_get_function_offset_some() {
        return; //functions are not implemented yet.
        let dsapi = unsafe{ (&raw const LOCAL_DSAPI).as_ref().unwrap() };
        assert_eq!(dsapi.get_function_offset("TestClass", "TestFunc"), Some(0x1234));
    }

    #[test]
    fn test_get_function_offset_none() {
        let dsapi = unsafe{ (&raw const LOCAL_DSAPI).as_ref().unwrap() };
        assert_eq!(dsapi.get_function_offset("NoClass", "NoFunc"), None);
    }

    #[test]
    fn test_get_enum_name_some() {
        let dsapi = unsafe{ (&raw const LOCAL_DSAPI).as_ref().unwrap() };
        assert_eq!(dsapi.get_enum_name("EFortRarity", 1), Some("EFortRarity__Uncommon".to_string()));
    }

    #[test]
    fn test_get_enum_name_none() {
        let dsapi = unsafe{ (&raw const LOCAL_DSAPI).as_ref().unwrap() };
        assert_eq!(dsapi.get_enum_name("NoEnum", 2), None);
    }

    #[test]
    fn test_get_offset_some() {
        let dsapi = unsafe{ (&raw const LOCAL_DSAPI).as_ref().unwrap() };
        assert_eq!(dsapi.get_offset("OFFSET_GWORLD"), Some(0x14942840));
    }

    #[test]
    fn test_get_offset_none() {
        let dsapi = unsafe{ (&raw const LOCAL_DSAPI).as_ref().unwrap() };
        assert_eq!(dsapi.get_offset("NO_OFFSET"), None);
    }

    #[test]
    fn test_get_member_offset_unchecked() {
        let dsapi = unsafe{ (&raw const LOCAL_DSAPI).as_ref().unwrap() };
        let offset = dsapi.get_member_offset_unchecked("UWorld", "OwningGameInstance");
        assert_eq!(offset, 0x228);
    }

    #[test]
    #[should_panic]
    fn test_get_member_offset_unchecked_panic() {
        let dsapi = unsafe{ (&raw const LOCAL_DSAPI).as_ref().unwrap() };
        dsapi.get_member_offset_unchecked("NoClass", "NoMember");
    }
}