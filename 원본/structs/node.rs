use serde::{Deserialize, Serialize};
use rocksdb::{Options, DB};
use std::str;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Node {
    pub id: String,
    //pub created_at: u128,
    //pub initialized_at: u128,
    pub created_at: String,
    pub initialized_at: String,
    pub ip: String,
    pub version: String,
    pub node_type: String,
    pub chain_key: String,
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync + 'static>>;
pub type Nodes = Vec<Node>;

#[cfg(target_os = "windows")]
pub fn create_node(uuid: String) -> Node {
    let user_ip = ipconfig::get_adapters().unwrap().get(0).unwrap().ip_addresses().get(1).unwrap().to_string();

    let node = Node {
        id: uuid,
        created_at: SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis()
        .to_string(),
        initialized_at: String::from("0"),
        ip: user_ip,
        version: String::from("1.0"),
        node_type: String::from("temp-type"),
        chain_key: String::from("SeekersPrivateKey"),
    };
    node
}

#[cfg(target_os = "linux")]
pub fn create_node(uuid: String) -> Node {
    let user_ip = local_ip_address::local_ip().unwrap().to_string();
    
    let node = Node {
        id: uuid,
        created_at: SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis()
        .to_string(),
        initialized_at: String::from("0"),
        ip: user_ip,
        version: String::from("1.0"),
        node_type: String::from("temp-type"),
        chain_key: String::from("SeekersPrivateKey"),
    };
    node
}

pub fn check_node(uuid: String) -> bool {
    let mut flag = false;
    let db = DB::open_default("/tmp/chainforest/nodes").unwrap();
    //let opts = Options::default();
    //let db = DB::open_for_read_only(&opts, "/tmp/chainforest/nodes", false).unwrap();
    let mut iter = db.raw_iterator();
    iter.seek_to_first();
    while iter.valid() {
        match str::from_utf8(&iter.value().to_owned().unwrap()) {
            Ok(v) => {
                let temp_node: Node = serde_json::from_str(v).unwrap();
                if temp_node.id == uuid {
                    flag = true;
                }
            },
            Err(e) => panic!("Invalid utf-8 seq!: {}", e),
        };
        iter.next();
    }
    println!("check my node flag: {}", flag);
    flag
}

pub fn my_node(uuid: String) -> Option<Node> {
    let mut node = None;
    let db = DB::open_default("/tmp/chainforest/nodes").unwrap();
    let mut iter = db.raw_iterator();
    iter.seek_to_first();
    while iter.valid() {
        match str::from_utf8(&iter.value().to_owned().unwrap()) {
            Ok(v) => {
                let temp_node: Node = serde_json::from_str(v).unwrap();
                if temp_node.id == uuid {
                    node = Some(temp_node);
                }
            },
            Err(e) => panic!("Invalid utf-8 seq!: {}", e),
        };
        iter.next();
    }
    node
}

pub async fn renewal_initialized_at(uuid: String) -> Result<()> {
    let db = DB::open_default("/tmp/chainforest/nodes").unwrap();
    let mut iter = db.raw_iterator();
    iter.seek_to_first();
    let mut ktb_option: Option<Vec<u8>> = None;
    let mut ntb_option: Option<Vec<u8>> = None;
    while iter.valid() {
        match str::from_utf8(&iter.value().to_owned().unwrap()) {
            Ok(v) => {
                let mut temp_node: Node = serde_json::from_str(v).unwrap();
                if temp_node.id == uuid {
                    let initialized_time = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("Time went backwards")
                    .as_millis()
                    .to_string();
                    temp_node.initialized_at = initialized_time;
                    ktb_option = Some(temp_node.id.clone().as_bytes().to_vec());
                    ntb_option = Some(serde_json::to_string(&temp_node).expect("jsonify node").as_bytes().to_vec());
                }
            },
            Err(e) => panic!("Invalid utf-8 seq!: {}", e),
        };
        iter.next();
    }
    
    if ktb_option.is_some() && ntb_option.is_some() {
        db.put(ktb_option.unwrap(), ntb_option.unwrap()).unwrap();
    }

    Ok(())
}

pub async fn read_local_nodes() -> Result<Nodes> { 
    let mut result = Vec::new();
    let db = DB::open_default("/tmp/chainforest/nodes").unwrap();

    let mut iter = db.raw_iterator();
    iter.seek_to_first();
    while iter.valid() {
        match str::from_utf8(&iter.value().to_owned().unwrap()) {
            Ok(v) => {
                let nodetojson: Node = serde_json::from_str(v)?;
                result.push(nodetojson);
            },
            Err(e) => panic!("Invalid utf-8 seq!: {}", e),
        };
        iter.next();
    }

    Ok(result)
}

pub fn read_local_nodes_rest() -> Result<Nodes> {
    let mut result = Vec::new();
    let opts = Options::default();
    let db = DB::open_for_read_only(&opts, "/tmp/chainforest/nodes", false).unwrap();
    let mut iter = db.raw_iterator();

    iter.seek_to_first();
    while iter.valid() {
        match str::from_utf8(&iter.value().to_owned().unwrap()) {
            Ok(v) => {
                let nodetojson: Node = serde_json::from_str(v).unwrap();
                result.push(nodetojson);
            },
            Err(e) => panic!("Invalid utf-8 seq!: {}", e),
        };
        iter.next();
    }

    Ok(result)
}


pub fn write_local_nodes(node: &Node) -> Result<()> {
    println!("write node...: {:?}", node);
    let db = DB::open_default("/tmp/chainforest/nodes").unwrap();
    let keytobytes = node.id.clone().as_bytes().to_vec();
    let nodetobytes = serde_json::to_string(&node).expect("jsonify node").as_bytes().to_vec();
    db.put(keytobytes.clone(), nodetobytes).unwrap();
    Ok(())
}