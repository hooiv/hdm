use warp::Filter;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Debug, Deserialize)]
pub struct DownloadRequest {
    pub url: String,
    pub filename: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DownloadResponse {
    pub success: bool,
    pub message: String,
    pub id: Option<String>,
}

/// Channel sender type for communicating with the main app
pub type DownloadSender = mpsc::UnboundedSender<DownloadRequest>;

/// Start the HTTP server for browser extension communication
pub async fn start_server(tx: DownloadSender) {
    let tx = Arc::new(tx);
    
    // CORS headers for browser extension
    let cors = warp::cors()
        .allow_any_origin()
        .allow_methods(vec!["POST", "OPTIONS"])
        .allow_headers(vec!["Content-Type"]);

    // POST /download endpoint
    let download_route = warp::path("download")
        .and(warp::post())
        .and(warp::body::json())
        .and(with_sender(tx.clone()))
        .and_then(handle_download);

    // Health check endpoint
    let health_route = warp::path("health")
        .and(warp::get())
        .map(|| warp::reply::json(&serde_json::json!({"status": "ok", "app": "hyperstream"})));

    let routes = download_route.or(health_route).with(cors);

    println!("DEBUG: HTTP server starting on http://localhost:9876");
    warp::serve(routes).run(([127, 0, 0, 1], 9876)).await;
}

fn with_sender(
    tx: Arc<DownloadSender>,
) -> impl Filter<Extract = (Arc<DownloadSender>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || tx.clone())
}

async fn handle_download(
    req: DownloadRequest,
    tx: Arc<DownloadSender>,
) -> Result<impl warp::Reply, warp::Rejection> {
    println!("DEBUG: Received download request from extension: {}", req.url);
    
    // Generate a simple ID
    let id = format!("ext-{}", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis());
    
    // Send to main app
    match tx.send(req) {
        Ok(_) => {
            let response = DownloadResponse {
                success: true,
                message: "Download started".to_string(),
                id: Some(id),
            };
            Ok(warp::reply::json(&response))
        }
        Err(e) => {
            let response = DownloadResponse {
                success: false,
                message: format!("Failed to start download: {}", e),
                id: None,
            };
            Ok(warp::reply::json(&response))
        }
    }
}
