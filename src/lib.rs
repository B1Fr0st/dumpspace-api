use std::{collections::HashMap, io::Read};

use reqwest;
use serde_derive::Deserialize;



#[derive(Debug)]
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
        //do stuff
        let base_url = "https://dumpspace.spuckwaffel.com/Games";





        let url = format!("{}/{}/{}/ClassesInfo.json.gz", base_url, self.engine, self.location);
        let resp = download_url(&url)
            .expect("Failed to download classes info");
        let classes_info = serde_json::from_str::<BlobInfo>(&resp)
            .expect("Failed to parse classes info");
        parse_class_info(&classes_info, self);


        let url = format!("{}/{}/{}/StructsInfo.json.gz", base_url, self.engine, self.location);
        let resp = download_url(&url)
            .expect("Failed to download structs info"); 
        let structs_info = serde_json::from_str::<BlobInfo>(&resp)
            .expect("Failed to parse structs info");
        parse_class_info(&structs_info, self);


        let url = format!("{}/{}/{}/EnumsInfo.json.gz", base_url, self.engine, self.location);
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


        let url = format!("{}/{}/{}/FunctionsInfo.json.gz", base_url, self.engine, self.location);
        let resp = download_url(&url)
            .expect("Failed to download functions info"); 
        let functions_info = serde_json::from_str::<BlobInfo>(&resp)
            .expect("Failed to parse functions info");

        for function in &functions_info.data {
            continue;
            for (key, value) in function {
                dbg!(key, value);
                let function_name = key;
                let value = value.as_array().unwrap()[2].as_u64().unwrap();
                self.function_offset_map.insert(function_name.clone() + &function_name, value);
            }
        }


        let url = format!("{}/{}/{}/OffsetsInfo.json.gz", base_url, self.engine, self.location);
        let resp = download_url(&url)
            .expect("Failed to download offsets info"); 
        let offsets_info = serde_json::from_str::<OffsetBlob>(&resp)
            .expect("Failed to parse offsets info");
        
        for offset in &offsets_info.data {
            self.offset_map.insert(offset[0].as_str().unwrap().to_string(), offset[1].as_u64().unwrap());
        }




        Ok(())
    }

    pub fn get_member_offset(&self, class_name: &str, member_name: &str) -> Option<OffsetInfo> {
        self.class_member_map.get(&(class_name.to_string() + member_name)).cloned()
    }
    pub fn get_class_size(&self, class_name: &str) -> Option<i32> {
        self.class_size_map.get(class_name).cloned()
    }
    fn get_function_offset(&self, function_class: &str, function_name: &str) -> Option<u64> {
        self.function_offset_map.get(&(function_class.to_string() + function_name)).cloned()
    }
    pub fn get_enum_name(&self, enum_name: &str, enum_value: i64) -> Option<String> {
        self.enum_name_map.get(&(enum_name.to_string() + &enum_value.to_string())).cloned()
    }
    pub fn get_offset(&self, offset_name: &str) -> Option<u64> {
        self.offset_map.get(offset_name).cloned()
    }
    pub fn get_member_offset_unchecked(&self, class_name: &str, member_name: &str) -> usize {
        self.class_member_map.get(&(class_name.to_string() + member_name)).cloned().unwrap().offset as usize
    }
}


#[derive(Deserialize, Debug)]
pub struct GameList {
    pub games: Vec<Game>
}


#[derive(Deserialize, Debug)]
#[allow(dead_code)]
pub struct Game {
    pub hash: String,
    pub name: String,
    pub engine: String,
    pub location: String,
    pub uploaded: u64, // Unix timestamp
    pub uploader: Uploader
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
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
#[allow(dead_code)]
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

// fn main() {

//     let game_id = "6b77eceb"; // Example game ID, replace with actual game hash
//     let mut dsapi = DSAPI::new(game_id);

//     dsapi.download_content().unwrap_or_else(|err| {
//         eprintln!("Error downloading content: {}", err);
//         std::process::exit(1);
//     });

//     println!("{:?}", dsapi.get_member_offset("UWorld", "OwningGameInstance"));
    
//     println!("{:?}", dsapi.get_enum_name("EFortRarity", 4));

//     println!("0x{:x?}", dsapi.get_class_size("AActor").unwrap());

//     println!("0x{:x?}", dsapi.get_offset("OFFSET_GWORLD").unwrap());

    
//     // let game_list = GameList::init().unwrap_or_else(|err| {
//     //     eprintln!("Error initializing game list: {}", err);
//     //     std::process::exit(1);
//     // });

//     // println!("Game List: {:#?}", game_list.get_game_by_name("Fortnite").unwrap_or_else(|| {
//     //     eprintln!("Game not found");
//     //     std::process::exit(1);
//     // }).get_classes());
// }