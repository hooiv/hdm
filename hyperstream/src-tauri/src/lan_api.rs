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

    /// Generate a pairing code (6 digits)
    pub fn generate_pairing_code() -> String {
        use rand::Rng;
        let mut rng = rand::rng();
        format!("{:06}", rng.random_range(0..1000000))
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
        
        // Health check endpoint
        let health = warp::path!("api" / "health")
            .map(|| warp::reply::json(&ApiResponse::<()> {
                success: true,
                data: None,
                error: None,
            }));

        // Get downloads list
        let downloads = warp::path!("api" / "downloads")
            .and(warp::get())
            .map(|| {
                // TODO: Get actual downloads from state
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
                // TODO: Add download to queue
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
    // TODO: Verify pairing code
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
