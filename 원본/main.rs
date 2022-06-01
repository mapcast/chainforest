mod structs;
mod connection;

#[macro_use]
extern crate tower_web;
#[macro_use]
extern crate lazy_static;
use libp2p::{Transport, core::upgrade, floodsub::Floodsub, mdns::TokioMdns, mplex, noise::{Keypair, NoiseConfig, X25519Spec}, swarm::{Swarm, SwarmBuilder}, tcp::TokioTcpConfig};
use std::collections::HashSet;
use tokio::{io::AsyncBufReadExt, sync::mpsc};
use configparser::ini::Ini;
use std::{str, fs, thread, time};
use uuid::Uuid;
use std::sync::Mutex;
use lazy_static::lazy_static;
use tower_web::{ServiceBuilder, codegen::http::header::CONTENT_TYPE};
use crate::structs::transaction::{create_new_transaction, save_transaction, get_and_flush_transactions};
use crate::structs::block::{Block, write_local_blocks, read_last_block, init_local_block};
use crate::structs::node::{read_local_nodes, write_local_nodes, create_node, check_node, renewal_initialized_at, my_node};
use crate::structs::user::{User, write_local_users};
use crate::structs::verification::{verify_transaction};
use crate::connection::libp2psetting::{
    KEYS,
    PEER_ID,
    TOPIC,
    ListMode,
    ListRequest,
    EventType,
    MyBehaviour,
};
use crate::connection::rest::{
    Rest,
    RestRequest,
    RestRequestType,
    check_rest_request,
    init_get_last_block,
};

lazy_static! {
    static ref MINEDBLOCK: Mutex<Vec<String>> = Mutex::new(Vec::new());
    static ref LASTBLOCK: Mutex<Vec<Block>> = Mutex::new(Vec::new());
}

#[tokio::main]
async fn main() {
    //읽어온 설정 파일의 값에 따라 ID를 세팅합니다. val 변수가 자신의 ID입니다. 설정 파일이 없을 시 새로 생성합니다.
    let mut config = Ini::new();
    let _map = config.load("/tmp/chainforest/config/chainforest.conf");
    let mut val = config.get("SECRET", "NID").unwrap_or_else(|| String::from("temp"));
    
    if val.eq(&String::from("temp")) {
        match fs::create_dir("/tmp"){
            Ok(_) => {},
            Err(e) => println!("error occured while create config folder: {:?}", e),
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

    //자신의 노드를 불러와서 initialized_at 현재 시간으로 변경
    match renewal_initialized_at(val.clone()).await {
        Ok(_) => {},
        Err(e) => println!("error occured while save renewalizing initialized_at field: {:?}", e),
    };

    println!("Peer Id: {}", PEER_ID.clone());
    let (response_sender, mut response_rcv) = mpsc::unbounded_channel();

    let auth_keys = Keypair::<X25519Spec>::new()
        .into_authentic(&KEYS)
        .expect("can create auth keys");
    
    let transp = TokioTcpConfig::new()
        .upgrade(upgrade::Version::V1)
        .authenticate(NoiseConfig::xx(auth_keys).into_authenticated())
        .multiplex(mplex::MplexConfig::new())
        .boxed();

    let mut behaviour = MyBehaviour {
        floodsub: Floodsub::new(PEER_ID.clone()),
        mdns: TokioMdns::new().expect("can create mdns"),
        response_sender,
    };

    behaviour.floodsub.subscribe(TOPIC.clone());

    let mut swarm = SwarmBuilder::new(transp, behaviour, PEER_ID.clone())
        .executor(Box::new(|fut| {
            tokio::spawn(fut);
        }))
        .build();

    //let mut stdin = tokio::io::BufReader::new(tokio::io::stdin()).lines();

    Swarm::listen_on(
        &mut swarm,
        "/ip4/0.0.0.0/tcp/7777"
            .parse()
            .expect("can get a local socket"),
    ).expect("swarm can be started");

    //MINING THREAD
    thread::spawn(move || {
        let mut init_flag = true;
        let mut wait_flag = true;
        loop {
            println!("debug 1");
            thread::sleep(time::Duration::from_secs(10));
            if init_flag {
                //REST 요청을 관리하는 전역변수에 다른 노드들에게서 채굴된 마지막 블럭을 요청하는 처리 삽입.
                init_get_last_block();
                init_flag = false;
                thread::sleep(time::Duration::from_secs(3));
            }
            println!("debug 2");
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
            
            println!("debug 3");
            //저장된 마지막 블럭이 없을때 채굴을 시작하지 않고 마지막 블럭을 요청하란 메시지와 함께 thread를 10초간 sleep 시킵니다. 있으면 정상적으로 채굴을 시작합니다.
            if last_block.is_some() {
                println!("");
                println!("/////////////////////////////////////////////////");
                println!("success to read last block: {:?}", last_block);
                println!("start to mining block...");
                let mut prefix = "00000";
                if wait_flag {
                    println!("upgrade difficulty, until you receive at least one of mined block.");
                    prefix = "000000";
                }
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
                    let mut minedblock = MINEDBLOCK.lock().unwrap();
                    minedblock.push(json);
                    println!("/////////////////////////////////////////////////");
                    println!("");
                } else {
                    println!("");
                    println!("/////////////////////////////////////////////////");
                    println!("another node has mined new block. suspend mining.");
                    println!("/////////////////////////////////////////////////");
                    println!("");
                    if wait_flag { wait_flag = false; }
                }
            } else {
                println!("");
                println!("/////////////////////////////////////////////////");
                println!("can't search last block. please wait...");
                println!("/////////////////////////////////////////////////");
                println!("");
            }
        }
    });

    //REST API SUB THREAD
    thread::spawn(move || {
        let addr = "127.0.0.1:8080".parse().expect("Invalid address");
        println!("Listening on http://{}", addr);

        ServiceBuilder::new()
            .resource(Rest)
            .run(&addr)
            .unwrap();
    });

    loop {
        let evt = {
            //tokio::select!는 아래의 메서드 중 하나가 실행되면 화살표 뒤에 있는 코드가 실행되는 구조입니다. 
            tokio::select! {
                //line = stdin.next_line() => Some(EventType::Input(line.expect("can get line").expect("can read line from stdin"))),
                event = swarm.next() => {
                    println!("Unhandled Swarm Event: {:?}", event);
                    None
                },
                response = response_rcv.recv() => Some(EventType::Response(response.expect("response exists"))),
                requestjson = check_mined_block() => {
                    match requestjson.as_str() {
                        "" => None,
                        _ => Some(EventType::Mined(requestjson)), 
                    }
                },
                restrequest = check_rest_request() => {
                    match restrequest.as_str() {
                        "" => None,
                        _ => Some(EventType::Rest(restrequest)),
                    }
                },
            }
        };

        if let Some(event) = evt {
            match event {
                EventType::Response(resp) => {
                    let json = serde_json::to_string(&resp).expect("can jsonify response");
                    swarm.floodsub.publish(TOPIC.clone(), json.as_bytes());
                },
                /* 
                EventType::Input(line) => match line.as_str() {
                    "ls p" => handle_list_peers(&mut swarm).await,
                    _ => println!("unknown command"),
                },
                */
                EventType::Mined(json) => {
                    let mut block: Block = serde_json::from_str(&json.clone()).unwrap();
                    let transactions = get_and_flush_transactions();
                    block.transactions = transactions.unwrap();
                    let my_node = my_node(val.clone());
                    match write_local_blocks(&block.clone()) {
                        Ok(_) => {},
                        Err(e) => panic!("error occured while saving mined block: {:?}", e),
                    }
                    push_last_block(block.clone());
                    
                    let req = ListRequest {
                        mode: ListMode::NEWBLOCK,
                        transaction: None,
                        block: Some(block),
                        node: my_node,
                    };
                    let reqjson = serde_json::to_string(&req).expect("can jsonify request");
                    swarm.floodsub.publish(TOPIC.clone(), reqjson.as_bytes());
                },
                EventType::Rest(request) => {
                    match request.as_str() {
                        "" => {},
                        _ => {
                            let restreq: RestRequest = serde_json::from_str(&request.as_str()).unwrap();
                            match restreq.mode {
                                RestRequestType::GETLASTBLOCK => {
                                    println!("program init - get last block activated.");
                                    let req = ListRequest {
                                        mode: ListMode::LASTBLOCK,
                                        transaction: None,
                                        block: None,
                                        node: None,
                                    };
                                
                                    let json = serde_json::to_string(&req).expect("can jsonify request");
                                    swarm.floodsub.publish(TOPIC.clone(), json.as_bytes());
                                },
                                RestRequestType::CREATEUSER => {
                                    let user = User::new();
                                    match write_local_users(&user) {
                                        Ok(_) => println!("CREATEUSER: new user saved."),
                                        Err(e) => println!("error occured while save new user: {:?}", e),
                                    }
                                },
                                RestRequestType::CREATETRANSACTION => {
                                    //println!("from: {}, to: {}", from, to);
                                    //51% 인증을 진행한다. 승인 되었을때 트랙잭션을 삽입하고 해당 트랙잭션을 브로드캐스팅 한다.
                                    //51% 인증은 새로운 boolean 벡터 전역변수를 만들고 거기에 true가 들어왔나 false가 들어왔나에 따라 트랙잭션을 삽입하거나 그냥 버린다
                                    let my_node = my_node(val.clone());
                                    let req = ListRequest {
                                        mode: ListMode::VERIFY51,
                                        transaction: Some(create_new_transaction(restreq.hashcode, restreq.from, restreq.to)),
                                        block: None,
                                        node: my_node,
                                    };
                                    let reqjson = serde_json::to_string(&req).expect("can jsonify request");
                                    swarm.floodsub.publish(TOPIC.clone(), reqjson.as_bytes());
                                },
                                RestRequestType::VERIFYTRANSACTION => {
                                    match verify_transaction(restreq.hashcode) {
                                        Ok(t) => {
                                            if t.is_some() {
                                                println!("success to verify transaction: {:?}", t);
                                                let req = ListRequest {
                                                    mode: ListMode::NEWTRANSACTION,
                                                    transaction: t.clone(),
                                                    block: None,
                                                    node: None,
                                                };
                                                match save_transaction(t.unwrap()) {
                                                    Ok(_) => {},
                                                    Err(e) => panic!("error occured while saving transaction: {:?}", e),
                                                }
                                                
                                                let reqjson = serde_json::to_string(&req).expect("can jsonify request");
                                                swarm.floodsub.publish(TOPIC.clone(), reqjson.as_bytes());
                                            } else {
                                                println!("verify request is denied.");
                                            }
                                        },
                                        Err(e) => println!("error occured while verify transaction: {:?}", e),
                                    }
                                },
                                _ => {
                                    println!("wrong rest request has come.")
                                },
                            }
                        },
                    }
                }
            }
        }   
    }
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