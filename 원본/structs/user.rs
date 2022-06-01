use serde::{Deserialize, Serialize};
use crypto_hash::{hex_digest, Algorithm};
use std::time::{SystemTime, UNIX_EPOCH};
use rocksdb::{Options, DB};
use crate::structs::block::Block;
use std::str;


#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct User {
    hash_code: String, 
    created_at: u128,
}

impl User {
    pub fn new() -> User {
        User {
            hash_code: hex_digest(Algorithm::SHA256, Block::genesis().to_json().as_bytes()),
            created_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis(),
        }
    }
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync + 'static>>;

pub fn write_local_users(user: &User) -> Result<()> {
    let db = DB::open_default("/tmp/chainforest/users").unwrap();
    let keytobytes = user.hash_code.clone().to_string().as_bytes().to_vec();
    let blocktobytes = serde_json::to_string(&user).expect("jsonify user").as_bytes().to_vec();
    db.put(keytobytes, blocktobytes).unwrap();
    Ok(())
}

pub fn read_local_users() -> String {
    let mut result = String::from("[");
    let opts = Options::default();
    let db = DB::open_for_read_only(&opts, "/tmp/chainforest/users", false).unwrap();
    let mut iter = db.raw_iterator();
    iter.seek_to_first();
    while iter.valid() {
        match str::from_utf8(&iter.value().to_owned().unwrap()) {
            Ok(v) => {
                result.push_str(v);
                result.push_str(",");
            },
            Err(e) => panic!("Invalid utf-8 seq!: {}", e),
        };
        iter.next();
    }
    result.pop();
    result.push_str("]");
    result
}

pub fn check_from_to(from: String, to: String) -> Result<bool> {
    let mut flag1 = false;
    let mut flag2 = false;
    let db = DB::open_default("/tmp/chainforest/users").unwrap();
    let mut iter = db.raw_iterator();
    iter.seek_to_first();
    while iter.valid() {
        match str::from_utf8(&iter.value().to_owned().unwrap()) {
            Ok(v) => {
                let tempuser: User = serde_json::from_str(v)?;
                if tempuser.hash_code == from || tempuser.hash_code == to {
                    if flag1 {
                        flag2 = true;
                    } else {
                        flag1 = true;
                    }
                }
            },
            Err(e) => panic!("Invalid utf-8 seq!: {}", e),
        };
        iter.next();
    }
    Ok(flag2)
}