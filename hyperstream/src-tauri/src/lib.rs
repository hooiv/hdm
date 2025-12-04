use tauri::{Emitter, State};
use futures_util::StreamExt;
use std::sync::{Arc, Mutex};
use crate::downloader::manager::DownloadManager;
use crate::downloader::disk::{DiskWriter, WriteRequest};
use std::sync::mpsc;
use std::thread;
use std::collections::HashMap;
use tokio::sync::broadcast;

mod downloader;
mod persistence;
mod http_server;

use persistence::SavedDownload;

#[derive(Clone, serde::Serialize)]
struct Payload {
    downloaded: u64,
    total: u64,
}

struct DownloadSession {
    #[allow(dead_code)]
    manager: Arc<Mutex<DownloadManager>>,
    stop_tx: broadcast::Sender<()>,
    #[allow(dead_code)]
    url: String,
    #[allow(dead_code)]
    path: String,
    #[allow(dead_code)]
    file_writer: Arc<Mutex<std::fs::File>>,
}

struct AppState {
    downloads: Mutex<HashMap<String, DownloadSession>>,
}

#[tauri::command]
async fn start_download(
    id: String, 
    url: String, 
    path: String, 
    window: tauri::Window, 
    state: State<'_, AppState>
) -> Result<(), String> {
    println!("DEBUG: Starting download ID: {}", id);

    // 1. Check for saved download (Resume logic)
    let saved_downloads = persistence::load_downloads().unwrap_or_default();
    let saved = saved_downloads.iter().find(|d| d.id == id);
    let resume_from: u64 = saved.map(|s| s.downloaded_bytes).unwrap_or(0);
    
    if resume_from > 0 {
        println!("DEBUG: Resuming from byte {}", resume_from);
    }
    
    // 2. Get Content Length
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let head_resp = client.head(&url).send().await.map_err(|e| e.to_string())?;
    let mut total_size = head_resp.content_length().unwrap_or(0);

    // Manual fallback
    if total_size == 0 {
        if let Some(len_header) = head_resp.headers().get("content-length") {
            if let Ok(len_str) = len_header.to_str() {
                if let Ok(len) = len_str.parse::<u64>() {
                    total_size = len;
                }
            }
        }
    }

    if total_size == 0 {
        // Try Range 0-1
        let range_resp = client.get(&url).header("Range", "bytes=0-1").send().await.map_err(|e| e.to_string())?;
        if let Some(content_range) = range_resp.headers().get("content-range") {
            let s = content_range.to_str().unwrap_or("");
            if let Some(slash_pos) = s.find('/') {
                if let Ok(size) = s[slash_pos + 1..].parse::<u64>() {
                    total_size = size;
                }
            }
        }
    }
    
    if total_size == 0 {
        return Err("Could not determine file size".to_string());
    }

    // 3. Initialize File - open for writing, don't truncate if resuming
    let file = if resume_from > 0 {
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .map_err(|e| e.to_string())?
    } else {
        let f = std::fs::File::create(&path).map_err(|e| e.to_string())?;
        f.set_len(total_size).map_err(|e| e.to_string())?;
        f
    };
    let file_mutex = Arc::new(Mutex::new(file));

    // 4. Initialize Manager - for resume, we use a single segment from resume_from to end
    let manager = if resume_from > 0 {
        // Simple resume: single segment from resume_from to end
        let mut mgr = DownloadManager::new(total_size, 1);
        mgr.segments[0].start_byte = resume_from;
        mgr.segments[0].downloaded_cursor = resume_from;
        Arc::new(Mutex::new(mgr))
    } else {
        Arc::new(Mutex::new(DownloadManager::new(total_size, 8)))
    };
    let downloaded_total = Arc::new(Mutex::new(resume_from));

    // 5. Setup Stop Signal
    let (stop_tx, _) = broadcast::channel(1);

    // 6. Store Session
    {
        let mut downloads = state.downloads.lock().unwrap();
        downloads.insert(id.clone(), DownloadSession {
            manager: manager.clone(),
            stop_tx: stop_tx.clone(),
            url: url.clone(),
            path: path.clone(),
            file_writer: file_mutex.clone(),
        });
    }

    // 7. Disk Writer
    let (tx, rx) = mpsc::channel::<WriteRequest>();
    let file_writer_clone = file_mutex.clone();
    thread::spawn(move || {
        let writer = DiskWriter::new(file_writer_clone, rx);
        writer.run();
    });

    // 8. Spawn Threads
    let mut handles = Vec::new();
    
    // We need to clone manager segments to iterate, but we need the Arc for the threads
    let segments_count = {
        let m = manager.lock().unwrap();
        m.segments.len()
    };

    for i in 0..segments_count {
        let manager_clone = manager.clone();
        let url_clone = url.clone();
        let tx_clone = tx.clone();
        let client_clone = client.clone();
        let window_clone = window.clone();
        let downloaded_clone = downloaded_total.clone();
        let mut stop_rx = stop_tx.subscribe();

        let handle = tokio::spawn(async move {
            let (start, end) = {
                let mut m = manager_clone.lock().unwrap();
                let seg = &mut m.segments[i];
                seg.state = crate::downloader::structures::SegmentState::Downloading;
                (seg.start_byte, seg.end_byte)
            };

            if end == 0 || start >= end { return; }

            let mut current_pos = start;
            let mut retry_count = 0;
            const MAX_RETRIES: u32 = 5;

            loop {
                // Check for stop signal
                if stop_rx.try_recv().is_ok() {
                    println!("DEBUG: Thread {} received stop signal", i);
                    // Update state before exit
                    let mut m = manager_clone.lock().unwrap();
                    m.segments[i].downloaded_cursor = current_pos;
                    m.segments[i].state = crate::downloader::structures::SegmentState::Paused;
                    break;
                }

                if current_pos >= end {
                    let mut m = manager_clone.lock().unwrap();
                    m.segments[i].state = crate::downloader::structures::SegmentState::Complete;
                    break;
                }

                let range_header = format!("bytes={}-{}", current_pos, end - 1);
                
                // Use tokio::select to allow cancellation during request
                let res_future = client_clone.get(&url_clone).header("Range", &range_header).send();
                
                let res = tokio::select! {
                    _ = stop_rx.recv() => {
                        println!("DEBUG: Thread {} stopped during request", i);
                        let mut m = manager_clone.lock().unwrap();
                        m.segments[i].downloaded_cursor = current_pos;
                        m.segments[i].state = crate::downloader::structures::SegmentState::Paused;
                        break;
                    }
                    r = res_future => r
                };

                let res = match res {
                    Ok(r) => r,
                    Err(e) => {
                        println!("DEBUG: Thread {} error: {}", i, e);
                        retry_count += 1;
                        if retry_count > MAX_RETRIES { break; }
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        continue;
                    }
                };

                let mut stream = res.bytes_stream();
                
                loop {
                    tokio::select! {
                        _ = stop_rx.recv() => {
                            println!("DEBUG: Thread {} stopped during stream", i);
                            let mut m = manager_clone.lock().unwrap();
                            m.segments[i].downloaded_cursor = current_pos;
                            m.segments[i].state = crate::downloader::structures::SegmentState::Paused;
                            return; // Exit thread
                        }
                        item = stream.next() => {
                            match item {
                                Some(Ok(chunk)) => {
                                    let len = chunk.len() as u64;
                                    tx_clone.send(WriteRequest { offset: current_pos, data: chunk.to_vec() }).unwrap();
                                    current_pos += len;
                                    
                                    // Update global progress
                                    {
                                        let mut d = downloaded_clone.lock().unwrap();
                                        *d += len;
                                        window_clone.emit("download_progress", Payload { downloaded: *d, total: total_size }).unwrap();
                                    }
                                }
                                Some(Err(_)) => {
                                    break; // Stream error, retry loop
                                }
                                None => {
                                    break; // End of stream
                                }
                            }
                        }
                    }
                }
            }
        });
        handles.push(handle);
    }

    // We don't await handles here anymore because we want start_download to return immediately
    // so the UI doesn't freeze. The threads run in background.
    // However, for this simple version, if we return, the command finishes.
    // But the threads are spawned on tokio runtime, so they keep running.
    
    Ok(())
}

#[tauri::command]
fn pause_download(id: String, url: String, path: String, filename: String, downloaded: u64, total: u64, state: State<'_, AppState>) -> Result<(), String> {
    let downloads = state.downloads.lock().unwrap();
    if let Some(session) = downloads.get(&id) {
        let _ = session.stop_tx.send(());
        println!("DEBUG: Pause signal sent to ID: {}", id);
        
        // Save to persistence
        let saved = SavedDownload {
            id: id.clone(),
            url,
            path,
            filename,
            total_size: total,
            downloaded_bytes: downloaded,
            status: "Paused".to_string(),
        };
        persistence::upsert_download(saved)?;
    }
    Ok(())
}

#[tauri::command]
fn get_downloads() -> Result<Vec<SavedDownload>, String> {
    persistence::load_downloads()
}

#[tauri::command]
fn remove_download_entry(id: String) -> Result<(), String> {
    persistence::remove_download(&id)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Create channel for HTTP server to send download requests
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<http_server::DownloadRequest>();
    
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState { downloads: Mutex::new(HashMap::new()) })
        .invoke_handler(tauri::generate_handler![start_download, pause_download, get_downloads, remove_download_entry])
        .setup(move |app| {
            let handle = app.handle().clone();
            
            // Spawn HTTP server
            let tx_clone = tx.clone();
            tauri::async_runtime::spawn(async move {
                http_server::start_server(tx_clone).await;
            });
            
            // Handle download requests from HTTP server
            tauri::async_runtime::spawn(async move {
                while let Some(req) = rx.recv().await {
                    println!("DEBUG: Processing download from extension: {}", req.url);
                    // Emit event to frontend to add download
                    let _ = handle.emit("extension_download", serde_json::json!({
                        "url": req.url,
                        "filename": req.filename
                    }));
                }
            });
            
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
