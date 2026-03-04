use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};
use warp::Filter;

/// LAN API Server for mobile app integration
#[allow(dead_code)]
pub struct LanApiServer {
    port: u16,
    paired_devices: Arc<RwLock<Vec<PairedDevice>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct PairedDevice {
    pub id: String,
    pub name: String,
    pub paired_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct DownloadRequest {
    pub url: String,
    pub filename: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl LanApiServer {
    pub fn new(port: u16) -> Self {
        Self {
            port,
            paired_devices: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Generate a pairing code (8 alphanumeric chars) and store it for verification
    pub fn generate_pairing_code() -> String {
        use rand::Rng;
        let mut rng = rand::rng();
        // Use 8 alphanumeric characters (~2.8 trillion possibilities) instead of 6 digits (1M)
        let charset: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789"; // no 0/O/1/I to avoid confusion
        let code: String = (0..8)
            .map(|_| charset[rng.random_range(0..charset.len())] as char)
            .collect();
        // Store the code synchronously for immediate availability
        if let Ok(mut guard) = CURRENT_PAIRING_CODE.write() {
            *guard = Some(code.clone());
        }
        code
    }

    /// Generate QR code data for pairing
    pub fn get_pairing_qr_data(&self, pairing_code: &str) -> String {
        // Get local IP address
        let local_ip = local_ip_address::local_ip()
            .map(|ip| ip.to_string())
            .unwrap_or_else(|_| "127.0.0.1".to_string());
        
        // QR code contains: hyperstream://<ip>:<port>?code=<pairing_code>
        format!("hyperstream://{}:{}?code={}", local_ip, self.port, pairing_code)
    }

    /// Start the API server
    pub async fn start(&self) -> Result<(), String> {
        let devices = self.paired_devices.clone();

        // Broadcast Channel for Sync
        let (tx, _rx) = tokio::sync::broadcast::channel::<String>(100);
        let tx = Arc::new(tx);

        // Store tx in a global/static or pass it to state so lib.rs can access it?
        // Ideally, we'd store it in AppState. For now, we'll put it in a global for easy access by `add_download`.
        *crate::lan_api::BROADCAST_TX.write().await = Some(tx.clone());
        
        // Health check
        let health = warp::path!("api" / "health")
            .map(|| warp::reply::json(&ApiResponse::<()> {
                success: true,
                data: None,
                error: None,
            }));

        // Sync WebSocket (auth via query param since WS can't use headers easily)
        // /api/sync?token=<TOKEN>
        let sync_route = warp::path!("api" / "sync")
            .and(warp::ws())
            .and(warp::query::<std::collections::HashMap<String, String>>())
            .and(warp::any().map(move || tx.clone()))
            .map(|ws: warp::ws::Ws, params: std::collections::HashMap<String, String>, tx: Arc<tokio::sync::broadcast::Sender<String>>| {
                let token_valid = params.get("token").map_or(false, |t| {
                    PAIRED_TOKENS.read().unwrap_or_else(|e| e.into_inner()).contains(t.as_str())
                });
                ws.on_upgrade(move |socket| async move {
                    if !token_valid {
                        // Close unauthorized connections immediately
                        drop(socket);
                    } else {
                        handle_sync_socket(socket, tx).await;
                    }
                })
            });

        // Get downloads list (requires auth)
        let downloads = warp::path!("api" / "downloads")
            .and(warp::get())
            .and(require_auth())
            .map(|_| {
                warp::reply::json(&ApiResponse {
                    success: true,
                    data: Some(Vec::<String>::new()),
                    error: None,
                })
            });

        // Add new download (requires auth)
        let add_download = warp::path!("api" / "downloads")
            .and(warp::post())
            .and(require_auth())
            .and(warp::body::content_length_limit(64 * 1024))
            .and(warp::body::json())
            .map(|_, req: DownloadRequest| {
                println!("API: New download request: {}", req.url);
                // Actually enqueue the download via broadcast
                broadcast_download(req.url.clone());
                warp::reply::json(&ApiResponse {
                    success: true,
                    data: Some("Download added"),
                    error: None,
                })
            });

        // Pair device
        let pair = warp::path!("api" / "pair")
            .and(warp::post())
            .and(warp::body::content_length_limit(16 * 1024))
            .and(warp::body::json())
            .and(with_devices(devices.clone()))
            .and_then(handle_pair);

        let routes = health
            .or(sync_route)
            .or(downloads)
            .or(add_download)
            .or(pair)
            .with(warp::cors().allow_any_origin());

        let addr: SocketAddr = format!("0.0.0.0:{}", self.port).parse().unwrap();
        println!("LAN API server starting on {}", addr);

        warp::serve(routes).run(addr).await;
        Ok(())
    }
}

// Global Broadcast Sender
use lazy_static::lazy_static;
lazy_static! {
    pub static ref BROADCAST_TX: tokio::sync::RwLock<Option<std::sync::Arc<tokio::sync::broadcast::Sender<String>>>> = tokio::sync::RwLock::new(None);
    pub static ref CURRENT_PAIRING_CODE: std::sync::RwLock<Option<String>> = std::sync::RwLock::new(None);
    /// Tokens issued during pairing — required for all protected routes
    static ref PAIRED_TOKENS: std::sync::RwLock<std::collections::HashSet<String>> = std::sync::RwLock::new(std::collections::HashSet::new());
}

/// Warp filter that rejects requests without a valid paired auth token.
fn require_auth() -> impl Filter<Extract = ((),), Error = warp::Rejection> + Clone {
    warp::header::optional::<String>("authorization")
        .and_then(|auth_header: Option<String>| async move {
            let token = auth_header
                .as_deref()
                .and_then(|h| h.strip_prefix("Bearer "));
            match token {
                Some(t) => {
                    let tokens = PAIRED_TOKENS.read().unwrap_or_else(|e| e.into_inner());
                    if tokens.contains(t) {
                        Ok(())
                    } else {
                        Err(warp::reject::reject())
                    }
                }
                None => Err(warp::reject::reject()),
            }
        })
}

pub fn broadcast_download(url: String) {
    tokio::spawn(async move {
        if let Some(tx) = BROADCAST_TX.read().await.as_ref() {
            let msg = serde_json::json!({
                "event_type": "ADD_DOWNLOAD",
                "payload": url
            }).to_string();
            let _ = tx.send(msg);
        }
    });
}

async fn handle_sync_socket(ws: warp::ws::WebSocket, tx: Arc<tokio::sync::broadcast::Sender<String>>) {
    use futures_util::{StreamExt, SinkExt};
    let (mut user_ws_tx, mut user_ws_rx) = ws.split();
    let mut rx = tx.subscribe();

    tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if let Err(_) = user_ws_tx.send(warp::ws::Message::text(msg)).await {
                break;
            }
        }
    });

    while let Some(_result) = user_ws_rx.next().await {
        // Just keep connection open
    }
}

fn with_devices(
    devices: Arc<RwLock<Vec<PairedDevice>>>,
) -> impl Filter<Extract = (Arc<RwLock<Vec<PairedDevice>>>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || devices.clone())
}

#[derive(Debug, Deserialize)]
struct PairRequest {
    #[allow(dead_code)]
    code: String,
    device_name: String,
}

async fn handle_pair(
    req: PairRequest,
    devices: Arc<RwLock<Vec<PairedDevice>>>,
) -> Result<impl warp::Reply, warp::Rejection> {
    // Verify and atomically invalidate the pairing code
    {
        let mut code_guard = CURRENT_PAIRING_CODE.write().unwrap_or_else(|e| e.into_inner());
        match code_guard.as_deref() {
            Some(expected) if expected == req.code => {
                // Code matches — invalidate immediately to prevent reuse
                *code_guard = None;
            }
            _ => {
                return Ok(warp::reply::json(&ApiResponse::<serde_json::Value> {
                    success: false,
                    data: None,
                    error: Some("Invalid pairing code".to_string()),
                }));
            }
        }
    }

    // Generate an auth token for this device
    let auth_token = uuid::Uuid::new_v4().to_string();
    if let Ok(mut tokens) = PAIRED_TOKENS.write() {
        // Cap the number of paired devices to prevent unbounded growth
        const MAX_PAIRED: usize = 20;
        if tokens.len() >= MAX_PAIRED {
            return Ok(warp::reply::json(&ApiResponse::<serde_json::Value> {
                success: false,
                data: None,
                error: Some(format!("Maximum paired devices ({}) reached. Unpair a device first.", MAX_PAIRED)),
            }));
        }
        tokens.insert(auth_token.clone());
    }

    let device = PairedDevice {
        id: uuid::Uuid::new_v4().to_string(),
        name: req.device_name,
        paired_at: chrono::Local::now().to_rfc3339(),
    };

    devices.write().await.push(device.clone());

    Ok(warp::reply::json(&ApiResponse {
        success: true,
        data: Some(serde_json::json!({
            "device": device,
            "token": auth_token
        })),
        error: None,
    }))
}
