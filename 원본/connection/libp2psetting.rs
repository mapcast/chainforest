use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use libp2p::{
    floodsub::{Floodsub, FloodsubEvent, Topic},
    identity,
    swarm::{NetworkBehaviourEventProcess},
    mdns::{MdnsEvent, TokioMdns},
    NetworkBehaviour, PeerId,
};
use configparser::ini::Ini;
use crate::{push_last_block, structs::{node::{Node, Nodes, read_local_nodes, write_local_nodes}, verification::write_local_verification}};
use crate::structs::transaction::{Transaction, save_transaction, get_and_flush_transactions};
use crate::structs::block::{Block, Blocks, read_local_block, write_local_blocks, read_last_block_async};
use crate::structs::verification::Verification;
use std::sync::{LockResult, Mutex};
use once_cell::sync::Lazy;

lazy_static! { static ref RELOAD_FLAG: Mutex<Vec<bool>> = Mutex::new(Vec::new()); }
pub static KEYS: Lazy<identity::Keypair> = Lazy::new(|| identity::Keypair::generate_ed25519());
pub static PEER_ID: Lazy<PeerId> = Lazy::new(|| PeerId::from(KEYS.public()));
pub static TOPIC: Lazy<Topic> = Lazy::new(|| Topic::new("nodes"));

pub trait LockResultExt {
    type Guard;
    fn ignore_poison(self) -> Self::Guard;
}

impl<Guard> LockResultExt for LockResult<Guard> {
    type Guard = Guard;
    fn ignore_poison(self) -> Guard {
        self.unwrap_or_else(|e| e.into_inner())
    }
}


#[derive(Debug, Serialize, Deserialize)]
pub enum ListMode {
    ALLNODE,
    ONENODE(String),
    ALLBLOCK,
    ONEBLOCK(String),
    VERIFY51,
    NEWBLOCK,
    NEWNODE, //NEWNODE와 LASTBLOCK은 현재 사실상 사용처가 없는 상태이고, 차후 삭제 될 수 있습니다.
    LASTBLOCK,
    NEWTRANSACTION,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListRequest { 
    pub mode: ListMode,
    pub transaction: Option<Transaction>,
    pub block: Option<Block>,
    pub node: Option<Node>
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListResponse {
    mode: ListMode,
    node: Option<Nodes>,//이름 바뀐다고 문제 생기는거 아님
    block: Option<Blocks>,
    verification: Option<Verification>,
    receiver: String,
}

pub enum EventType {
    Response(ListResponse),
    //Input(String),
    Mined(String),
    Rest(String),
}

#[derive(NetworkBehaviour)]
pub struct MyBehaviour {
    pub floodsub: Floodsub,
    pub mdns: TokioMdns,
    #[behaviour(ignore)]
    pub response_sender: mpsc::UnboundedSender<ListResponse>,
}

//다른 노드에서 노드 리스트를 요청할때 보내는 펑션을 정의합니다.
impl NetworkBehaviourEventProcess<FloodsubEvent> for MyBehaviour {
    fn inject_event(&mut self, event: FloodsubEvent) {
        match event {
            FloodsubEvent::Message(msg) => {
                if let Ok(resp) = serde_json::from_slice::<ListResponse>(&msg.data) {
                    
                    if resp.receiver == PEER_ID.to_string() {
                        println!("Response from {}:", msg.source);

                        match resp.mode {
                            ListMode::LASTBLOCK => {
                                let mut blocks = resp.block.unwrap();
                                if blocks.len() != 0 {
                                    let block = blocks.pop().unwrap();
                                    write_local_blocks(&block.clone());
                                    push_last_block(block);
                                }
                            },
                            ListMode::VERIFY51 => {
                                //일단 그냥 데이터베이스에 verification을 저장합시다.
                                let verification = resp.verification.unwrap();
                                write_local_verification(&verification);
                            },
                            _ => {
                                resp.node.iter().for_each(|r| println!("{:?}", r));
                                resp.block.iter().for_each(|r| println!("{:?}", r));
                                resp.verification.iter().for_each(|r| println!("{:?}", r));
                            },
                        }
                    }
                } else if let Ok(req) = serde_json::from_slice::<ListRequest>(&msg.data) {
                    match req.mode {
                        ListMode::ALLNODE => {
                            println!("Received ALL req: {:?} from {:?}", req, msg.source);
                            respond_with_public_nodes(
                                self.response_sender.clone(),
                                msg.source.to_string(),
                            );
                        }
                        ListMode::ONENODE(ref peer_id) => {
                            if peer_id == &PEER_ID.to_string() {
                                println!("Received req: {:?} from {:?}", req, msg.source);
                                respond_with_public_nodes(
                                    self.response_sender.clone(),
                                    msg.source.to_string(),
                                );
                            }
                        }
                        ListMode::ALLBLOCK => {
                            println!("Received ALL req: {:?} from {:?}", req, msg.source);
                            respond_with_public_blocks(
                                self.response_sender.clone(),
                                msg.source.to_string(),
                            )
                        }
                        ListMode::ONEBLOCK(ref peer_id) => {
                            if peer_id == &PEER_ID.to_string() {
                                println!("Received req: {:?} from {:?}", req, msg.source);
                                respond_with_public_blocks(
                                    self.response_sender.clone(),
                                    msg.source.to_string(),
                                );
                            }
                        }
                        ListMode::VERIFY51 => {
                            println!("Received VERIFY51 req: {:?} from {:?}", req, msg.source);
                            let transaction = req.transaction.unwrap();
                            let node = req.node.unwrap();
                            respond_with_verifies(
                                self.response_sender.clone(),
                                msg.source.to_string(),
                                transaction,
                                node,
                            );
                        }
                        ListMode::NEWBLOCK => {
                            //새로운 블록이 브로드캐스팅 되었습니다. 해당 블록을 로컬 rocksdb에 삽입합니다.
                            //채굴한 사람의 노드 객체도 같이 브로드캐스팅 됩니다. 노드 갱신에 사용 할 수 있습니다.
                            println!("");
                            println!("/////////////////////////////////////////////////");
                            println!("Received NEWBLOCK req: {:?} from {:?}", req, msg.source);
                            let mined_block = req.block.unwrap();
                            let miner = req.node.unwrap();
                            //println!("miner info: {:?}", miner);
                            write_local_nodes(&miner);
                            //println!("save mined block....: {:?}", mined_block);
                            let mut reload_flag = RELOAD_FLAG.lock().ignore_poison();
                            reload_flag.push(true);
                            //채굴한 블럭에서 트랙잭션을 다 넣었기에 여기서는 그냥 로컬 rocksdb의 트랙잭션을 싹 비워줍시다.
                            get_and_flush_transactions();
                            write_local_blocks(&mined_block.clone());
                            push_last_block(mined_block);
                            println!("/////////////////////////////////////////////////");
                            println!("");
                        }
                        ListMode::NEWNODE => {
                            println!("");
                            println!("/////////////////////////////////////////////////");
                            println!("Received NEWNODE req: {:?} from {:?}", req, msg.source);
                            let new_node = req.node.unwrap();
                            println!("new node detected: {:?}", new_node);
                            write_local_nodes(&new_node);
                            println!("/////////////////////////////////////////////////");
                            println!("");
                        }
                        ListMode::LASTBLOCK => {
                            println!("");
                            println!("get last block req: {:?} from {:?}", req, msg.source);
                            respond_with_last_block(
                                self.response_sender.clone(),
                                msg.source.to_string(),
                            );
                        }
                        ListMode::NEWTRANSACTION => {
                            println!("create transaction request has come: {:?}", req.transaction);
                            save_transaction(req.transaction.unwrap());
                        }
                    }
                } 
            }
            _ => (),
        }
    }
}

pub fn get_reload_flag() -> bool {
    let mut flag = false; 
    let mut reload_flag = RELOAD_FLAG.lock().ignore_poison();
    if reload_flag.len() != 0 {
        flag = reload_flag.pop().unwrap();
        reload_flag.clear();
    }
    flag
}

fn respond_with_public_nodes(sender: mpsc::UnboundedSender<ListResponse>, receiver: String) {
    tokio::spawn(async move {
        match read_local_nodes().await {
            Ok(nodes) => {
                let resp = ListResponse {
                    mode: ListMode::ALLNODE,
                    receiver,
                    node: Some(nodes),
                    block: None,
                    verification: None,
                };
                if let Err(e) = sender.send(resp) {
                    println!("error sending response via channel, {}", e);
                }
            }
            Err(e) => println!("error fetching local nodes to answer ALL request, {}", e),
        }
    });
}

fn respond_with_public_blocks(sender: mpsc::UnboundedSender<ListResponse>, receiver: String) {
    tokio::spawn(async move {
        match read_local_block().await {
            Ok(blocks) => {
                let resp = ListResponse {
                    mode: ListMode::ALLBLOCK,
                    receiver,
                    node: None,
                    block: Some(blocks),
                    verification: None,
                };
                if let Err(e) = sender.send(resp) {
                    println!("error sending response via channel, {}", e);
                }
            }
            Err(e) => println!("error fetching local transactions to answer ALL request, {}", e),
        }
    });
}

fn respond_with_last_block(sender: mpsc::UnboundedSender<ListResponse>, receiver: String) {
    tokio::spawn(async move {
        match read_last_block_async().await {
            Ok(block) => {
                let mut blocks = Vec::new();
                if block.is_some() {
                    blocks.push(block.unwrap());
                }

                let resp = ListResponse {
                    mode: ListMode::LASTBLOCK,
                    receiver,
                    node: None, 
                    block: Some(blocks),
                    verification: None,
                };
                if let Err(e) = sender.send(resp) {
                    println!("error sending response via channel, {}", e);
                }
            }
            Err(e) => println!("error fetching local transactions to answer ALL request, {}", e),
        }
    });
}

fn respond_with_verifies(sender: mpsc::UnboundedSender<ListResponse>, receiver: String, transaction: Transaction, node: Node) {
    tokio::spawn(async move {
        match read_local_nodes().await {
            Ok(nodes) => {

                let mut config = Ini::new();
                let _map = config.load("/tmp/chainforest/config/chainforest.conf");
                let val = config.get("SECRET", "NID").unwrap();

                let mut flag = false;
                let mut ids = Vec::new();

                //노드의 모든 ID를 가져옵니다(현재 node id가 다르게 정의되있어서 테스트용으로 ip 필드를 사용)
                for n in nodes.iter() {
                    ids.push(n.ip.clone());
                }
                //id에 요청을 보낸 node의 id가 포함되어 있는지를 확인합니다.
                if ids.contains(&node.id) {
                    flag = true;
                }

                //반환되는 Verification 객체에는 자신의 PEER ID와 테스트 성공 여부가 들어갑니다.
                let resp = ListResponse {
                    mode: ListMode::VERIFY51,
                    receiver,
                    node: None,
                    block: None,
                    verification: Some(Verification {
                        id: val,
                        transaction: transaction,
                        status: flag,
                    }),
                };

                if let Err(e) = sender.send(resp) {
                    println!("error sending response via channel, {}", e);
                }
            }
            Err(e) => println!("error fetching local nodes to answer ALL request, {}", e),
        }
    });
}


impl NetworkBehaviourEventProcess<MdnsEvent> for MyBehaviour {
    fn inject_event(&mut self, event: MdnsEvent) {
        match event {
            MdnsEvent::Discovered(discovered_list) => {
                for (peer, _addr) in discovered_list {
                    self.floodsub.add_node_to_partial_view(peer);
                }
            }
            MdnsEvent::Expired(expired_list) => {
                for (peer, _addr) in expired_list {
                    if !self.mdns.has_node(&peer) {
                        self.floodsub.remove_node_from_partial_view(&peer);
                    }
                }
            }
        }
    }
}