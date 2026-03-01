use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use warp::Filter;
use tokio::sync::oneshot;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EphemeralShare {
    pub id: String,
    pub file_path: String,
    pub file_name: String,
    pub file_size: u64,
    pub url: String,
    pub port: u16,
    pub token: String,
    pub created_at: u64,
    pub expires_at: u64,
    pub download_count: u64,
}

struct ShareHandle {
    info: EphemeralShare,
    stop_tx: Option<oneshot::Sender<()>>,
}

pub struct EphemeralManager {
    shares: Arc<Mutex<HashMap<String, ShareHandle>>>,
}

impl EphemeralManager {
    pub fn new() -> Self {
        Self {
            shares: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Start sharing a file via temporary HTTP server
    pub async fn start_share(&self, file_path: String, timeout_mins: u64) -> Result<EphemeralShare, String> {
        let path = PathBuf::from(&file_path);
        if !path.exists() {
            return Err("File does not exist".to_string());
        }

        let file_name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();
        
        let file_size = std::fs::metadata(&path)
            .map(|m| m.len())
            .unwrap_or(0);
        
        let id = uuid::Uuid::new_v4().to_string();
        let token = uuid::Uuid::new_v4().to_string().replace("-", "")[..16].to_string();
        
        // Find an available port
        let port = find_available_port().await?;
        
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        let expires_at = now + (timeout_mins * 60);
        
        // Get local IP for URL
        let local_ip = local_ip_address::local_ip()
            .map(|ip| ip.to_string())
            .unwrap_or_else(|_| "127.0.0.1".to_string());
        
        let url = format!("http://{}:{}/{}/{}", local_ip, port, token, file_name);
        
        let share = EphemeralShare {
            id: id.clone(),
            file_path: file_path.clone(),
            file_name: file_name.clone(),
            file_size,
            url: url.clone(),
            port,
            token: token.clone(),
            created_at: now,
            expires_at,
            download_count: 0,
        };
        
        // Create stop channel
        let (stop_tx, stop_rx) = oneshot::channel::<()>();
        
        // Spawn the warp server
        let serve_path = file_path.clone();
        let serve_token = token.clone();
        let serve_filename = file_name.clone();
        let shares_ref = self.shares.clone();
        let share_id = id.clone();
        
        tokio::spawn(async move {
            // Route: GET /<token>/<filename>
            let file_route = warp::path(serve_token.clone())
                .and(warp::path(serve_filename.clone()))
                .and(warp::get())
                .map(move || {
                    let path = PathBuf::from(&serve_path);
                    match std::fs::read(&path) {
                        Ok(data) => {
                            // Guess content type
                            let content_type = guess_content_type(&path);
                            warp::http::Response::builder()
                                .header("Content-Type", content_type)
                                .header("Content-Disposition", format!("attachment; filename=\"{}\"", 
                                    path.file_name().and_then(|n| n.to_str()).unwrap_or("file")))
                                .header("Content-Length", data.len().to_string())
                                .body(data)
                                .unwrap()
                        }
                        Err(_) => {
                            warp::http::Response::builder()
                                .status(404)
                                .body(b"File not found".to_vec())
                                .unwrap()
                        }
                    }
                });
            
            // Landing page at /<token>/
            let landing_token = serve_token.clone();
            let landing_filename = serve_filename.clone();
            let landing_file_size = file_size;
            let landing_route = warp::path(landing_token)
                .and(warp::path::end())
                .and(warp::get())
                .map(move || {
                    let size_str = format_size(landing_file_size);
                    let html = format!(r#"<!DOCTYPE html>
<html><head><title>HyperStream Share</title>
<style>
body {{ font-family: 'Segoe UI', sans-serif; background: #0f0f23; color: #e0e0e0; display: flex; justify-content: center; align-items: center; height: 100vh; margin: 0; }}
.card {{ background: linear-gradient(135deg, #1a1a3e, #2d1b69); border-radius: 16px; padding: 40px; text-align: center; box-shadow: 0 20px 60px rgba(100,50,255,0.2); max-width: 400px; }}
h1 {{ font-size: 24px; margin-bottom: 8px; background: linear-gradient(90deg, #a855f7, #6366f1); -webkit-background-clip: text; -webkit-text-fill-color: transparent; }}
.filename {{ font-size: 18px; color: #c4b5fd; margin: 16px 0; word-break: break-all; }}
.size {{ color: #8b8b9e; margin-bottom: 24px; }}
a.download {{ display: inline-block; padding: 12px 32px; background: linear-gradient(135deg, #7c3aed, #6366f1); color: white; text-decoration: none; border-radius: 8px; font-weight: 600; transition: transform 0.2s; }}
a.download:hover {{ transform: scale(1.05); }}
.footer {{ margin-top: 24px; font-size: 12px; color: #555; }}
</style></head><body>
<div class="card">
<h1>⚡ HyperStream Share</h1>
<div class="filename">📄 {}</div>
<div class="size">{}</div>
<a class="download" href="{}">⬇ Download</a>
<div class="footer">This link will expire automatically.</div>
</div></body></html>"#, landing_filename, size_str, landing_filename);
                    warp::reply::html(html)
                });
                
            let routes = landing_route.or(file_route);
            
            let (_, server) = warp::serve(routes)
                .bind_with_graceful_shutdown(([0, 0, 0, 0], port), async move {
                    let _ = stop_rx.await;
                });
            
            // Run server with timeout
            let timeout_duration = std::time::Duration::from_secs(timeout_mins * 60);
            tokio::select! {
                _ = server => {},
                _ = tokio::time::sleep(timeout_duration) => {
                    println!("[EphemeralServer] Share {} expired after {} minutes", share_id, timeout_mins);
                }
            }
            
            // Cleanup
            if let Ok(mut shares) = shares_ref.lock() {
                shares.remove(&share_id);
            }
        });
        
        // Store handle
        {
            let mut shares = self.shares.lock().unwrap();
            shares.insert(id.clone(), ShareHandle {
                info: share.clone(),
                stop_tx: Some(stop_tx),
            });
        }
        
        println!("[EphemeralServer] Started share: {} -> {}", file_name, url);
        Ok(share)
    }

    /// Stop and remove a share
    pub fn stop_share(&self, id: &str) -> Result<(), String> {
        let mut shares = self.shares.lock().unwrap();
        if let Some(mut handle) = shares.remove(id) {
            if let Some(tx) = handle.stop_tx.take() {
                let _ = tx.send(());
            }
            println!("[EphemeralServer] Stopped share: {}", id);
            Ok(())
        } else {
            Err("Share not found".to_string())
        }
    }

    /// List all active shares
    pub fn list_shares(&self) -> Vec<EphemeralShare> {
        let shares = self.shares.lock().unwrap();
        shares.values().map(|h| h.info.clone()).collect()
    }
}

async fn find_available_port() -> Result<u16, String> {
    // Try ports in range 18000-18100
    for port in 18000..18100 {
        if tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await.is_ok() {
            return Ok(port);
        }
    }
    Err("No available port found for ephemeral server".to_string())
}

fn format_size(bytes: u64) -> String {
    if bytes == 0 { return "0 B".to_string(); }
    let units = ["B", "KB", "MB", "GB", "TB"];
    let i = (bytes as f64).log(1024.0).floor() as usize;
    let i = i.min(units.len() - 1);
    format!("{:.1} {}", bytes as f64 / 1024_f64.powi(i as i32), units[i])
}

fn guess_content_type(path: &PathBuf) -> &'static str {
    let ext = path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    
    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "mp4" => "video/mp4",
        "mp3" => "audio/mpeg",
        "txt" => "text/plain",
        "html" | "htm" => "text/html",
        _ => "application/octet-stream",
    }
}

lazy_static::lazy_static! {
    pub static ref EPHEMERAL_MANAGER: EphemeralManager = EphemeralManager::new();
}
