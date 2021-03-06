mod structs;

#[macro_use]
extern crate lazy_static;
use std::collections::HashSet;
use configparser::ini::Ini;
use std::{str, fs, thread, time};
use uuid::Uuid;
use std::sync::Mutex;
use std::fmt;
use lazy_static::lazy_static;


use crate::structs::transaction::{create_new_transaction, save_transaction, get_and_flush_transactions};
use crate::structs::block::{Block, write_local_blocks, read_last_block, init_local_block, create_genesis_block};
use crate::structs::node::{read_local_nodes, write_local_nodes, create_node, check_node, renewal_initialized_at, my_node};
use crate::structs::user::{User, write_local_users};
use crate::structs::verification::{verify_transaction};

use actix_web::{get, post, web, App, HttpResponse, HttpServer, Responder};

lazy_static! {
    static ref MINEDBLOCK: Mutex<Vec<String>> = Mutex::new(Vec::new());
    static ref LASTBLOCK: Mutex<Vec<Block>> = Mutex::new(Vec::new());
}

#[get("/")]
async fn hello() -> impl Responder {
    //mining_block(false, false);
    HttpResponse::Ok().body("mining block!")
}

#[post("/echo")]
async fn echo(req_body: String) -> impl Responder {
    HttpResponse::Ok().body(req_body)
}

async fn manual_hello() -> impl Responder {
    HttpResponse::Ok().body("Hey there!")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {

    //읽어온 설정 파일의 값에 따라 ID를 세팅합니다. val 변수가 자신의 ID입니다. 설정 파일이 없을 시 새로 생성합니다.
    let mut config = Ini::new();
    let _map = config.load("/tmp/chainforest/config/chainforest.conf");
    let mut val = config.get("SECRET", "NID").unwrap_or_else(|| String::from("temp"));

    if val.eq(&String::from("temp")) {
        match fs::create_dir("/tmp"){
            Ok(_) => {},
            Err(e) => println!("error occured while create config folder:   {:?}", e),
        }
        match fs::create_dir("/tmp/chainforest"){
            Ok(_) => {},
            Err(e) => println!("error occured while create config folder: {:?}", e),
        }
        match fs::create_dir("/tmp/chainforest/config"){
            Ok(_) => {},
            Err(e) => println!("error occured while create config folder: {:?}", e),
        }
        let tempid = Uuid::new_v4().to_string();
        val = tempid.clone();
        config.set("SECRET", "NID", Some(tempid.clone()));
        match config.write("/tmp/chainforest/config/chainforest.conf") {
            Ok(_) => {},
            Err(e) => println!("error occured while write config file: {:?}", e),
        }
        println!("first setting finished.");
    } 

    init_local_block();

    //노드 목록에 자신의 아이디가 없으면 해당 노드를 생성
    if !check_node(val.clone()) {
        let node = create_node(val.clone());
        match write_local_nodes(&node) {
            Ok(_) => {},
            Err(e) => println!("error occured while save your node: {:?}", e),
        }
    }

    
    thread::spawn(move || {
        //이 두개를 전역변수로 옮깁니다.
        let mut init_flag = true;
        let mut wait_flag = true;
        loop {
            if init_flag {
                //REST 요청을 관리하는 전역변수에 다른 노드들에게서 채굴된 마지막 블럭을 요청하는 처리 삽입.
                //init_get_last_block();
                init_flag = false;
                //thread::sleep(time::Duration::from_secs(3));
            }
            //저장된 마지막 블럭을 가져옵니다.
            //에러 발생을 최소화시키기 위해 전역변수를 늘리더라도 서브스레드에서 rocksdb는 가능하면 조회하지 않는 방향으로
            let mut last_block = read_last_block_at_mutex();
        
            if last_block.is_none() {
                println!("can't find last block at mutex. search rocksdb.");
                last_block = match read_last_block() {
                    Ok(b) => b,
                    Err(e) => { println!("error occured when read last block: {}", e); None },
                };
            }
            
            //저장된 마지막 블럭이 없을때 채굴을 시작하지 않고 마지막 블럭을 요청하란 메시지와 함께 thread를 10초간 sleep 시킵니다. 있으면 정상적으로 채굴을 시작합니다.
            if last_block.is_some() {
                println!("");
                println!("/////////////////////////////////////////////////");
                println!("success to read last block: {:?}", last_block);
                println!("start to mining block...");
                let mut prefix = "0000";
                //if wait_flag {
                //    println!("upgrade difficulty, until you receive at least one of mined block.");
                //    prefix = "000000";
                //}
                println!("/////////////////////////////////////////////////");
                println!("");
        
                let mut new_block = Block::new(Vec::new(), &last_block.unwrap());
                new_block.update_hash();
                //채굴을 시작합니다. 채굴 도중 브로드캐스팅을 받으면 채굴을 중단하고 flag로 false를 반환합니다. 채굴이 정상적으로 완료되면 flag는 true가 됩니다.
                let mined_flag = Block::mine_single_threaded_mutably(&mut new_block, prefix); 
        
                if mined_flag {
                    //채굴에 성공시 노드를 저장하고 접속중인 다른 노드들에게 이를 브로드캐스팅 합니다.
                    println!("");
                    println!("/////////////////////////////////////////////////");
                    println!("complete to mining new block... nonce: {}", new_block.nonce);
                    let json = serde_json::to_string(&new_block).expect("can jsonify request");
                    //let mut minedblock = MINEDBLOCK.lock().unwrap();
                    //minedblock.push(json);
                    write_local_blocks(&new_block);
                    println!("/////////////////////////////////////////////////");
                    println!("");
                } else {    
                    println!("");
                    println!("/////////////////////////////////////////////////");
                    println!("another node has mined new block. suspend mining.");
                    println!("/////////////////////////////////////////////////");
                    println!("");
                }
            } else {
                println!("");
                println!("/////////////////////////////////////////////////");
                println!("can't search last block. create genesis block...");
                println!("/////////////////////////////////////////////////");
                println!("");
                create_genesis_block();
            }
        }
    });

    HttpServer::new(|| {
        App::new()
            .service(hello)
            .service(echo)
            .route("/hey", web::get().to(manual_hello))
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}

async fn check_mined_block() -> String {
    let mut minedblock = MINEDBLOCK.lock().unwrap();
    let mut returnstr = String::from("");
    if minedblock.len() != 0 {
        let requestjson = minedblock.pop();
        returnstr = requestjson.unwrap();
    }
    returnstr
}

pub fn push_last_block(block: Block) {
    let mut lastblock = LASTBLOCK.lock().unwrap();
    if lastblock.len() != 0 {
        let prev_block = lastblock.pop().unwrap();
        if prev_block.index == block.index && prev_block.created_at > block.created_at {
            lastblock.push(block);
        } else if block.index > prev_block.index {
            lastblock.push(block);
        } else {
            lastblock.push(prev_block);
        }
    } else {
        lastblock.push(block);
    }
    
}

fn read_last_block_at_mutex() -> Option<Block> {
    let lastblock = LASTBLOCK.lock().unwrap();
    let mut block: Option<Block> = None;
    if lastblock.len() != 0 {
        block = Some(lastblock.get(0).unwrap().clone());
    }
    block
}

//ls p 명령 입력 시 실행됩니다. 피어 리스트를 불러옵니다. mdns가 모든 발견된 노드 리스트를 줍니다.
/*
async fn handle_list_peers(swarm: &mut Swarm<MyBehaviour>) {
    println!("Discovered Peers:");
    let nodes = swarm.mdns.discovered_nodes();
    let mut unique_peers = HashSet::new();
    for peer in nodes {
        unique_peers.insert(peer);
    }
    unique_peers.iter().for_each(|p| println!("{}", p));
}
*/