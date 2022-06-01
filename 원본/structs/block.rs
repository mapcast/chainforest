extern crate lazy_static;
use serde::{Deserialize, Serialize};
use crypto_hash::{hex_digest, Algorithm};
use crate::structs::transaction::Transaction;
use crate::connection::libp2psetting::get_reload_flag;
use std::time::{SystemTime, UNIX_EPOCH};
use rocksdb::{Options, DB};
use std::str;
use std::sync::Mutex;


#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Block {
    pub index: u64, 
    //created_at: u128,
    pub created_at: String,
    pub nonce: u64,
    pub transactions: Vec<Transaction>,
    pub block_hash: Option<String>, 
    previous_block_hash: String,
}

lazy_static! { static ref EMPTY_FLAG: Mutex<Vec<bool>> = Mutex::new(Vec::new()); }

impl Block {
    pub fn new(transactions: Vec<Transaction>, previous_block: &Block) -> Block {
        Block {
            index: previous_block.index + 1,
            created_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis()
            .to_string(),
            nonce: 0,
            transactions: transactions,
            block_hash: None,
            previous_block_hash: previous_block.block_hash.clone().unwrap(),
        }
    }

    pub fn genesis() -> Block {
        let transaction = Transaction {
            hash: String::from("genesis"),
            from: String::from("genesis"),
            to: String::from("genesis"),
            amount: 0,
        };

        Block {
            index: 1,
            created_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis()
            .to_string(),
            nonce: 0,
            transactions: vec![transaction],
            block_hash: None,
            previous_block_hash: String::from("0"),
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap()
    }

    pub fn hash(block: &Block) -> String {
        hex_digest(Algorithm::SHA256, block.to_json().as_bytes())
    }

    pub fn update_hash(&mut self) {
        self.block_hash = Some(hex_digest(Algorithm::SHA256, self.to_json().as_bytes()));
    }

    pub fn valid(hash: &str, prefix: &str) -> bool {
        hash.starts_with(prefix)
    }

    pub fn mine_single_threaded_mutably(block: &mut Block, prefix: &str) -> bool {
        while !Self::valid(&Self::hash(block), prefix) {
            //reload_flag가 true일 시 채굴을 중지하고 false를 반환합니다.
            let flag = get_reload_flag();
            if flag {
                return false;
            }
            block.nonce += 1;
        }
        block.update_hash();
        true
    }
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync + 'static>>;
pub type Blocks = Vec<Block>;

pub fn init_local_block() {
    let db = DB::open_default("/tmp/chainforest/blocks").unwrap();
}

pub async fn read_local_block() -> Result<Blocks> {
    let mut result = Vec::new();
    let db = DB::open_default("/tmp/chainforest/blocks").unwrap();
    let mut iter = db.raw_iterator();
    iter.seek_to_first();
    while iter.valid() {
        match str::from_utf8(&iter.value().to_owned().unwrap()) {
            Ok(v) => {
                let blocktojson: Block = serde_json::from_str(v)?;
                result.push(blocktojson);
            },
            Err(e) => panic!("Invalid utf-8 seq!: {}", e),
        };
        iter.next();
    }
    Ok(result)
}

pub fn read_local_block_rest() -> Result<Blocks> {
    let mut result = Vec::new();
    let opts = Options::default();
    let db = DB::open_for_read_only(&opts, "/tmp/chainforest/blocks", false).unwrap();
    let mut iter = db.raw_iterator();
    iter.seek_to_first();
    while iter.valid() {
        match str::from_utf8(&iter.value().to_owned().unwrap()) {
            Ok(v) => {
                let blocktojson: Block = serde_json::from_str(v).unwrap();
                result.push(blocktojson);
            },
            Err(e) => panic!("Invalid utf-8 seq!: {}", e),
        };
        iter.next();
    }

    Ok(result)
}
/* 
pub async fn write_local_blocks(blocks: &Blocks) -> Result<()> {
    let json = serde_json::to_string(&blocks)?;
    fs::write(STORAGE_FILE_PATH, &json).await?;
    Ok(())
}
*/

pub fn write_local_blocks(block: &Block) -> Result<()> {
    let db = DB::open_default("/tmp/chainforest/blocks").unwrap();
    //let keytobytes = key.as_bytes().to_vec();
    //println!("{:?}", block);
    
    let keytobytes = block.index.clone().to_string().as_bytes().to_vec();
    let mut flag = true;
    //모든 블록을 불러오고, 채굴 된 블록과 index를 대조합니다.
    let mut iter = db.raw_iterator();
    iter.seek_to_first();
    while iter.valid() {
        match str::from_utf8(&iter.value().to_owned().unwrap()) {
            Ok(v) => {
                let tempblock: Block = serde_json::from_str(v)?;
                let tempblock_created_at = tempblock.created_at.parse::<u128>().unwrap();
                let block_created_at = block.created_at.parse::<u128>().unwrap();
                
                if tempblock.index == block.index && tempblock_created_at <= block_created_at {
                    flag = false;
                }
            },
            Err(e) => panic!("Invalid utf-8 seq!: {}", e),
        };
        iter.next();
    }

    //인덱스가 겹치지 않을 시의 처리입니다. 채굴된 블록을 rocksdb에 넣습니다.
    if flag {
        let blocktobytes = serde_json::to_string(&block).expect("jsonify block").as_bytes().to_vec();
        db.put(keytobytes.clone(), blocktobytes).unwrap();
    }
    
    Ok(())
}

pub fn create_genesis_block() -> Result<()> {
    let db = DB::open_default("/tmp/chainforest/blocks").unwrap();
    let mut iter = db.raw_iterator();
    
    iter.seek_to_last();

    if !iter.valid() {
        println!("can't find block. program will create genesis block...");
        let mut genesis = Block::genesis();
        genesis.update_hash();
        let keytobytes = String::from("genesis").as_bytes().to_vec();
        let genesistobytes = serde_json::to_string(&genesis).expect("jsonify block").as_bytes().to_vec();
        db.put(keytobytes, genesistobytes).unwrap();
    }

    Ok(())
}

pub fn read_last_block_rest() -> Block {
    let opts = Options::default();
    let db = DB::open_for_read_only(&opts, "/tmp/chainforest/blocks", false).unwrap();
    let mut searchblock: Block = Block::genesis();
    let mut count = 0;
    searchblock.update_hash();
    let mut iter = db.raw_iterator();
    iter.seek_to_first();
    while iter.valid() {
        match str::from_utf8(&iter.value().to_owned().unwrap()) {
            Ok(v) => {
                let tempblock: Block = serde_json::from_str(v).unwrap();
                count = count + 1;
                //result.push(blocktojson);
                if tempblock.index > searchblock.index {
                    searchblock = tempblock;
                }
            },
            Err(e) => panic!("Invalid utf-8 seq!: {}", e),
        };
        iter.next();
    }
    searchblock
}

pub fn read_last_block() -> Result<Option<Block>> {
    let opts = Options::default();
    let db = DB::open_for_read_only(&opts, "/tmp/chainforest/blocks", false).unwrap();
    let mut lastblock: Option<Block> = None;
    let mut searchblock: Block = Block::genesis();
    let mut count = 0;
    searchblock.update_hash();
    let mut iter = db.raw_iterator();
    iter.seek_to_first();
    while iter.valid() {
        match str::from_utf8(&iter.value().to_owned().unwrap()) {
            Ok(v) => {
                let tempblock: Block = serde_json::from_str(v)?;
                count = count + 1;
                //result.push(blocktojson);
                if tempblock.index > searchblock.index {
                    searchblock = tempblock;
                }
            },
            Err(e) => println!("read last block - Invalid utf-8 seq!: {}", e),
        };
        iter.next();
    }
    if count != 0 {
        lastblock = Some(searchblock);
    }
    Ok(lastblock)
}

//thread::spwan에서는 async가 사용이 힘들고, swarm response에서는 async를 요구하기 때문에 같은 내용의 async function을 정의했습니다.
pub async fn read_last_block_async() -> Result<Option<Block>> {
    let db = DB::open_default("/tmp/chainforest/blocks").unwrap();
    let mut lastblock: Option<Block> = None;
    let mut searchblock: Block = Block::genesis();
    let mut count = 0;
    searchblock.update_hash();
    let mut iter = db.raw_iterator();
    iter.seek_to_first();
    while iter.valid() {
        match str::from_utf8(&iter.value().to_owned().unwrap()) {
            Ok(v) => {
                let tempblock: Block = serde_json::from_str(v)?;
                count = count + 1;
                //result.push(blocktojson);
                if tempblock.index >= searchblock.index {
                    searchblock = tempblock;
                }
            },
            Err(e) => panic!("Invalid utf-8 seq!: {}", e),
        };
        iter.next();
    }

    if count != 0 {
        lastblock = Some(searchblock);
    }
    Ok(lastblock)
}