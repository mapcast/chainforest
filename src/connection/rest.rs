use crypto_hash::{hex_digest, Algorithm};
use uuid::Uuid;
use crate::structs::block::{read_last_block_rest, read_local_block_rest, create_genesis_block};
use crate::structs::node::{read_local_nodes_rest};
use crate::structs::user::{read_local_users, check_from_to};
use crate::structs::transaction::{FromTo, read_local_transactions};
use crate::structs::verification::{read_verification_hash};
use std::{str, sync::Mutex};

lazy_static! {
    static ref RESTMESSAGE: Mutex<Vec<String>> = Mutex::new(Vec::new());
}

#[derive(Clone, Debug)]
pub struct Rest;

#[derive(Response)]
pub struct RestResponse {
    pub message: String,
}

#[derive(Response)]
pub struct SerdeJsonResponse {
    pub message: serde_json::Value,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RestRequest {
    pub mode: RestRequestType,
    pub hashcode: String,
    pub from: String,
    pub to: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum RestRequestType {
    CREATEUSER,
    CREATETRANSACTION,
    VERIFYTRANSACTION,
    GETLASTBLOCK,
}


fn create_random_hash_code() -> String {
    hex_digest(Algorithm::SHA256, Uuid::new_v4().to_string().as_bytes())
}

pub fn init_get_last_block() {
    let mut restm = RESTMESSAGE.lock().unwrap();

    let req = RestRequest {
        mode: RestRequestType::GETLASTBLOCK,
        hashcode: String::from(""),
        from: String::from(""),
        to: String::from(""),
    };
    let req_to_str = serde_json::to_string(&req).unwrap();
    restm.push(req_to_str);
}

pub async fn check_rest_request() -> String {
    let mut restm = RESTMESSAGE.lock().unwrap();
    let mut returnstr = String::from("");
    if restm.len() != 0 {
        let request = restm.pop();
        returnstr = request.unwrap();
    }
    returnstr
}

impl_web! {
    impl Rest {
        #[get("/")]
        #[content_type("json")]
        fn get_last(&self) -> Result<SerdeJsonResponse, ()> {
            let block = read_last_block_rest();
            let jsonfy = serde_json::to_value(block);
            Ok(SerdeJsonResponse {
                message: jsonfy.unwrap(),
            })
        }

        #[post("/user")]
        #[content_type("json")]
        fn create_user(&self) -> Result<RestResponse, ()> {
            let mut restm = RESTMESSAGE.lock().unwrap();

            let req = RestRequest {
                mode: RestRequestType::CREATEUSER,
                hashcode: String::from(""),
                from: String::from(""),
                to: String::from(""),
            };
            let req_to_str = serde_json::to_string(&req).unwrap();
            restm.push(req_to_str);
            Ok(RestResponse {
                message: String::from("CREATEUSER: request create new user"),
            })
        }

        #[get("/user")]
        #[content_type("json")]
        fn get_users(&self) -> Result<RestResponse, ()> {
            let users_string = read_local_users();
            Ok(RestResponse {
                message: users_string,
            })
        }

        #[get("/transaction")]
        #[content_type("json")]
        fn get_transactions(&self) -> Result<SerdeJsonResponse, ()> {
            let transactions = read_local_transactions().unwrap();
            let jsonfy = serde_json::to_value(transactions);
            Ok(SerdeJsonResponse {
                message: jsonfy.unwrap(),
            })
        }

        #[post("/transaction")]
        #[content_type("json")]
        fn create_transaction(&self, content_type: String, body: Vec<u8>) -> Result<RestResponse, ()> {
            let s = match str::from_utf8(&body) {
                Ok(v) => v,
                Err(e) => panic!("Invalid UTF-8 sequence: {}", e),
            };
            let fromto: FromTo = serde_json::from_str(s).unwrap();

            let flag = check_from_to(fromto.from.clone(), fromto.to.clone());

            let mut return_message = String::from("51% Verification is Requested, But invalid field has detected.");

            if flag.unwrap() {
                let random_hash = create_random_hash_code();
                let req = RestRequest {
                    mode: RestRequestType::CREATETRANSACTION,
                    hashcode: random_hash.clone(), 
                    from: fromto.from,
                    to: fromto.to,
                };
                let req_to_str = serde_json::to_string(&req).unwrap();
    
                let mut restm = RESTMESSAGE.lock().unwrap();
                restm.push(req_to_str);

                return_message = String::from("51% Verification is Requested. This Transaction's hash code: ");
                return_message.push_str(random_hash.as_str());
            }

            Ok(RestResponse {
                message: return_message,
            })
        }

        #[post("/transaction/:hashcode")]
        #[content_type("json")]
        fn verify_transaction(&self, hashcode: String) -> Result<RestResponse, ()> {
            let req = RestRequest {
                mode: RestRequestType::VERIFYTRANSACTION,
                hashcode: hashcode,
                from: String::from(""),
                to: String::from(""),
            };
            let req_to_str = serde_json::to_string(&req).unwrap();

            let mut restm = RESTMESSAGE.lock().unwrap();
            restm.push(req_to_str);

            Ok(RestResponse {
                message: String::from("request to read responded 51% Verification.")
            })
        }

        #[get("/verification")]
        #[content_type("json")]
        fn get_verification_hash(&self) -> Result<SerdeJsonResponse, ()> {
            let verification_hash = read_verification_hash().unwrap();
            let jsonfy = serde_json::to_value(verification_hash);
            Ok(SerdeJsonResponse {
                message: jsonfy.unwrap(),
            })
        }

        #[get("/block")]
        #[content_type("json")]
        fn get_blocks(&self) -> Result<SerdeJsonResponse, ()> {
            let blocks = read_local_block_rest().unwrap();
            let jsonfy = serde_json::to_value(blocks);
            Ok(SerdeJsonResponse {
                message: jsonfy.unwrap(),
            })
        }

        #[post("/block/genesis")]
        #[content_type("json")]
        fn create_genesis(&self) -> Result<RestResponse, ()> {
            match create_genesis_block() {
                Ok(_) => {},
                Err(e) => println!("error occured while create genesis block: {:?}", e),
            }
            Ok(RestResponse {
                message: String::from("genesis block is created"),
            })
        }

        #[get("/node")]
        #[content_type("json")]
        fn get_nodes(&self) -> Result<SerdeJsonResponse, ()> {
            let nodes = read_local_nodes_rest().unwrap();
            let jsonfy = serde_json::to_value(nodes);
            Ok(SerdeJsonResponse {
                message: jsonfy.unwrap(),
            })
        }
    }
}
