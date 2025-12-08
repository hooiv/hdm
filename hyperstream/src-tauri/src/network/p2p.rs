use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::collections::{HashMap, HashSet};
use tokio::net::UdpSocket;
use serde::{Serialize, Deserialize};


const DISCOVERY_PORT: u16 = 14734;
const P2P_HTTP_PORT: u16 = 14733; // Same as main HTTP server

#[derive(Debug, Clone, Serialize, Deserialize)]
enum P2PMessage {
    WhoHas { url_hash: String },
    IHave { url_hash: String, http_port: u16 },
}

#[derive(Clone)]
pub struct P2PNode {
    socket: Arc<UdpSocket>,
    // Map of URL Hash -> List of Peer Addrs (IP:Port)
    peers: Arc<Mutex<HashMap<String, HashSet<SocketAddr>>>>,
    // Set of URL Hashes we have locally and can serve
    my_files: Arc<Mutex<HashSet<String>>>,
}

impl P2PNode {
    pub async fn new() -> Result<Self, String> {
        let socket = UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], DISCOVERY_PORT)))
            .await
            .map_err(|e| format!("Failed to bind UDP socket: {}", e))?;
        
        socket.set_broadcast(true).map_err(|e| format!("Failed to set broadcast: {}", e))?;

        let node = Self {
            socket: Arc::new(socket),
            peers: Arc::new(Mutex::new(HashMap::new())),
            my_files: Arc::new(Mutex::new(HashSet::new())),
        };

        // Start listening
        let node_clone = node.clone();
        tokio::spawn(async move {
            node_clone.listen_loop().await;
        });

        Ok(node)
    }

    async fn listen_loop(&self) {
        let mut buf = [0u8; 1024];
        loop {
            match self.socket.recv_from(&mut buf).await {
                Ok((len, addr)) => {
                    if let Ok(msg) = serde_json::from_slice::<P2PMessage>(&buf[..len]) {
                        self.handle_message(msg, addr).await;
                    }
                }
                Err(e) => eprintln!("[P2P] UDP Receive Error: {}", e),
            }
        }
    }

    async fn handle_message(&self, msg: P2PMessage, addr: SocketAddr) {
        // Don't process our own messages (approximate check by IP, though NAT/local might make this tricky)
        // Ideally we include a unique NodeID in message to filter self. 
        // For now, we'll just process everything.
        
        match msg {
            P2PMessage::WhoHas { url_hash } => {
                // Check if we have this file
                let has_file = {
                     self.my_files.lock().unwrap().contains(&url_hash)
                };

                if has_file {
                    // Respond with IHave
                    let response = P2PMessage::IHave {
                        url_hash,
                        http_port: P2P_HTTP_PORT,
                    };
                    if let Ok(data) = serde_json::to_vec(&response) {
                        // Send back to the requester
                        let _ = self.socket.send_to(&data, addr).await;
                    }
                }
            }
            P2PMessage::IHave { url_hash, http_port } => {
                // Record this peer
                let peer_ip = addr.ip();
                let peer_addr = SocketAddr::new(peer_ip, http_port);
                
                let mut peers = self.peers.lock().unwrap();
                peers.entry(url_hash.clone())
                    .or_default()
                    .insert(peer_addr);
                
                println!("[P2P] Found peer for {}: {}", url_hash, peer_addr);
            }
        }
    }

    pub async fn advertise_file(&self, url_hash: String) {
        {
            self.my_files.lock().unwrap().insert(url_hash.clone());
        }
        // Ideally we might broadcast "IHave" proactively, but "WhoHas" on demand is less spammy.
    }

    pub async fn find_peers(&self, url_hash: String) {
        println!("[P2P] Looking for peers for {}", url_hash);
        let msg = P2PMessage::WhoHas { url_hash };
        if let Ok(data) = serde_json::to_vec(&msg) {
            let broadcast_addr = SocketAddr::from(([255, 255, 255, 255], DISCOVERY_PORT));
            let _ = self.socket.send_to(&data, broadcast_addr).await;
        }
    }

    pub fn get_peers_for_hash(&self, url_hash: &str) -> Vec<String> {
        let peers = self.peers.lock().unwrap();
        if let Some(set) = peers.get(url_hash) {
            set.iter().map(|addr| format!("http://{}", addr)).collect()
        } else {
            Vec::new()
        }
    }
}
