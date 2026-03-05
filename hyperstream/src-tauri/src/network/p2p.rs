use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_tungstenite::{accept_async, connect_async, tungstenite::Message};
use futures_util::{SinkExt, StreamExt};

// Magic wormhole-style wordlist (abbreviated - expand to 2048 words in production)
const WORDS: &[&str] = &[
    "brave", "tiger", "mountain", "ocean", "forest", "river", "eagle", "storm",
    "cloud", "sunrise", "sunset", "winter", "spring", "summer", "autumn", "galaxy",
    "comet", "nebula", "asteroid", "planet", "star", "moon", "crystal", "diamond",
    "phoenix", "dragon", "thunder", "lightning", "rainbow", "cascade", "volcano", "breeze",
    "falcon", "harbor", "meadow", "canyon", "glacier", "prairie", "tundra", "aurora",
    "zenith", "anchor", "beacon", "copper", "ember", "flint", "ivory", "jasper",
    "kindle", "lantern", "marble", "obsidian", "opal", "quartz", "ruby", "silver",
    "topaz", "velvet", "willow", "zephyr", "amber", "cobalt", "dusk", "frost",
    "granite", "haven", "indigo", "jade", "lapis", "magnet", "onyx", "pearl",
    "raven", "scholar", "timber", "umber", "vertex", "walnut", "coral", "delta",
    "echo", "forge", "grove", "hedge", "inlet", "jewel", "knoll", "lotus",
    "marsh", "nexus", "olive", "pebble", "ridge", "spruce", "tropic", "umbra",
    "vapor", "wren", "apex", "birch", "cedar", "drift", "elder", "fable",
    "garnet", "hazel", "iris", "juniper", "kelp", "linen", "mango", "niche",
    "orchid", "pine", "quill", "reed", "sage", "thorn", "urchin", "vine",
    "weave", "yucca", "zinc", "agate", "basalt", "clover", "dune", "elm",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2PShareSession {
    pub id: String,
    pub download_id: String,
    pub pairing_code: String,
    pub peers: Vec<String>,  // WebSocket URLs
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub created_at: u64,
    pub is_host: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2PStats {
    pub active_sessions: u32,
    pub total_peers: u32,
    pub bytes_sent_total: u64,
    pub bytes_received_total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerReputation {
    pub peer_id: String,
    pub successful_transfers: u32,
    pub failed_transfers: u32,
    pub average_speed_kbps: f64,
    pub last_seen: u64,
}

// P2P Protocol Messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum P2PMessage {
    // Discovery
    Announce { session_id: String, download_id: String, pairing_code: String },
    Join { pairing_code: String },
    JoinAccept { session_id: String, download_id: String },
    
    // File Transfer
    RequestRange { session_id: String, start: u64, end: u64 },
    RangeData { session_id: String, start: u64, data: Vec<u8> },
    RangeError { session_id: String, error: String },
    
    // Control
    Ping,
    Pong,
    Disconnect,
}

pub struct P2PNode {
    sessions: Arc<Mutex<HashMap<String, P2PShareSession>>>,
    pairing_registry: Arc<Mutex<HashMap<String, String>>>, // code -> session_id
    my_files: Arc<Mutex<HashSet<String>>>,
    reputation: Arc<Mutex<HashMap<String, PeerReputation>>>,
    ws_port: u16,
    upload_limiter: Arc<crate::speed_limiter::SpeedLimiter>,
}

impl P2PNode {
    /// Create a disabled P2PNode stub - no WebSocket server is started.
    /// Used as a fallback when all port attempts fail.
    pub fn disabled() -> Self {
        eprintln!("[P2P] Running in DISABLED mode — P2P features will not work.");
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            pairing_registry: Arc::new(Mutex::new(HashMap::new())),
            my_files: Arc::new(Mutex::new(HashSet::new())),
            reputation: Arc::new(Mutex::new(HashMap::new())),
            ws_port: 0,
            upload_limiter: Arc::new(crate::speed_limiter::SpeedLimiter::new()),
        }
    }

    pub async fn new(ws_port: u16) -> Result<Self, String> {
        // Load upload limit from settings
        let settings = crate::settings::load_settings();
        let upload_limiter = Arc::new(crate::speed_limiter::SpeedLimiter::new());
        if let Some(kbps) = settings.p2p_upload_limit_kbps {
            if kbps > 0 {
                upload_limiter.set_limit(kbps * 1024);
                println!("[P2P] Upload speed limit set to {} KB/s", kbps);
            }
        }

        let node = Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            pairing_registry: Arc::new(Mutex::new(HashMap::new())),
            my_files: Arc::new(Mutex::new(HashSet::new())),
            reputation: Arc::new(Mutex::new(HashMap::new())),
            ws_port,
            upload_limiter,
        };

        // Start WebSocket server
        let node_clone = node.clone();
        tokio::spawn(async move {
            if let Err(e) = node_clone.start_ws_server().await {
                eprintln!("[P2P] WebSocket server error: {}", e);
            }
        });

        // Start session cleanup task — evict sessions older than 24 hours every 10 minutes
        let cleanup_sessions = node.sessions.clone();
        let cleanup_pairing = node.pairing_registry.clone();
        tokio::spawn(async move {
            const MAX_AGE_SECS: u64 = 24 * 60 * 60;
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(600)).await;
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let mut sessions = cleanup_sessions.lock().unwrap_or_else(|e| e.into_inner());
                let expired: Vec<String> = sessions.iter()
                    .filter(|(_, s)| now.saturating_sub(s.created_at) > MAX_AGE_SECS)
                    .map(|(id, _)| id.clone())
                    .collect();
                for id in &expired {
                    sessions.remove(id);
                }
                if !expired.is_empty() {
                    let mut pairing = cleanup_pairing.lock().unwrap_or_else(|e| e.into_inner());
                    pairing.retain(|_, sid| !expired.contains(sid));
                    println!("[P2P] Cleaned up {} expired sessions", expired.len());
                }
            }
        });

        println!("[P2P] Node initialized on WebSocket port {}", ws_port);
        Ok(node)
    }

    async fn start_ws_server(&self) -> Result<(), String> {
        // Bind to all interfaces so LAN peers can connect
        let addr = format!("0.0.0.0:{}", self.ws_port);
        let listener = TcpListener::bind(&addr).await
            .map_err(|e| format!("Failed to bind WebSocket server: {}", e))?;
        
        println!("[P2P] WebSocket server listening on {}", addr);

        loop {
            match listener.accept().await {
                Ok((stream, peer_addr)) => {
                    println!("[P2P] New connection from {}", peer_addr);
                    let node = self.clone();
                    tokio::spawn(async move {
                        if let Err(e) = node.handle_connection(stream).await {
                            eprintln!("[P2P] Connection error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    eprintln!("[P2P] Accept error: {}", e);
                }
            }
        }
    }

    async fn handle_connection(&self, stream: TcpStream) -> Result<(), String> {
        let ws_stream = accept_async(stream).await
            .map_err(|e| format!("WebSocket handshake failed: {}", e))?;
        
        let (mut write, mut read) = ws_stream.split();

        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if let Ok(p2p_msg) = serde_json::from_str::<P2PMessage>(&text) {
                        if let Some(response) = self.handle_message(p2p_msg).await {
                            match serde_json::to_string(&response) {
                                Ok(response_text) => { let _ = write.send(Message::Text(response_text)).await; }
                                Err(e) => eprintln!("[P2P] Failed to serialize response: {}", e),
                            }
                        }
                    }
                }
                Ok(Message::Close(_)) => break,
                Err(e) => {
                    eprintln!("[P2P] WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }

    async fn handle_message(&self, msg: P2PMessage) -> Option<P2PMessage> {
        match msg {
            P2PMessage::Join { pairing_code } => {
                // Look up session by pairing code
                let session_id = self.pairing_registry.lock().unwrap_or_else(|e| e.into_inner())
                    .get(&pairing_code).cloned();
                
                if let Some(sid) = session_id {
                    let download_id = self.sessions.lock().unwrap_or_else(|e| e.into_inner())
                        .get(&sid)
                        .map(|s| s.download_id.clone());
                    
                    if let Some(did) = download_id {
                        println!("[P2P] Peer joined session {} via code {}", sid, pairing_code);
                        return Some(P2PMessage::JoinAccept {
                            session_id: sid,
                            download_id: did,
                        });
                    }
                }
                None
            }
            P2PMessage::RequestRange { session_id, start, end } => {
                println!("[P2P] RequestRange: session={}, range={}-{}", session_id, start, end);
                
                // Get download path from session
                let download_id = {
                    let sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
                    sessions.get(&session_id).map(|s| s.download_id.clone())
                };
                
                if let Some(did) = download_id {
                    // Validate download_id to prevent path traversal
                    if did.contains("..") || did.contains('/') || did.contains('\\') || did.contains(':') {
                        return Some(P2PMessage::RangeError {
                            session_id,
                            error: "Invalid download ID".to_string(),
                        });
                    }
                    
                    // Read file chunk from the user's configured download directory
                    let settings = crate::settings::load_settings();
                    let base_dir = std::path::PathBuf::from(&settings.download_dir);
                    let file_path = base_dir.join(&did);

                    // Canonicalize to ensure path stays within downloads directory
                    let canon_base = dunce::canonicalize(&base_dir).unwrap_or_else(|_| base_dir.to_path_buf());
                    let canon_file = dunce::canonicalize(&file_path).unwrap_or_else(|_| file_path.clone());
                    if !canon_file.starts_with(&canon_base) {
                        return Some(P2PMessage::RangeError {
                            session_id,
                            error: "Path traversal denied".to_string(),
                        });
                    }
                    
                    match tokio::fs::File::open(&file_path).await {
                        Ok(mut file) => {
                            if let Ok(metadata) = file.metadata().await {
                                let file_len = metadata.len();
                                if start >= file_len {
                                    return Some(P2PMessage::RangeError {
                                        session_id: session_id.clone(),
                                        error: format!("Range out of bounds: start={}, file_size={}", start, file_len),
                                    });
                                }

                                if end <= start {
                                    return Some(P2PMessage::RangeError {
                                        session_id: session_id.clone(),
                                        error: format!("Invalid range: end ({}) <= start ({})", end, start),
                                    });
                                }

                                // Cap chunk size to 4 MB to prevent OOM from malicious requests
                                const MAX_CHUNK_SIZE: u64 = 4 * 1024 * 1024;
                                let length = (end - start).min(MAX_CHUNK_SIZE).min(file_len - start);
                                let length_usize = length as usize;

                                if let Err(e) = file.seek(std::io::SeekFrom::Start(start)).await {
                                    return Some(P2PMessage::RangeError {
                                        session_id: session_id.clone(),
                                        error: format!("Seek error: {}", e),
                                    });
                                }

                                let mut buffer = vec![0u8; length_usize];
                                match file.read_exact(&mut buffer).await {
                                    Ok(_) => {
                                        
                                        // Apply upload speed limit (G1)
                                        let _allowed = self.upload_limiter.acquire(buffer.len() as u64).await;
                                        
                                        // Update bytes sent stats
                                        self.sessions.lock().unwrap_or_else(|e| e.into_inner())
                                            .get_mut(&session_id)
                                            .map(|s| s.bytes_sent += buffer.len() as u64);
                                        
                                        println!("[P2P] Sending {} bytes for range {}-{}", buffer.len(), start, end);
                                        return Some(P2PMessage::RangeData {
                                            session_id: session_id.clone(),
                                            start,
                                            data: buffer,
                                        });
                                    }
                                    Err(e) => {
                                        return Some(P2PMessage::RangeError {
                                            session_id: session_id.clone(),
                                            error: format!("Read error: {}", e),
                                        });
                                    }
                                }
                            } else {
                                return Some(P2PMessage::RangeError {
                                    session_id: session_id.clone(),
                                    error: "Failed to get file metadata".to_string(),
                                });
                            }
                        }
                        Err(e) => {
                            eprintln!("[P2P] File open error: {}", e);
                            return Some(P2PMessage::RangeError {
                                session_id: session_id.clone(),
                                error: format!("File not found or open error: {}", e),
                            });
                        }
                    }
                }
                None
            }
            P2PMessage::Ping => Some(P2PMessage::Pong),
            _ => None,
        }
    }

    // Generate magic-wormhole style pairing code
    pub fn generate_pairing_code() -> String {
        use rand::Rng;
        let mut rng = rand::rng();
        // Use 4 words from 128-word list for ~28 bits of entropy
        let words: Vec<_> = (0..4)
            .map(|_| WORDS[rng.random_range(0..WORDS.len())])
            .collect();
        words.join("-")
    }

    // Create a share session for a download
    pub async fn create_share_session(&self, download_id: String) -> Result<P2PShareSession, String> {
        let session_id = uuid::Uuid::new_v4().to_string();
        let pairing_code = Self::generate_pairing_code();
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let session = P2PShareSession {
            id: session_id.clone(),
            download_id: download_id.clone(),
            pairing_code: pairing_code.clone(),
            peers: Vec::new(),
            bytes_sent: 0,
            bytes_received: 0,
            created_at,
            is_host: true,
        };

        self.sessions.lock().unwrap_or_else(|e| e.into_inner()).insert(session_id.clone(), session.clone());
        self.pairing_registry.lock().unwrap_or_else(|e| e.into_inner()).insert(pairing_code.clone(), session_id.clone());
        self.my_files.lock().unwrap_or_else(|e| e.into_inner()).insert(download_id.clone());

        println!("[P2P] Created share session: {} with code: {}", session_id, pairing_code);
        Ok(session)
    }

    // Join a share session using pairing code and peer address
    pub async fn join_share_session(&self, code: String, peer_addr: String) -> Result<P2PShareSession, String> {
        // Connect to peer's WebSocket server
        let ws_url = format!("ws://{}/p2p", peer_addr);
        let (ws_stream, _) = connect_async(&ws_url).await
            .map_err(|e| format!("Failed to connect to peer: {}", e))?;

        let (mut write, mut read) = ws_stream.split();

        // Send Join message
        let join_msg = P2PMessage::Join { pairing_code: code.clone() };
        let join_text = serde_json::to_string(&join_msg)
            .map_err(|e| format!("Failed to serialize join message: {}", e))?;
        write.send(Message::Text(join_text)).await
            .map_err(|e| format!("Failed to send join: {}", e))?;

        // Wait for JoinAccept
        if let Some(Ok(Message::Text(response))) = read.next().await {
            if let Ok(P2PMessage::JoinAccept { session_id, download_id }) = serde_json::from_str(&response) {
                let session = P2PShareSession {
                    id: session_id.clone(),
                    download_id: download_id.clone(),
                    pairing_code: code,
                    peers: vec![peer_addr],
                    bytes_sent: 0,
                    bytes_received: 0,
                    created_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
                    is_host: false,
                };
                
                self.sessions.lock().unwrap_or_else(|e| e.into_inner()).insert(session_id.clone(), session.clone());
                println!("[P2P] Joined session: {}", session_id);
                return Ok(session);
            }
        }

        Err("Failed to join session".to_string())
    }

    // Get all active sessions
    pub fn list_sessions(&self) -> Vec<P2PShareSession> {
        self.sessions.lock().unwrap_or_else(|e| e.into_inner()).values().cloned().collect()
    }

    // Close a session
    pub fn close_session(&self, session_id: &str) -> Result<(), String> {
        if let Some(session) = self.sessions.lock().unwrap_or_else(|e| e.into_inner()).remove(session_id) {
            self.pairing_registry.lock().unwrap_or_else(|e| e.into_inner()).remove(&session.pairing_code);
            println!("[P2P] Closed session: {}", session_id);
            Ok(())
        } else {
            Err("Session not found".to_string())
        }
    }

    // Get P2P statistics
    pub fn get_stats(&self) -> P2PStats {
        let sessions = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        let total_sent: u64 = sessions.values().map(|s| s.bytes_sent).sum();
        let total_received: u64 = sessions.values().map(|s| s.bytes_received).sum();
        let total_peers: usize = sessions.values()
            .flat_map(|s| s.peers.iter())
            .collect::<HashSet<_>>()
            .len();

        P2PStats {
            active_sessions: sessions.len() as u32,
            total_peers: total_peers as u32,
            bytes_sent_total: total_sent,
            bytes_received_total: total_received,
        }
    }

    // Get peer reputation
    pub fn get_reputation(&self, peer_id: &str) -> Option<PeerReputation> {
        self.reputation.lock().unwrap_or_else(|e| e.into_inner()).get(peer_id).cloned()
    }

    /// Set upload speed limit in KB/s (0 = unlimited)
    pub fn set_upload_limit(&self, kbps: u64) {
        self.upload_limiter.set_limit(kbps * 1024);
        println!("[P2P] Upload limit set to {} KB/s", kbps);
    }

    /// Get current upload speed limit in KB/s
    pub fn get_upload_limit(&self) -> u64 {
        self.upload_limiter.get_limit() / 1024
    }
}

// Clone implementation for tokio::spawn
impl Clone for P2PNode {
    fn clone(&self) -> Self {
        Self {
            sessions: Arc::clone(&self.sessions),
            pairing_registry: Arc::clone(&self.pairing_registry),
            my_files: Arc::clone(&self.my_files),
            reputation: Arc::clone(&self.reputation),
            ws_port: self.ws_port,
            upload_limiter: Arc::clone(&self.upload_limiter),
        }
    }
}
