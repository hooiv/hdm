use warp::Filter;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;
use std::collections::HashMap;


use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::io::{AsyncRead, AsyncSeek};
use tokio_util::io::ReaderStream;
use crate::network::bittorrent::manager::TorrentManager;

#[derive(Debug, Clone)]
pub enum StreamingSource {
    FileSystem(PathBuf),
    Torrent { torrent_id: usize, file_id: usize },
}

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

pub type DownloadSender = mpsc::UnboundedSender<DownloadRequest>;
pub type FileMap = Arc<std::sync::Mutex<HashMap<String, StreamingSource>>>;

pub async fn start_server(tx: DownloadSender, file_map: FileMap, torrent_manager: Arc<TorrentManager>) {
    let tx = Arc::new(tx);
    let torrent_manager = torrent_manager.clone();
    
    let cors = warp::cors()
        .allow_any_origin()
        .allow_methods(vec!["POST", "OPTIONS"])
        .allow_headers(vec!["Content-Type"]);

    let download_route = warp::path("download")
        .and(warp::post())
        .and(warp::body::json())
        .and(with_sender(tx.clone()))
        .and_then(handle_download);

    let health_route = warp::path("health")
        .and(warp::get())
        .map(|| warp::reply::json(&serde_json::json!({"status": "ok", "app": "hyperstream"})));

    let file_map_filter = warp::any().map(move || file_map.clone());
    let tm_filter = warp::any().map(move || torrent_manager.clone());

    let p2p_route = warp::path!("p2p" / String)
        .and(warp::get())
        .and(warp::header::optional::<String>("range"))
        .and(file_map_filter)
        .and(tm_filter)
        .and_then(handle_p2p_request);

    let routes = download_route.or(health_route).or(p2p_route).with(cors);

    println!("DEBUG: HTTP server starting on http://localhost:14733");
    warp::serve(routes).run(([0, 0, 0, 0], 14733)).await; 
}

// Helper to create error response
fn error_response(code: warp::http::StatusCode) -> warp::http::Response<warp::hyper::Body> {
    warp::http::Response::builder()
        .status(code)
        .body(warp::hyper::Body::empty())
        .unwrap()
}

async fn handle_p2p_request(
    id: String,
    range_header: Option<String>,
    file_map: FileMap,
    torrent_manager: Arc<TorrentManager>,
) -> Result<warp::http::Response<warp::hyper::Body>, warp::Rejection> {
    let source = {
        let map = file_map.lock().unwrap();
        map.get(&id).cloned()
    };

    match source {
        Some(StreamingSource::FileSystem(path)) => {
            serve_file(path, range_header).await
        }
        Some(StreamingSource::Torrent { torrent_id, file_id }) => {
            serve_torrent_stream(torrent_manager, torrent_id, file_id, range_header).await
        }
        None => Ok(error_response(warp::http::StatusCode::NOT_FOUND)),
    }
}

async fn serve_file(path: PathBuf, range_header: Option<String>) -> Result<warp::http::Response<warp::hyper::Body>, warp::Rejection> {
     if !path.exists() {
         return Ok(error_response(warp::http::StatusCode::NOT_FOUND));
     }
     let file = match File::open(&path).await {
         Ok(f) => f,
         Err(_) => return Ok(error_response(warp::http::StatusCode::INTERNAL_SERVER_ERROR)),
     };
     let file_len = file.metadata().await.map(|m| m.len()).unwrap_or(0);
     handle_range_request(file, file_len, range_header).await
}

async fn serve_torrent_stream(tm: Arc<TorrentManager>, tid: usize, fid: usize, range_header: Option<String>) -> Result<warp::http::Response<warp::hyper::Body>, warp::Rejection> {
    let stream = match tm.create_stream(tid, fid) {
       Ok(s) => s,
       Err(_) => return Ok(error_response(warp::http::StatusCode::INTERNAL_SERVER_ERROR)),
    };
    
    // Get actual file length
    let file_len = tm.get_file_length(tid, fid).unwrap_or(0);
    
    handle_range_request(stream, file_len, range_header).await
}

async fn handle_range_request<T>(mut source: T, file_len: u64, range_header: Option<String>) -> Result<warp::http::Response<warp::hyper::Body>, warp::Rejection> 
where T: AsyncRead + AsyncSeek + Unpin + Send + 'static 
{
    if let Some(range) = range_header {
        if let Some(range_str) = range.strip_prefix("bytes=") {
            let parts: Vec<&str> = range_str.split('-').collect();
            if parts.len() == 2 {
                let start: u64 = parts[0].parse().unwrap_or(0);
                let end_parsed: Option<u64> = parts[1].parse().ok();
                let end = end_parsed.unwrap_or(file_len - 1);
                
                let length = end - start + 1;
                
                if let Err(_) = source.seek(std::io::SeekFrom::Start(start)).await {
                     return Ok(error_response(warp::http::StatusCode::INTERNAL_SERVER_ERROR));
                }
                
                let stream = ReaderStream::new(source.take(length));
                let body = warp::hyper::Body::wrap_stream(stream);
                
                let response = warp::http::Response::builder()
                        .status(warp::http::StatusCode::PARTIAL_CONTENT)
                        .header("Content-Range", format!("bytes {}-{}/{}", start, end, file_len))
                        .header("Content-Length", length)
                        .header("Content-Type", "application/octet-stream")
                        .header("Accept-Ranges", "bytes")
                        .body(body)
                        .unwrap();
                return Ok(response);
            }
        }
    }
    
    // Full content
    let stream = ReaderStream::new(source);
    let body = warp::hyper::Body::wrap_stream(stream);
    Ok(warp::http::Response::builder()
        .status(warp::http::StatusCode::OK)
        .body(body)
        .unwrap())
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
    
    let id = format!("ext-{}", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis());
    
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
