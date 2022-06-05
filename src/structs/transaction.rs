use serde::{Deserialize, Serialize};
use rocksdb::{DB, Options};
use std::str;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Transaction {
    pub hash: String,
    pub from: String,
    pub to: String,
    pub amount: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FromTo {
    pub from: String,
    pub to: String,
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync + 'static>>;
pub type Transactions = Vec<Transaction>;

pub fn create_new_transaction(hashcode: String, from: String, to: String) -> Transaction {
    let transaction = Transaction {
        hash: hashcode,
        from: from,
        to: to,
        amount: 0,
    };
    println!("create new transaction: {:?}", transaction);
    transaction
}

pub fn save_transaction(transaction: Transaction) -> Result<()> {
    let db = DB::open_default("/tmp/chainforest/transaction").unwrap();
    let keytobytes = transaction.hash.clone().as_bytes().to_vec();
    let vertobytes = serde_json::to_string(&transaction).expect("jsonify transaction").as_bytes().to_vec();
    db.put(keytobytes, vertobytes).unwrap();
    Ok(())
}

pub fn get_and_flush_transactions() ->  Result<Transactions> {
    let db = DB::open_default("/tmp/chainforest/transaction").unwrap();
    let mut result: Transactions = Vec::new();
    let mut keyvector: Vec<String> = Vec::new();
    let mut iter = db.raw_iterator();
    iter.seek_to_first();
    while iter.valid() {
        match str::from_utf8(&iter.value().to_owned().unwrap()) {
            Ok(v) => {
                let transtojson: Transaction = serde_json::from_str(v)?;
                keyvector.push(transtojson.hash.clone());
                result.push(transtojson);
            },
            Err(e) => panic!("Invalid utf-8 seq!: {}", e),
        };
        iter.next();
    }

    for key in keyvector {
        let keytobytes = key.as_bytes().to_vec();
        match db.delete(keytobytes) {
            Ok(_) => {},
            Err(e) => println!("error occured while delete verification data: {:?}", e),
        }
    }
    
    Ok(result)
}

pub fn flush_transactions() {
    let db = DB::open_default("/tmp/chainforest/transaction").unwrap();
    //현재 삭제 방식에 대해 성능상 문제가 생길것 같음. rocksdb에서 데이터를 한번에 깔끔하게 삭제할 방법 고안 필요
}

pub fn read_local_transactions() -> Result<Transactions> {
    let mut result: Transactions = Vec::new();
    let opts = Options::default();
    let db = DB::open_for_read_only(&opts, "/tmp/chainforest/transaction", false).unwrap();
    let mut iter = db.raw_iterator();
    iter.seek_to_first();
    while iter.valid() {
        match str::from_utf8(&iter.value().to_owned().unwrap()) {
            Ok(v) => {
                let temp_transaction: Transaction = serde_json::from_str(v).unwrap();
                result.push(temp_transaction);
            },
            Err(e) => panic!("Invalid utf-8 seq!: {}", e),
        };
        iter.next();
    }
    Ok(result)
}