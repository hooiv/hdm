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

    /// Generate a pairing code (6 digits) and store it for verification
    pub fn generate_pairing_code() -> String {
        use rand::Rng;
        let mut rng = rand::rng();
        let code = format!("{:06}", rng.random_range(0..1000000));
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

        // Sync WebSocket
        // /api/sync
        let sync_route = warp::path!("api" / "sync")
            .and(warp::ws())
            .and(warp::any().map(move || tx.clone()))
            .map(|ws: warp::ws::Ws, tx: Arc<tokio::sync::broadcast::Sender<String>>| {
                ws.on_upgrade(move |socket| handle_sync_socket(socket, tx))
            });

        // Get downloads list
        let downloads = warp::path!("api" / "downloads")
            .and(warp::get())
            .map(|| {
                warp::reply::json(&ApiResponse {
                    success: true,
                    data: Some(Vec::<String>::new()),
                    error: None,
                })
            });

        // Add new download
        let add_download = warp::path!("api" / "downloads")
            .and(warp::post())
            .and(warp::body::json())
            .map(|req: DownloadRequest| {
                println!("API: New download request: {}", req.url);
                warp::reply::json(&ApiResponse {
                    success: true,
                    data: Some("Download added"),
                    error: None,
                })
            });

        // Pair device
        let pair = warp::path!("api" / "pair")
            .and(warp::post())
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
    // Verify pairing code
    {
        let valid_code = CURRENT_PAIRING_CODE.read().unwrap();
        match valid_code.as_deref() {
            Some(expected) if expected == req.code => {
                // Code matches — proceed
            }
            _ => {
                return Ok(warp::reply::json(&ApiResponse::<PairedDevice> {
                    success: false,
                    data: None,
                    error: Some("Invalid pairing code".to_string()),
                }));
            }
        }
    }

    // Invalidate the code after successful pairing
    if let Ok(mut guard) = CURRENT_PAIRING_CODE.write() {
        *guard = None;
    }

    let device = PairedDevice {
        id: uuid::Uuid::new_v4().to_string(),
        name: req.device_name,
        paired_at: chrono::Local::now().to_rfc3339(),
    };

    devices.write().await.push(device.clone());

    Ok(warp::reply::json(&ApiResponse {
        success: true,
        data: Some(device),
        error: None,
    }))
}
