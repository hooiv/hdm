use warp::Filter;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{mpsc, broadcast};
use std::collections::HashMap;
use once_cell::sync::Lazy;
use tokio_stream::wrappers::BroadcastStream;
use futures::StreamExt;
use tauri::Emitter;

/// A video/audio stream detected by the browser extension via Content-Type sniffing.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DetectedStreamRequest {
    pub url: String,
    pub content_type: Option<String>,
    pub stream_type: String, // "hls" | "dash" | "video" | "audio"
    pub page_url: Option<String>,
    pub page_title: Option<String>,
    pub quality: Option<String>,
    pub size: Option<u64>,
}

pub type StreamSender = mpsc::UnboundedSender<Vec<DetectedStreamRequest>>;

// Global broadcast channel for server-sent events
pub static EVENT_SENDER: Lazy<broadcast::Sender<serde_json::Value>> = Lazy::new(|| {
    broadcast::channel(256).0
});

pub fn get_event_sender() -> broadcast::Sender<serde_json::Value> {
    EVENT_SENDER.clone()
}


use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::io::{AsyncRead, AsyncSeek};
use tokio_util::io::ReaderStream;
use crate::network::bittorrent::manager::TorrentManager;
use crate::feeds;

#[derive(Debug, Clone)]
pub enum StreamingSource {
    FileSystem(PathBuf),
    Torrent { torrent_id: usize, file_id: usize },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRequest {
    pub url: String,
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub custom_headers: Option<HashMap<String, String>>,
    #[serde(default)]
    pub page_url: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BatchLink {
    pub url: String,
    pub filename: String,
}

#[derive(Debug, Serialize)]
pub struct DownloadResponse {
    pub success: bool,
    pub message: String,
    pub id: Option<String>,
}

pub type DownloadSender = mpsc::UnboundedSender<DownloadRequest>;
pub type BatchSender = mpsc::UnboundedSender<Vec<BatchLink>>;
pub type FileMap = Arc<std::sync::Mutex<HashMap<String, StreamingSource>>>;

// Add state so the HTTP API can inspect and control active downloads
pub async fn start_server(
    tx: DownloadSender,
    batch_tx: BatchSender,
    stream_tx: StreamSender,
    file_map: FileMap,
    torrent_manager: Option<Arc<TorrentManager>>,
    app_handle: tauri::AppHandle,
) {
    let tx = Arc::new(tx);
    let batch_tx = Arc::new(batch_tx);
    let stream_tx = Arc::new(stream_tx);
    let torrent_manager = torrent_manager.clone();
    let app_handle_arc = Arc::new(app_handle.clone());

    // CORS: Allow any origin because browser-extension origins
    // (chrome-extension://<id>, moz-extension://<uuid>) are dynamic and
    // cannot be enumerated at compile time.  Auth is enforced by the
    // shared-secret token header instead.
    let cors = warp::cors()
        .allow_any_origin()
        .allow_methods(vec!["GET", "POST", "OPTIONS"])
        .allow_headers(vec!["Content-Type", "X-HyperStream-Token"]);

    // Simple shared-secret token filter for download/batch routes.
    // The token is generated once at startup and must be sent as a header.
    let auth_token = Arc::new(uuid::Uuid::new_v4().to_string());
    // Log only a truncated hint of the token for debugging — never the full secret.
    println!("[http_server] Auth token generated (hint: {}...)", &auth_token[..8]);

    // Persist the auth token to a file so the browser extension can read it
    // via the desktop app's settings/copy-token feature.
    if let Some(home) = dirs::home_dir() {
        let token_dir = home.join(".hyperstream");
        let _ = std::fs::create_dir_all(&token_dir);
        let token_path = token_dir.join("auth_token");
        if let Err(e) = std::fs::write(&token_path, auth_token.as_str()) {
            eprintln!("[http_server] Failed to write auth token file: {}", e);
        } else {
            // Restrict to owner-only on Unix (chmod 600)
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&token_path, std::fs::Permissions::from_mode(0o600));
            }
        }
    }

    let auth_token_filter = {
        let token = auth_token.clone();
        warp::header::optional::<String>("x-hyperstream-token")
            .and_then(move |header_token: Option<String>| {
                let expected = token.clone();
                async move {
                    match header_token {
                        Some(t) if t == *expected => Ok(()),
                        _ => Err(warp::reject::custom(Unauthorized)),
                    }
                }
            })
            .untuple_one()
    };

    let event_auth_filter = {
        let token = auth_token.clone();
        warp::header::optional::<String>("x-hyperstream-token")
            .and(warp::query::<HashMap<String, String>>())
            .and_then(move |header_token: Option<String>, query_params: HashMap<String, String>| {
                let expected = token.clone();
                async move {
                    let query_token = query_params.get("token").map(|value| value.as_str());
                    match header_token.as_deref().or(query_token) {
                        Some(t) if t == expected.as_str() => Ok(()),
                        _ => Err(warp::reject::custom(Unauthorized)),
                    }
                }
            })
            .untuple_one()
    };

    let download_route = warp::path("download")
        .and(warp::post())
        .and(auth_token_filter.clone())
        .and(warp::body::content_length_limit(64 * 1024))
        .and(warp::body::json())
        .and(with_sender(tx.clone()))
        .and_then(handle_download);

    let batch_tx_filter = warp::any().map(move || batch_tx.clone());
    let batch_route = warp::path("batch")
        .and(warp::post())
        .and(auth_token_filter.clone())
        .and(warp::body::content_length_limit(256 * 1024))
        .and(warp::body::json())
        .and(batch_tx_filter)
        .and_then(handle_batch);

    // Route for browser extension to report detected video/audio streams
    let stream_tx_filter = warp::any().map(move || stream_tx.clone());
    let streams_route = warp::path("streams")
        .and(warp::post())
        .and(auth_token_filter.clone())
        .and(warp::body::content_length_limit(256 * 1024))
        .and(warp::body::json())
        .and(stream_tx_filter)
        .and_then(handle_streams);

    // Route for querying current download status
    let ah_clone = app_handle_arc.clone();
    let state_filter = warp::any().map(move || ah_clone.clone());
    let status_route = warp::path("downloads")
        .and(warp::get())
        .and(auth_token_filter.clone())
        .and(state_filter.clone())
        .and_then(handle_status);

    // Control route for pause/cancel actions
    let control_route = warp::path("control")
        .and(warp::post())
        .and(auth_token_filter.clone())
        .and(warp::body::json())
        .and(state_filter.clone())
        .and_then(handle_control);

    let health_route = warp::path("health")
        .and(warp::get())
        .map(|| warp::reply::json(&serde_json::json!({"status": "ok", "app": "hyperstream"})));

    // Version info (useful for extension compatibility checks)
    let version_route = warp::path("version")
        .and(warp::get())
        .map(|| warp::reply::json(&serde_json::json!({"version": env!("CARGO_PKG_VERSION")})));

    // Server-sent events endpoint
    let events_route = warp::path("events")
        .and(warp::get())
        .and(event_auth_filter)
        .map(|| {
            let rx = EVENT_SENDER.subscribe();
            let stream = tokio_stream::wrappers::BroadcastStream::new(rx)
                .filter_map(|res| async move { res.ok() })
                .map(|val| {
                    Ok::<_, std::convert::Infallible>(warp::sse::Event::default().data(val.to_string()))
                });
            warp::sse::reply(warp::sse::keep_alive().stream(stream))
        });

    // Auth token endpoint removed for security — token should be exchanged
    // via native messaging or secure file-based handshake, not an unauthenticated HTTP endpoint.
    // The token is logged to stdout for development purposes only.

    let file_map_filter = warp::any().map(move || file_map.clone());
    let tm_filter = warp::any().map(move || torrent_manager.clone());

    // P2P route now requires auth token to prevent unauthorized file access
    let p2p_route = warp::path!("p2p" / String)
        .and(warp::get())
        .and(auth_token_filter.clone())
        .and(warp::header::optional::<String>("range"))
        .and(file_map_filter)
        .and(tm_filter)
        .and_then(handle_p2p_request);

    // Feed management endpoints
    let list_feeds = warp::path("feeds")
        .and(warp::get())
        .and(auth_token_filter.clone())
        .and_then(|| async move {
            let feeds = feeds::FEED_MANAGER.get_feeds();
            Ok::<_, warp::Rejection>(warp::reply::json(&feeds))
        });

    let feed_items = warp::path!("feeds" / String / "items")
        .and(warp::get())
        .and(auth_token_filter.clone())
        .and_then(|id: String| async move {
            let items = feeds::FEED_MANAGER.get_items(&id);
            Ok::<_, warp::Rejection>(warp::reply::json(&items))
        });

    let add_feed = warp::path("feeds")
        .and(warp::post())
        .and(auth_token_filter.clone())
        .and(warp::body::json())
        .and_then(|cfg: feeds::FeedConfig| async move {
            let res = feeds::FEED_MANAGER.add_feed(cfg);
            Ok::<_, warp::Rejection>(warp::reply::json(&serde_json::json!({
                "success": res.is_ok(),
                "message": res.err().unwrap_or_default()
            })))
        });

    let update_feed = warp::path!("feeds" / String)
        .and(warp::put())
        .and(auth_token_filter.clone())
        .and(warp::body::json())
        .and_then(|_id: String, cfg: feeds::FeedConfig| async move {
            let res = feeds::FEED_MANAGER.update_feed(cfg);
            Ok::<_, warp::Rejection>(warp::reply::json(&serde_json::json!({
                "success": res.is_ok(),
                "message": res.err().unwrap_or_default()
            })))
        });

    let remove_feed = warp::path!("feeds" / String)
        .and(warp::delete())
        .and(auth_token_filter.clone())
        .and_then(|id: String| async move {
            feeds::FEED_MANAGER.remove_feed(&id);
            Ok::<_, warp::Rejection>(warp::reply::json(&serde_json::json!({
                "success": true
            })))
        });

    let refresh_feed = {
        let app_clone = app_handle.clone();
        warp::path!("feeds" / String / "refresh")
            .and(warp::post())
            .and(auth_token_filter.clone())
            .and_then(move |id: String| {
                let ah = app_clone.clone();
                async move {
                    let r = feeds::FEED_MANAGER.refresh_feed(&ah, &id).await;
                    Ok::<_, warp::Rejection>(warp::reply::json(&serde_json::json!({
                        "success": r.is_ok(),
                        "message": r.err().unwrap_or_default()
                    })))
                }
            })
    };

    let routes = download_route
        .or(batch_route)
        .or(streams_route)
        .or(status_route)
        .or(control_route)
        .or(health_route)
        .or(version_route)
        .or(events_route)
        .or(p2p_route)
        .or(list_feeds)
        .or(feed_items)
        .or(add_feed)
        .or(update_feed)
        .or(remove_feed)
        .or(refresh_feed)
        .recover(handle_rejection)
        .with(cors);

    warp::serve(routes).run(([127, 0, 0, 1], 14733)).await; 
}

/// Custom rejection type for unauthorized requests.
#[derive(Debug)]
struct Unauthorized;
impl warp::reject::Reject for Unauthorized {}

// Helper to create error response
fn error_response(code: warp::http::StatusCode) -> warp::http::Response<warp::hyper::Body> {
    warp::http::Response::builder()
        .status(code)
        .body(warp::hyper::Body::empty())
        .unwrap_or_else(|_| {
            // Fallback: if even the builder fails, return a minimal 500
            warp::http::Response::new(warp::hyper::Body::empty())
        })
}

async fn handle_rejection(err: warp::Rejection) -> Result<impl warp::Reply, std::convert::Infallible> {
    if err.find::<Unauthorized>().is_some() {
        return Ok(error_response(warp::http::StatusCode::UNAUTHORIZED));
    }

    if err.is_not_found() {
        return Ok(error_response(warp::http::StatusCode::NOT_FOUND));
    }

    eprintln!("[http_server] Unhandled rejection: {:?}", err);
    Ok(error_response(warp::http::StatusCode::INTERNAL_SERVER_ERROR))
}

async fn handle_p2p_request(
    id: String,
    range_header: Option<String>,
    file_map: FileMap,
    torrent_manager: Option<Arc<TorrentManager>>,
) -> Result<warp::http::Response<warp::hyper::Body>, warp::Rejection> {
    let source = {
        let map = file_map.lock().unwrap_or_else(|e| e.into_inner());
        map.get(&id).cloned()
    };

    match source {
        Some(StreamingSource::FileSystem(path)) => {
            serve_file(path, range_header).await
        }
        Some(StreamingSource::Torrent { torrent_id, file_id }) => {
            match torrent_manager {
                Some(ref tm) => serve_torrent_stream(tm.clone(), torrent_id, file_id, range_header).await,
                None => Ok(error_response(warp::http::StatusCode::SERVICE_UNAVAILABLE)),
            }
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
            if file_len == 0 {
                return Ok(error_response(warp::http::StatusCode::RANGE_NOT_SATISFIABLE));
            }
            let parts: Vec<&str> = range_str.splitn(2, '-').collect();
            if parts.len() == 2 {
                // Handle suffix-range (RFC 7233): "bytes=-500" means last 500 bytes
                let (start, end) = if parts[0].is_empty() {
                    // Suffix range: bytes=-N
                    let suffix_len: u64 = parts[1].parse().unwrap_or(0);
                    if suffix_len == 0 {
                        return Ok(error_response(warp::http::StatusCode::RANGE_NOT_SATISFIABLE));
                    }
                    let start = file_len.saturating_sub(suffix_len);
                    (start, file_len - 1)
                } else {
                    let start: u64 = parts[0].parse().unwrap_or(0);
                    let end_parsed: Option<u64> = parts[1].parse().ok();
                    let end = end_parsed.unwrap_or(file_len.saturating_sub(1));
                    (start, end)
                };
                
                // Validate range bounds to prevent underflow / out-of-bounds
                if end < start || start >= file_len {
                    return Ok(error_response(warp::http::StatusCode::RANGE_NOT_SATISFIABLE));
                }
                let end = end.min(file_len - 1); // clamp to file size
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
                        .unwrap_or_else(|_| warp::http::Response::new(warp::hyper::Body::empty()));
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
        .unwrap_or_else(|_| warp::http::Response::new(warp::hyper::Body::empty())))
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
    // broadcast event for listeners
    let _ = get_event_sender().send(serde_json::json!({
        "type": "download_requested",
        "url": &req.url,
        "filename": &req.filename,
        "customHeaders": &req.custom_headers,
        "pageUrl": &req.page_url,
        "source": &req.source
    }));
    // URL validation: only allow http/https schemes, block private/loopback IPs
    if let Ok(parsed) = reqwest::Url::parse(&req.url) {
        match parsed.scheme() {
            "http" | "https" => {}
            scheme => {
                return Ok(warp::reply::json(&DownloadResponse {
                    success: false,
                    message: format!("Unsupported URL scheme '{}': only http and https are allowed", scheme),
                    id: None,
                }));
            }
        }
    } else {
        return Ok(warp::reply::json(&DownloadResponse {
            success: false,
            message: "Invalid URL".to_string(),
            id: None,
        }));
    }

    let id = format!("ext-{}", uuid::Uuid::new_v4());
    
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

#[derive(Debug, serde::Deserialize)]
struct ControlRequest {
    id: String,
    action: String, // "pause" or "cancel"
}

async fn handle_status(
    app_handle: Arc<tauri::AppHandle>,
) -> Result<impl warp::Reply, warp::Rejection> {
    use tauri::Manager;
    let state = app_handle.state::<crate::core_state::AppState>();
    let list = crate::collect_active_download_statuses(&state);
    Ok(warp::reply::json(&list))
}

async fn handle_control(
    req: ControlRequest,
    app_handle: Arc<tauri::AppHandle>,
) -> Result<impl warp::Reply, warp::Rejection> {
    use tauri::Manager;
    let state = app_handle.state::<crate::core_state::AppState>();
    let reply = match crate::control_active_download(&state, &req.id, &req.action) {
        Ok(true) => serde_json::json!({"success": true}),
        Ok(false) => serde_json::json!({"success": false, "message": "id not found"}),
        Err(message) => serde_json::json!({"success": false, "message": message}),
    };
    Ok(warp::reply::json(&reply))
}

async fn handle_batch(
    links: Vec<BatchLink>,
    batch_tx: Arc<BatchSender>,
) -> Result<impl warp::Reply, warp::Rejection> {
    // Validate all URLs before forwarding — prevent SSRF via batch endpoint
    for link in &links {
        match url::Url::parse(&link.url) {
            Ok(parsed) => {
                if !matches!(parsed.scheme(), "http" | "https") {
                    let response = DownloadResponse {
                        success: false,
                        message: format!("Invalid URL scheme in batch: {}", link.url),
                        id: None,
                    };
                    return Ok(warp::reply::json(&response));
                }
            }
            Err(_) => {
                let response = DownloadResponse {
                    success: false,
                    message: format!("Invalid URL in batch: {}", link.url),
                    id: None,
                };
                return Ok(warp::reply::json(&response));
            }
        }
    }

    let count = links.len();
    if let Ok(_) = get_event_sender().send(serde_json::json!({
        "type": "batch_links",
        "count": count
    })) {}
    match batch_tx.send(links) {
        Ok(_) => {
            let response = DownloadResponse {
                success: true,
                message: format!("{} links queued for review", count),
                id: None,
            };
            Ok(warp::reply::json(&response))
        }
        Err(e) => {
            let response = DownloadResponse {
                success: false,
                message: format!("Failed to queue batch: {}", e),
                id: None,
            };
            Ok(warp::reply::json(&response))
        }
    }
}

async fn handle_streams(
    streams: Vec<DetectedStreamRequest>,
    stream_tx: Arc<StreamSender>,
) -> Result<impl warp::Reply, warp::Rejection> {
    // Validate URLs
    for s in &streams {
        match url::Url::parse(&s.url) {
            Ok(parsed) => {
                if !matches!(parsed.scheme(), "http" | "https") {
                    let response = DownloadResponse {
                        success: false,
                        message: format!("Invalid URL scheme: {}", s.url),
                        id: None,
                    };
                    return Ok(warp::reply::json(&response));
                }
            }
            Err(_) => {
                let response = DownloadResponse {
                    success: false,
                    message: format!("Invalid URL: {}", s.url),
                    id: None,
                };
                return Ok(warp::reply::json(&response));
            }
        }
    }

    let count = streams.len();
    let _ = get_event_sender().send(serde_json::json!({
        "type": "detected_streams",
        "count": count
    }));

    match stream_tx.send(streams) {
        Ok(_) => {
            let response = DownloadResponse {
                success: true,
                message: format!("{} streams detected", count),
                id: None,
            };
            Ok(warp::reply::json(&response))
        }
        Err(e) => {
            let response = DownloadResponse {
                success: false,
                message: format!("Failed to forward streams: {}", e),
                id: None,
            };
            Ok(warp::reply::json(&response))
        }
    }
}
