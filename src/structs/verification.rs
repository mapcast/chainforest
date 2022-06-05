use serde::{Deserialize, Serialize};
use crate::structs::transaction::{Transaction, save_transaction};
use rocksdb::DB;
use std::str;

#[derive(Serialize, Deserialize, Debug)]
pub struct Verification {
    pub id: String, //자신의 아이디
    pub transaction: Transaction,
    pub status: bool,
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync + 'static>>;

pub fn read_verification_hash() -> Result<Vec<String>> {
    let db = DB::open_default("/tmp/chainforest/verification").unwrap();
    let mut result: Vec<String> = Vec::new();
    let mut iter = db.raw_iterator();
    iter.seek_to_first();
    while iter.valid() {
        match str::from_utf8(&iter.value().to_owned().unwrap()) {
            Ok(v) => {
                let temp_verification: Verification = serde_json::from_str(v)?;
                let tv_hash = temp_verification.transaction.hash;
                if !result.contains(&tv_hash) {
                    result.push(tv_hash);
                }
            },
            Err(e) => panic!("Invalid utf-8 seq!: {}", e),
        };
        iter.next();
    }

    Ok(result)
}

pub fn write_local_verification(verification: &Verification) -> Result<()> {
    let db = DB::open_default("/tmp/chainforest/verification").unwrap();
    let keytobytes = verification.id.clone().as_bytes().to_vec();
    let vertobytes = serde_json::to_string(&verification).expect("jsonify verification").as_bytes().to_vec();
    db.put(keytobytes, vertobytes).unwrap();
    Ok(())
}

pub fn verify_transaction(hashcode: String) -> Result<Option<Transaction>> {
    let db = DB::open_default("/tmp/chainforest/verification").unwrap();
    let mut transaction: Option<Transaction> = None;
    let mut flag = false;
    let mut verify_ok = 0;
    let mut verify_no = 0;
    let mut keyvector: Vec<String> = Vec::new();
    let mut iter = db.raw_iterator();
    iter.seek_to_first();
    while iter.valid() {
        match str::from_utf8(&iter.value().to_owned().unwrap()) {
            Ok(v) => {
                let temp_verification: Verification = serde_json::from_str(v).unwrap();
                if temp_verification.transaction.hash == hashcode {
                    if !flag {
                        transaction = Some(temp_verification.transaction);
                        flag = true;
                    }

                    if temp_verification.status {
                        verify_ok = verify_ok + 1;
                    } else {
                        verify_no = verify_no + 1;
                    }

                    keyvector.push(temp_verification.id);
                }
            },
            Err(e) => panic!("Invalid utf-8 seq!: {}", e),
        };
        iter.next();
    }

    if verify_ok > verify_no {
        println!("write transaction...");
        match save_transaction(transaction.clone().unwrap()) {
            Ok(_) => {},
            Err(e) => println!("error occured while save transaction: {:?}", e),
        }
    } else if verify_ok == 0 && verify_no == 0 {
        println!("no verify response for your hashcode");
    }

    //저장해둔 verification id들을 모두 지운다..
    for key in keyvector {
        let keytobytes = key.as_bytes().to_vec();
        match db.delete(keytobytes) {
            Ok(_) => {},
            Err(e) => println!("error occured while delete verification data: {:?}", e),
        }
    }

    Ok(transaction)
}