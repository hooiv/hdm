use tauri::{Emitter, State, Manager};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::menu::{Menu, MenuItem};
use futures_util::StreamExt;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
use crate::downloader::manager::DownloadManager;
use crate::downloader::disk::{DiskWriter, WriteRequest};
use std::sync::mpsc;
use std::thread;
use std::collections::HashMap;
use tokio::sync::broadcast;


mod downloader;
mod persistence;
mod http_server;
use crate::http_server::StreamingSource;
mod settings;
mod speed_limiter;
mod clipboard;
mod network;
mod scheduler;
mod media;
mod plugin_vm;
mod spider;

mod zip_preview;
mod proxy;
mod adaptive_threads;

// mod virus_scanner;
mod import_export;
mod lan_api;
mod system_monitor;
mod feeds;
mod search;

// mod virtual_drive;
mod cloud_bridge;
mod media_processor;
mod ai;
mod audio_events;
mod webhooks;
mod archive_manager;

use persistence::SavedDownload;
use settings::Settings;

// (id, start, end, cursor, state, speed)
type SlimSegment = (u32, u64, u64, u64, u8, u64);

#[derive(Clone, serde::Serialize)]
struct Payload {
    id: String,
    downloaded: u64,
    total: u64,
    segments: Vec<SlimSegment>,
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

pub(crate) struct AppState {
    pub(crate) downloads: Mutex<HashMap<String, DownloadSession>>,
    pub(crate) p2p_node: Arc<network::p2p::P2PNode>,
    pub(crate) p2p_file_map: http_server::FileMap,
    pub(crate) torrent_manager: Arc<network::bittorrent::manager::TorrentManager>,
    pub(crate) connection_manager: network::connection_manager::ConnectionManager,
    pub(crate) chatops_manager: Arc<network::chatops::ChatOpsManager>,
}

#[tauri::command]
async fn add_magnet_link(
    magnet: String,
    state: State<'_, AppState>
) -> Result<usize, String> {
    println!("Adding magnet link: {}", magnet);
    state.torrent_manager.add_magnet(&magnet).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn play_torrent(
    id: usize,
    state: State<'_, AppState>
) -> Result<String, String> {
    // 1. Get file ID (largest file)
    let fid = state.torrent_manager.get_largest_file_id(id)
        .ok_or_else(|| "Could not determine main file ID".to_string())?;
    
    // 2. Register in FileMap (ID -> Torrent Source)
    {
        let mut map = state.p2p_file_map.lock().unwrap();
        map.insert(id.to_string(), StreamingSource::Torrent { torrent_id: id, file_id: fid });
        // NOTE: If we want to support file system fallback (e.g. from get_main_file_path),
        // we could check if file exists on disk.
        // But for "Streaming Logic" task, we prefer the stream.
    }
    
    // 3. Return URL
    Ok(format!("http://localhost:14733/p2p/{}", id))
}

#[tauri::command]
async fn get_torrents(
    state: State<'_, AppState>
) -> Result<Vec<network::bittorrent::manager::TorrentStatus>, String> {
    Ok(state.torrent_manager.get_torrents())
}

#[tauri::command]
async fn export_data(path: String) -> Result<(), String> {
    let settings = settings::load_settings();
    let downloads = persistence::load_downloads().unwrap_or_default();
    
    let data = crate::import_export::ExportData {
        version: "1.0".to_string(),
        timestamp: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
        settings,
        downloads,
    };
    
    crate::import_export::save_export_to_file(&data, &path)
}

#[tauri::command]
async fn import_data(path: String) -> Result<(), String> {
    let data = crate::import_export::load_export_from_file(&path)?;
    
    // 1. Restore Settings
    settings::save_settings(&data.settings)?;
    
    // 2. Restore Downloads (Merge/Append)
    let mut current_downloads = persistence::load_downloads().unwrap_or_default();
    let mut count = 0;
    
    for d in data.downloads {
        if !current_downloads.iter().any(|existing| existing.id == d.id) {
            current_downloads.push(d);
            count += 1;
        }
    }
    
    persistence::save_downloads(&current_downloads).map_err(|e| e.to_string())?;
    
    println!("Imported settings and {} new downloads.", count);
    Ok(())
}


#[tauri::command]
async fn start_download(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
    url: String,
    path: String,
    _force: Option<bool>,
) -> Result<(), String> {
    start_download_impl(&app, &state, id, url, path, None).await
}

pub async fn start_download_impl(
    app: &tauri::AppHandle,
    state: &AppState,
    id: String, 
    url: String, 
    path: String,
    _resume_override: Option<u64>
) -> Result<(), String> {
    println!("DEBUG: Starting download ID: {}", id);
    
    // Play start sound
    crate::media::sounds::play_startup();
    
    // Trigger webhooks for download start
    {
        let settings = settings::load_settings();
        if let Some(webhooks) = settings.webhooks {
            let manager = webhooks::WebhookManager::new();
            manager.load_configs(webhooks).await;
            let payload = webhooks::WebhookPayload {
                event: "DownloadStart".to_string(),
                download_id: id.clone(),
                filename: path.split('\\').last().unwrap_or(&path).to_string(),
                url: url.clone(),
                size: 0,
                speed: 0,
                filepath: Some(path.clone()),
                timestamp: chrono::Utc::now().timestamp(),
            };
            manager.trigger(webhooks::WebhookEvent::DownloadStart, payload).await;
        }
    }

    // 1. Check for saved download (Resume logic)
    let saved_downloads = persistence::load_downloads().unwrap_or_default();
    let saved = saved_downloads.iter().find(|d| d.id == id);
    let resume_from: u64 = saved.map(|s| s.downloaded_bytes).unwrap_or(0);
    
    if resume_from > 0 {
        println!("DEBUG: Resuming from byte {}", resume_from);
    }
    
    let settings = settings::load_settings();

    // AUTO-SORT / CATEGORY LOGIC
    // We only change path if it's a new download (not resuming) OR if we force checks (safer to only do new)
    // But `resume_from > 0` implies file exists at `path`. If we change `path` on resume, it breaks.
    // So ONLY apply category rules if strict resume_from == 0 OR we check if file exists at old path.
    // Simplest: only apply on start (resume_from == 0).
    
    let final_path = if resume_from == 0 && settings.use_category_folders {
        // Parse filename
        let path_obj = std::path::Path::new(&path);
        let filename = path_obj.file_name().unwrap_or_default().to_string_lossy().to_string();
        
        // Find matching rule
        let mut new_path_buf = path_obj.to_path_buf();
        
        for rule in &settings.category_rules {
            if let Ok(re) = regex::Regex::new(&rule.pattern) {
                if re.is_match(&filename) {
                    println!("DEBUG: Matched Category Rule '{}' for '{}'", rule.name, filename);
                    
                    // Determine parent (Category Folder)
                    // If rule.path is absolute, use it. If relative, join with settings.download_dir
                    let category_path = if std::path::Path::new(&rule.path).is_absolute() {
                        std::path::PathBuf::from(&rule.path)
                    } else {
                        std::path::PathBuf::from(&settings.download_dir).join(&rule.path)
                    };
                    
                    // Create dir if needed
                    std::fs::create_dir_all(&category_path).ok();
                    
                    new_path_buf = category_path.join(&filename);
                    break; 
                }
            }
        }
        new_path_buf.to_string_lossy().to_string()
    } else {
        path.clone()
    };
    
    // Use final_path for the rest
    let path = final_path;

    // 2. Get Content Length
    let settings = settings::load_settings(); // reload settings fresh
    
    // Ensure Tor is ready if enabled (Idempotent call)
    if settings.use_tor {
        if crate::network::tor::get_socks_port().is_none() {
             // Try to init on demand
             let _ = crate::network::tor::init_tor().await.map_err(|e| format!("Tor Init Failed: {}", e))?;
        }
    }

    let proxy_config = crate::proxy::ProxyConfig::from_settings(&settings);

    // Use masquerading to evade anti-bot blocking
    let client = if settings.dpi_evasion {
        network::masq::build_impersonator_client(network::masq::BrowserProfile::Chrome, Some(&proxy_config))
    } else {
        network::masq::build_client(Some(&proxy_config))
    }.map_err(|e| e.to_string())?;

    let total_size = downloader::initialization::determine_total_size(&client, &url).await?;

    // 3. Initialize File
    let file = downloader::initialization::setup_file(&path, resume_from, total_size)?;
    let file_mutex = file;

    // Register P2P
    {
        let mut map = state.p2p_file_map.lock().unwrap();
        map.insert(id.clone(), StreamingSource::FileSystem(std::path::PathBuf::from(&path)));
    }
    let p2p_node = state.p2p_node.clone();
    let id_clone = id.clone();
    // P2P file advertising removed (not needed in simplified P2P)

    // 4. Initialize Manager
    let manager = downloader::initialization::setup_manager(total_size, saved, resume_from);
    let downloaded_total = Arc::new(AtomicU64::new(resume_from));
    // let last_progress_emit = Arc::new(Mutex::new(std::time::Instant::now())); // Removed

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
        let mut writer = DiskWriter::new(file_writer_clone, rx);
        writer.run();
    });

    // 8. Spawn Monitor Task (Decoupled Emission)
    let manager_monitor = manager.clone();
    let downloaded_monitor = downloaded_total.clone();
    let window_monitor = app.clone();
    let id_monitor = id.clone();
    let url_monitor = url.clone();
    let path_monitor = path.clone();
    let mut stop_rx_monitor = stop_tx.subscribe();
    
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(33)); // ~30fps
        loop {
            tokio::select! {
                _ = stop_rx_monitor.recv() => break,
                _ = interval.tick() => {
                    let d = downloaded_monitor.load(Ordering::Relaxed);
                    
                    // Get segment snapshot for visualization
                    // We only lock here, once per 33ms, instead of per-chunk
                    // Note: get_segments_snapshot internally locks.
                    let segments = manager_monitor.lock().unwrap().get_segments_snapshot();
                    
                    // Compress to tuple format
                    let slim_segments: Vec<SlimSegment> = segments.iter().map(|s| (
                        s.id,
                        s.start_byte,
                        s.end_byte,
                        s.downloaded_cursor,
                        s.state as u8,
                        s.speed_bps
                    )).collect();

                    let _ = window_monitor.emit("download_progress", Payload { 
                        id: id_monitor.clone(), 
                        downloaded: d, 
                        total: total_size,
                        segments: slim_segments
                    });

                    if d >= total_size {
                        crate::media::sounds::play_complete();
                        
                        // Trigger webhooks for download complete
                        let id_webhook = id_monitor.clone();
                        let url_webhook = url_monitor.clone();
                        let path_webhook = path_monitor.clone();
                        let size_webhook = total_size;
                        tokio::spawn(async move {
                            let settings = settings::load_settings();
                            if let Some(webhooks) = settings.webhooks {
                                let manager = webhooks::WebhookManager::new();
                                manager.load_configs(webhooks).await;
                                let payload = webhooks::WebhookPayload {
                                    event: "DownloadComplete".to_string(),
                                    download_id: id_webhook.clone(),
                                    filename: path_webhook.split('\\').last().unwrap_or(&path_webhook).to_string(),
                                    url: url_webhook.clone(),
                                    size: size_webhook,
                                    speed: 0,
                                    filepath: Some(path_webhook.clone()),
                                    timestamp: chrono::Utc::now().timestamp(),
                                };
                                manager.trigger(webhooks::WebhookEvent::DownloadComplete, payload).await;
                            }
                        });
                        
                        // Auto-extract archives if enabled
                        let path_archive = path_monitor.clone();
                        tokio::spawn(async move {
                            let settings = settings::load_settings();
                            if settings.auto_extract_archives {
                                if let Some(archive_info) = archive_manager::ArchiveManager::detect_archive(&path_archive) {
                                    println!("📦 Detected archive: {:?}", archive_info.archive_type);
                                    
                                    // Extract to same directory as archive
                                    let dest = std::path::Path::new(&path_archive)
                                        .parent()
                                        .and_then(|p| p.to_str())
                                        .unwrap_or(".")
                                        .to_string();
                                    
                                    match archive_manager::ArchiveManager::extract_archive(&path_archive, &dest) {
                                        Ok(msg) => {
                                            println!("✅ {}", msg);
                                            
                                            // Cleanup archives if enabled
                                            if settings.cleanup_archives_after_extract {
                                                if let Err(e) = archive_manager::ArchiveManager::cleanup_archive(&path_archive) {
                                                    eprintln!("⚠️  Cleanup failed: {}", e);
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            eprintln!("❌ Extraction failed: {}", e);
                                        }
                                    }
                                }
                            }
                        });
                        
                        break;
                    }
                }
            }
        }
    });

    // 9. Spawn Worker Threads
    let mut handles = Vec::new();
    
    // We need to clone manager segments to iterate
    let segments_count = manager.lock().unwrap().segments.read().unwrap().len();

    for i in 0..segments_count {
        let manager_clone = manager.clone();
        let url_clone = url.clone();
        let tx_clone = tx.clone();
        let client_clone = client.clone();
        // let window_clone = window.clone(); // Not needed in worker
        let downloaded_clone = downloaded_total.clone();
        // let last_emit_clone = last_progress_emit.clone(); // Not needed
        let cm_clone = state.connection_manager.clone();
        let mut stop_rx = stop_tx.subscribe();
        let stop_tx_clone = stop_tx.clone();
        let id_worker = id.clone();
        let path_worker = path.clone();
        let url_worker = url.clone(); // Alias for error handler
        let app_handle_clone = app.clone(); // Capture app handle for emitting events

        let handle = tokio::spawn(async move {
            let (start, end) = {
                let m = manager_clone.lock().unwrap();
                let mut segs = m.segments.write().unwrap();
                let seg = &mut segs[i];
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
                    let m = manager_clone.lock().unwrap();
                    let mut segs = m.segments.write().unwrap();
                    segs[i].downloaded_cursor = current_pos;
                    segs[i].state = crate::downloader::structures::SegmentState::Paused;
                    break;
                }

                if current_pos >= end {
                    let m = manager_clone.lock().unwrap();
                    let mut segs = m.segments.write().unwrap();
                    segs[i].state = crate::downloader::structures::SegmentState::Complete;
                    break;
                }

                let range_header = format!("bytes={}-{}", current_pos, end - 1);
                
                // Acquire permit via ConnectionManager
                let _permit = cm_clone.acquire(&url_clone).await;

                // Chaos Mode Check: Inject latency or failure here
                if let Err(_e) = crate::network::chaos::check_chaos().await {
                     retry_count += 1;
                     if retry_count <= MAX_RETRIES {
                         tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                         continue;
                     }
                }

                // Use tokio::select to allow cancellation during request
                let res_future = client_clone.get(&url_clone).header("Range", &range_header).send();
                
                let res = tokio::select! {
                    _ = stop_rx.recv() => {
                        let m = manager_clone.lock().unwrap();
                        let mut segs = m.segments.write().unwrap();
                        segs[i].downloaded_cursor = current_pos;
                        segs[i].state = crate::downloader::structures::SegmentState::Paused;
                        break;
                    }
                    r = res_future => r
                };

                let response = match res {
                    Ok(r) => r,
                    Err(e) => {
                        println!("DEBUG: Thread {} error: {}", i, e);
                        retry_count += 1;
                        if retry_count > MAX_RETRIES { 
                            crate::media::sounds::play_error();
                            break; 
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        continue;
                    }
                };

                // Check for 403 Forbidden / 410 Gone (Link Expired)
                if response.status() == rquest::StatusCode::FORBIDDEN || response.status() == rquest::StatusCode::GONE {
                     println!("Thread {} error: Link Expired (403/410). Requesting Hot-Swap.", i);
                     
                     // 1. Stop all threads
                     let _ = stop_tx_clone.send(());

                     // 2. Persist status as "WaitingForRefresh"
                     let segments = manager_clone.lock().unwrap().get_segments_snapshot();
                     let total_downloaded = segments.iter().map(|s| s.downloaded_cursor - s.start_byte).sum();
                     
                     let filename_s = std::path::Path::new(&path_worker).file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_else(|| "download".to_string());
                        
                     let saved = persistence::SavedDownload {
                         id: id_worker.clone(),
                         url: url_worker.clone(),
                         path: path_worker.clone(),
                         filename: filename_s,
                         total_size: 0, // Should be passed but not captured? total_size is u64 Copy.
                         downloaded_bytes: total_downloaded,
                         status: "WaitingForRefresh".to_string(),
                         segments: Some(segments),
                     };
                     // We need total_size in worker? It's captured if Copy.
                     // But we need to make sure `saved.total_size` is correct.
                     // `total_size` is available in `start_download` scope.
                     
                     let _ = persistence::upsert_download(saved);
                     
                     // 3. Notify UI
                     let _ = app_handle_clone.emit("download_progress", Payload {
                         id: id_worker.clone(),
                         downloaded: total_downloaded,
                         total: 0, 
                         segments: vec![],
                     });
                     
                     crate::media::sounds::play_error();
                     
                     // Trigger webhooks for download error
                     let id_error = id_worker.clone();
                     let url_error = url_worker.clone();
                     let path_error = path_worker.clone();
                     tokio::spawn(async move {
                         let settings = settings::load_settings();
                         if let Some(webhooks) = settings.webhooks {
                             let manager = webhooks::WebhookManager::new();
                             manager.load_configs(webhooks).await;
                             let payload = webhooks::WebhookPayload {
                                 event: "DownloadError".to_string(),
                                 download_id: id_error.clone(),
                                 filename: path_error.split('\\').last().unwrap_or(&path_error).to_string(),
                                 url: url_error.clone(),
                                 size: 0,
                                 speed: 0,
                                 filepath: Some(path_error.clone()),
                                 timestamp: chrono::Utc::now().timestamp(),
                             };
                             manager.trigger(webhooks::WebhookEvent::DownloadError, payload).await;
                         }
                     });
                     
                     return;
                }

                // Check for Rate Limiting (429/503)
                if response.status() == rquest::StatusCode::TOO_MANY_REQUESTS || response.status() == rquest::StatusCode::SERVICE_UNAVAILABLE {
                     let wait_time = if let Some(h) = response.headers().get("Retry-After") {
                         if let Ok(s) = h.to_str() {
                             crate::downloader::network::parse_retry_after(s).unwrap_or(std::time::Duration::from_secs(30))
                         } else {
                             std::time::Duration::from_secs(30)
                         }
                     } else {
                         std::time::Duration::from_secs(30)
                     };

                     tokio::time::sleep(wait_time).await;
                     continue;
                }

                let mut stream = response.bytes_stream();
                
                loop {
                    tokio::select! {
                        _ = stop_rx.recv() => {
                            let m = manager_clone.lock().unwrap();
                            let mut segs = m.segments.write().unwrap();
                            segs[i].downloaded_cursor = current_pos;
                            segs[i].state = crate::downloader::structures::SegmentState::Paused;
                            return; // Exit thread
                        }
                        item = stream.next() => {
                            match item {
                                Some(Ok(chunk)) => {
                                    let len = chunk.len() as u64;
                                    tx_clone.send(WriteRequest { offset: current_pos, data: chunk.to_vec(), segment_id: i as u32 }).unwrap();
                                    current_pos += len;
                                    
                                    // Update global progress ATOMICALLY (Lock-Free)
                                    downloaded_clone.fetch_add(len, Ordering::Relaxed);
                                    
                                    // NO EMISSION HERE!
                                    // Emission is handled by monitor_task
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

    // 9. Periodic Save Loop (Crash Recovery)
    let manager_save = manager.clone();
    let id_save = id.clone();
    let url_save = url.clone();
    let path_save = path.clone();
    // derived filename
    let filename_save = std::path::Path::new(&path).file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "download".to_string());
    let mut stop_rx_save = stop_tx.subscribe();
    
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = stop_rx_save.recv() => break,
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(5)) => {
                    let segments = manager_save.lock().unwrap().get_segments_snapshot();
                    let total_downloaded = segments.iter().map(|s| s.downloaded_cursor - s.start_byte).sum();
                    
                    let saved = persistence::SavedDownload {
                        id: id_save.clone(),
                        url: url_save.clone(),
                        path: path_save.clone(),
                        filename: filename_save.clone(),
                        total_size,
                        downloaded_bytes: total_downloaded,
                        status: "Downloading".to_string(),
                        segments: Some(segments),
                    };
                    // Silent save, ignore errors
                    let _ = persistence::upsert_download(saved);
                }
            }
        }
    });
    
    Ok(())
}



#[tauri::command]
async fn pause_download(
    id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut downloads = state.downloads.lock().unwrap();
    if let Some(session) = downloads.remove(&id) {
        let _ = session.stop_tx.send(());
    }
    
    // Update persistence
    let mut saved_downloads = persistence::load_downloads().unwrap_or_default();
    if let Some(d) = saved_downloads.iter_mut().find(|d| d.id == id) {
        d.status = "Paused".to_string();
        persistence::save_downloads(&saved_downloads).map_err(|e| e.to_string())?;
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

#[tauri::command]
fn get_settings() -> serde_json::Value {
    let s = settings::load_settings();
    serde_json::to_value(s).unwrap_or(serde_json::json!({}))
}

#[tauri::command]
fn save_settings(json: serde_json::Value) -> Result<(), String> {
    let new_settings: settings::Settings = serde_json::from_value(json).map_err(|e| e.to_string())?;
    // Update speed limiter when settings change
    speed_limiter::GLOBAL_LIMITER.set_limit(new_settings.speed_limit_kbps * 1024);
    // Update clipboard monitor
    clipboard::CLIPBOARD_MONITOR.set_enabled(new_settings.clipboard_monitor);
    settings::save_settings(&new_settings)
}

#[tauri::command]
fn open_file(path: String) -> Result<(), String> {
    std::process::Command::new("cmd")
        .args(["/c", "start", "", &path])
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn open_folder(path: String) -> Result<(), String> {
    let folder = std::path::Path::new(&path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| path.clone());
    
    std::process::Command::new("explorer")
        .arg(&folder)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn schedule_download(id: String, url: String, filename: String, scheduled_time: String) -> Result<(), String> {
    scheduler::add_scheduled_download(scheduler::ScheduledDownload {
        id,
        url,
        filename,
        scheduled_time,
        status: "pending".to_string(),
    });
    Ok(())
}



#[tauri::command]
fn cancel_scheduled_download(id: String) -> Result<(), String> {
    scheduler::remove_scheduled_download(&id);
    Ok(())
}

// ============ Spider / Site Grabber Commands ============

#[tauri::command]
async fn crawl_website(
    url: String, 
    max_depth: u32, 
    extensions: Vec<String>
) -> Result<Vec<spider::GrabbedFile>, String> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120.0.0.0")
        .build()
        .map_err(|e| e.to_string())?;
    
    let spider = spider::Spider::new(client);
    spider.crawl(spider::SpiderOptions {
        url,
        max_depth,
        same_domain: true,
        extensions,
    }).await
}


// ============ ZIP Preview Commands ============

#[tauri::command]
fn preview_zip_partial(data: Vec<u8>) -> Result<zip_preview::ZipPreview, String> {
    zip_preview::preview_zip_partial(&data)
}

#[tauri::command]
fn preview_zip_file(path: String) -> Result<zip_preview::ZipPreview, String> {
    zip_preview::preview_zip(std::path::Path::new(&path))
}

#[tauri::command]
fn extract_single_file(zip_path: String, entry_name: String, dest_path: String) -> Result<(), String> {
    zip_preview::extract_file(
        std::path::Path::new(&zip_path),
        &entry_name,
        std::path::Path::new(&dest_path)
    )
}

#[tauri::command]
async fn preview_zip_remote(url: String) -> Result<zip_preview::ZipPreview, String> {
    let client = rquest::Client::builder()
        .build()
        .map_err(|e| e.to_string())?;
    zip_preview::preview_zip_remote(url, client).await
}

#[tauri::command]
async fn download_zip_entry(url: String, entry_name: String, dest_path: String) -> Result<(), String> {
    let client = rquest::Client::builder()
        .build()
        .map_err(|e| e.to_string())?;
    let bytes = zip_preview::download_entry_remote(url, entry_name, client).await?;
    std::fs::write(dest_path, bytes).map_err(|e| e.to_string())
}

#[tauri::command]
fn read_zip_last_bytes(path: String, length: usize) -> Result<Vec<u8>, String> {
    zip_preview::read_last_bytes(std::path::Path::new(&path), length)
}


// ============ HLS/DASH Stream Parser Commands ============

#[tauri::command]
async fn init_tor_network() -> Result<u16, String> {
    network::tor::init_tor().await
}

#[tauri::command]
fn get_tor_status() -> Option<u16> {
    network::tor::get_socks_port()
}

#[tauri::command]
async fn perform_semantic_search(query: String) -> Result<Vec<ai::SearchResult>, String> {
    ai::semantic_search(&query)
}

#[tauri::command]
async fn index_all_downloads() -> Result<usize, String> {
    let downloads = persistence::load_downloads().unwrap_or_default();
    let mut count = 0;
    
    // Spawn task to avoid blocking main thread too long, though we await it here for result
    // Ideally should be background.
    for d in downloads {
        if d.status == "Complete" {
             if let Ok(_) = ai::index_file(&d.path) {
                 count += 1;
             }
        }
    }
    Ok(count)
}

#[tauri::command]
async fn join_workspace(host_ip: String) -> Result<(), String> {
    network::sync_client::connect_to_workspace(host_ip).await
}


#[tauri::command]
async fn parse_hls_stream(url: String) -> Result<media::HlsStream, String> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| e.to_string())?;
    
    let parser = media::HlsParser::new(client);
    parser.parse(&url).await
}

#[tauri::command]
fn parse_dash_manifest(content: String, base_url: String) -> Result<media::dash_parser::DashManifest, String> {
    media::dash_parser::parse_mpd(&content, &base_url)
}

// ============ Muxer Commands ============

#[tauri::command]
async fn mux_video_audio(video_path: String, audio_path: String, output_path: String) -> Result<(), String> {
    media::muxer::merge_streams(
        std::path::Path::new(&video_path),
        std::path::Path::new(&audio_path),
        std::path::Path::new(&output_path)
    )
}

#[tauri::command]
fn check_ffmpeg_installed() -> bool {
    media::muxer::is_ffmpeg_available()
}

#[tauri::command]
fn decrypt_aes_128(input_path: String, output_path: String, key_hex: String, iv_hex: String) -> Result<(), String> {
    let key = media::decrypt::decode_hex(&key_hex)?;
    let iv = media::decrypt::decode_hex(&iv_hex)?;
    
    let encrypted_data = std::fs::read(&input_path).map_err(|e| e.to_string())?;
    let decrypted = media::decrypt::decrypt_aes128(&encrypted_data, &key, &iv)?;
    
    std::fs::write(&output_path, decrypted).map_err(|e| e.to_string())?;
    Ok(())
}


#[tauri::command]
async fn test_browser_fingerprint() -> Result<String, String> {
    let settings = settings::load_settings();
    let proxy_config = crate::proxy::ProxyConfig::from_settings(&settings);

    // Enable DPI evasion for the test to verify headers
    let client = network::masq::build_impersonator_client(network::masq::BrowserProfile::Chrome, Some(&proxy_config))
        .map_err(|e| e.to_string())?;
    
    // Hit a trace URL (using httpbin for now to show headers)
    let resp = client.get("https://httpbin.org/headers")
        .send()
        .await
        .map_err(|e| e.to_string())?;
        
    let text = resp.text().await.map_err(|e| e.to_string())?;
    Ok(text)
}

// ============ Proxy Configuration Commands ============

#[tauri::command]
fn get_proxy_config() -> serde_json::Value {
    let settings = settings::load_settings();
    let config = proxy::ProxyConfig::from_settings(&settings);
    serde_json::to_value(config).unwrap_or(serde_json::json!({}))
}

#[tauri::command]
async fn test_proxy(config: proxy::ProxyConfig) -> Result<bool, String> {
    // Use rquest because it's our main driver
    let client = rquest::Client::builder()
        .proxy(config.to_rquest_proxy().ok_or("Invalid Proxy Config")?)
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    // Verify connectivity
    let _ = client.head("https://www.google.com")
        .send()
        .await
        .map_err(|e| format!("Connection failed: {}", e))?;
        
    Ok(true)
}

// ============ Virtual Drive Commands ============
// #[tauri::command]
// async fn mount_drive(id: String, path: String) -> Result<u16, String> {
//    virtual_drive::DRIVE_MANAGER.mount(id, path).await
// }

// #[tauri::command]
// async fn unmount_drive(id: String) -> Result<(), String> {
//    virtual_drive::DRIVE_MANAGER.unmount(id)
// }

// ============ Cloud Commands ============
#[tauri::command]
async fn upload_to_cloud(app_handle: tauri::AppHandle, path: String, target_name: Option<String>) -> Result<String, String> {
    let settings_state = app_handle.state::<std::sync::Arc<tokio::sync::Mutex<Settings>>>();
    let settings = settings_state.lock().await;

    let filename = std::path::Path::new(&path)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("Invalid path")?;
        
    let key = target_name.unwrap_or(filename.to_string());
    
    // Construct full path if simple filename given?
    // Usually 'path' from frontend 'task.filename' might be just name.
    // Need to resolve.
    let full_path = if std::path::Path::new(&path).is_absolute() {
        std::path::PathBuf::from(&path)
    } else {
         let download_dir = &settings.download_dir;
         // Resolve relative to download dir.
         // Need to handle user directory expansion if needed, but assuming absolute or simple join.
         std::path::PathBuf::from(download_dir).join(&path)
    };
    
    // Check if file exists there, if not check Desktop (legacy default)
    let final_path = if full_path.exists() {
        full_path
    } else {
         // Fallback to Desktop construction as seen in other parts
         // This is hacky but matches 'open_folder' logic seen previously
         let mut p = dirs::desktop_dir().ok_or("No desktop")?;
         p.push(&path);
         p
    };

    cloud_bridge::CloudBridge::upload_file(&settings, final_path.to_str().unwrap(), &key).await
}

// ============ Media Commands ============
#[tauri::command]
async fn process_media(app_handle: tauri::AppHandle, path: String, action: String) -> Result<String, String> {
    // action: "check", "preview", "audio"
    if action == "check" {
        return if media_processor::MediaProcessor::check_ffmpeg() {
            Ok("Available".to_string())
        } else {
            Err("FFmpeg not found".to_string())
        };
    }

    let settings_state = app_handle.state::<std::sync::Arc<tokio::sync::Mutex<Settings>>>();
    let settings = settings_state.lock().await;

    // Resolve path (reusing logic from upload because it's robust-ish)
    // TODO: move path resolution to helper
    let full_path = if std::path::Path::new(&path).is_absolute() {
        std::path::PathBuf::from(&path)
    } else {
         std::path::PathBuf::from(&settings.download_dir).join(&path)
    };
    
    let final_path = if full_path.exists() {
        full_path
    } else {
         let mut p = dirs::desktop_dir().ok_or("No desktop")?;
         p.push(&path);
         p
    };
    
    let input_str = final_path.to_str().unwrap();

    match action.as_str() {
        "preview" => {
            let output_path = final_path.with_extension("webp");
            media_processor::MediaProcessor::generate_preview(input_str, output_path.to_str().unwrap())
        },
        "audio" => {
            let output_path = final_path.with_extension("mp3");
            media_processor::MediaProcessor::extract_audio(input_str, output_path.to_str().unwrap())
        },
        _ => Err("Unknown action".to_string())
    }
}


// ============ Import/Export Commands (Disabled) ============
/*
#[tauri::command]
fn export_downloads(path: String) -> Result<(), String> {
    let downloads = persistence::load_downloads().unwrap_or_default();
    let mut export = import_export::HyperStreamExport::new();
    
    for d in downloads {
        export.downloads.push(import_export::ExportedDownload {
            url: d.url,
            filename: d.filename,
            save_path: d.path,
            category: None,
            total_size: d.total_size,
            downloaded_bytes: d.downloaded_bytes,
            status: d.status,
            added_at: String::new(), // Not stored in SavedDownload
        });
    }
    
    export.to_json_file(std::path::Path::new(&path))
}

#[tauri::command]
fn import_downloads(path: String) -> Result<Vec<import_export::ExportedDownload>, String> {
    let export = import_export::HyperStreamExport::from_json_file(std::path::Path::new(&path))?;
    Ok(export.downloads)
}

#[tauri::command]
fn import_from_idm_file(path: String) -> Result<Vec<import_export::ExportedDownload>, String> {
    import_export::import_from_idm(std::path::Path::new(&path))
}

#[tauri::command]
fn export_downloads_csv(path: String) -> Result<(), String> {
    let downloads = persistence::load_downloads().unwrap_or_default();
    let mut export = import_export::HyperStreamExport::new();
    
    for d in downloads {
        export.downloads.push(import_export::ExportedDownload {
            url: d.url,
            filename: d.filename,
            save_path: d.path,
            category: None,
            total_size: d.total_size,
            downloaded_bytes: d.downloaded_bytes,
            status: d.status,
            added_at: String::new(),
        });
    }
    
    export.to_csv_file(std::path::Path::new(&path))
}
*/


// ============ Virus Scanning Commands (Disabled) ============
/*
#[tauri::command]
async fn scan_file_for_virus(path: String) -> Result<String, String> {
    let scanner = virus_scanner::VirusScanner::new();
    if !scanner.is_available() {
        return Ok("Scanner not available".to_string());
    }
    
    let result = scanner.scan_file(std::path::Path::new(&path)).await;
    match result {
        virus_scanner::ScanResult::Clean => Ok("Clean".to_string()),
        virus_scanner::ScanResult::Infected { threat_name } => Ok(format!("Infected: {}", threat_name)),
        virus_scanner::ScanResult::Error { message } => Err(message),
        virus_scanner::ScanResult::NotScanned => Ok("Not scanned".to_string()),
    }
}

#[tauri::command]
fn is_antivirus_available() -> bool {
    virus_scanner::VirusScanner::new().is_available()
}
*/

// ============ Speed Limiter Commands ============

#[tauri::command]
async fn acquire_bandwidth(amount: u32) -> Result<(), String> {
    speed_limiter::GLOBAL_LIMITER.acquire(amount as u64).await;
    Ok(())
}

#[tauri::command]
fn set_speed_limit(limit_kbps: u64) {
    speed_limiter::GLOBAL_LIMITER.set_limit(limit_kbps * 1024);
}

#[tauri::command]
fn get_speed_limit() -> u64 {
    speed_limiter::GLOBAL_LIMITER.get_limit() / 1024
}

// ============ LAN API Commands ============

#[tauri::command]
fn generate_lan_pairing_code() -> String {
    lan_api::LanApiServer::generate_pairing_code()
}


#[tauri::command]
fn get_lan_pairing_qr_data(port: u16, code: String) -> String {
    let server = lan_api::LanApiServer::new(port);
    server.get_pairing_qr_data(&code)
}

#[tauri::command]
fn get_local_ip() -> String {
    local_ip_address::local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string())
}

// ============ Scheduler Helper Commands ============

#[tauri::command]
fn is_quiet_hours() -> bool {
    scheduler::is_quiet_hours()
}

#[tauri::command]
fn get_time_info() -> scheduler::TimeInfo {
    scheduler::get_current_time_info()
}

#[tauri::command]
fn get_scheduled_downloads() -> Vec<scheduler::ScheduledDownload> {
    scheduler::get_scheduled_downloads()
}

#[tauri::command]
fn remove_scheduled_download(id: String) {
    scheduler::remove_scheduled_download(&id);
}


#[tauri::command]
fn force_start_scheduled_download<R: tauri::Runtime>(app_handle: tauri::AppHandle<R>, id: String) -> Result<(), String> {
    if let Some(download) = scheduler::force_start_download(&id) {
        // Emit start event immediately
        app_handle.emit("scheduled_download_start", serde_json::json!({
            "id": download.id,
            "url": download.url,
            "filename": download.filename
        })).map_err(|e| e.to_string())?;
        Ok(())
    } else {
        Err("Download not found or already started".to_string())
    }
}



// ============ Plugin System Commands ============

#[tauri::command]
async fn get_plugin_metadata(app_handle: tauri::AppHandle, script: String) -> Result<Option<plugin_vm::lua_host::PluginMetadata>, String> {
    let client = rquest::Client::builder()
        // .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| e.to_string())?;
    
    let host = plugin_vm::lua_host::LuaPluginHost::new(client, app_handle);
    host.init().await.map_err(|e| e.to_string())?;
    host.load_script(&script).await.map_err(|e| e.to_string())?;
    host.get_plugin_metadata().await.map_err(|e| e.to_string())
}

// ============ Network Validation Commands ============

#[tauri::command]
fn analyze_http_status(status_code: u16) -> String {
    use rquest::StatusCode;
    let status = StatusCode::from_u16(status_code).unwrap_or(StatusCode::OK);
    let strategy = downloader::network::analyze_status(status);
    format!("{:?}", strategy)
}

#[tauri::command]
fn check_captive_portal(first_bytes: Vec<u8>) -> bool {
    downloader::network::is_captive_portal(&first_bytes)
}

// ============ Disk Operation Commands ============

#[tauri::command]
fn preallocate_download_file(path: String, size: u64) -> Result<(), String> {
    downloader::disk::preallocate_file(std::path::Path::new(&path), size)
        .map(|_| ()) // Discard the file handle
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn check_file_exists(path: String) -> bool {
    std::path::Path::new(&path).exists()
}

#[tauri::command]
fn get_file_size(path: String) -> Result<u64, String> {
    std::fs::metadata(&path)
        .map(|m| m.len())
        .map_err(|e| e.to_string())
}

// ============ Adaptive Thread Commands ============

lazy_static::lazy_static! {
    static ref THREAD_CONTROLLER: adaptive_threads::AdaptiveThreadController = 
        adaptive_threads::AdaptiveThreadController::new(2, 16);
    static ref BANDWIDTH_MONITOR: adaptive_threads::BandwidthMonitor = 
        adaptive_threads::BandwidthMonitor::new(5);
}

#[tauri::command]
fn get_adaptive_thread_count() -> u32 {
    THREAD_CONTROLLER.get_threads()
}

#[tauri::command]
fn update_thread_count(current_speed: u64, max_speed: u64) -> u32 {
    THREAD_CONTROLLER.update(current_speed, max_speed)
}

#[tauri::command]
fn add_bandwidth_sample(bytes: u64) {
    BANDWIDTH_MONITOR.add_sample(bytes);
}

#[tauri::command]
fn get_average_bandwidth() -> u64 {
    BANDWIDTH_MONITOR.get_average_speed()
}

// ============ More Scheduler Commands ============

#[tauri::command]
fn get_next_download_time() -> String {
    scheduler::get_next_download_time().to_rfc3339()
}

// ============ FDM Import Command (Disabled) ============
/*
#[tauri::command]
fn import_from_fdm_file(path: String) -> Result<Vec<import_export::ExportedDownload>, String> {
    import_export::import_from_fdm(std::path::Path::new(&path))
}
*/

// ============ Feeds Commands ============
#[tauri::command]
async fn fetch_feed(url: String) -> Result<Vec<feeds::FeedItem>, String> {
    feeds::fetch_feed(&url).await
}

#[tauri::command]
async fn perform_search(query: String) -> Result<Vec<search::SearchResult>, String> {
    let engine = search::SEARCH_ENGINE.lock().await;
    engine.search(query)
}

#[tauri::command]
fn get_feeds() -> Vec<feeds::FeedConfig> {
    feeds::FEED_MANAGER.get_feeds()
}

#[tauri::command]
fn add_feed(config: feeds::FeedConfig) {
    feeds::FEED_MANAGER.add_feed(config);
}

#[tauri::command]
fn remove_feed(id: String) {
    feeds::FEED_MANAGER.remove_feed(&id);
}

// ============ Tray & Setup ============



// ============ ZIP Extraction Commands ============

#[tauri::command]
fn read_file_bytes_at_offset(path: String, offset: u64, length: usize) -> Result<Vec<u8>, String> {
    zip_preview::read_bytes_at_offset(std::path::Path::new(&path), offset, length)
}

// ============ HTTP Client Commands ============

#[tauri::command]
fn validate_http_response(
    status_code: u16,
    content_length: Option<u64>,
    content_type: Option<String>,
    accept_ranges: Option<String>
) -> Result<(), String> {
    use rquest::StatusCode;
    let validator = downloader::network::ResponseValidator::new();
    let status = StatusCode::from_u16(status_code).map_err(|e| e.to_string())?;
    validator.validate(
        status,
        content_length,
        content_type.as_deref(),
        accept_ranges.as_deref()
    )
}

#[tauri::command]
fn parse_retry_after_header(value: String) -> Option<u64> {
    downloader::network::parse_retry_after(&value)
        .map(|d| d.as_secs())
}

#[tauri::command]
fn check_error_content_type(content_type: Option<String>, expected_type: Option<String>) -> bool {
    downloader::network::is_error_content_type(
        content_type.as_deref(),
        expected_type.as_deref()
    )
}

// ============ Stealth HTTP Client Commands ============

#[tauri::command]
fn get_chrome_user_agent() -> String {
    downloader::http_client::CHROME_USER_AGENT.to_string()
}

#[tauri::command]
fn get_default_http_config() -> serde_json::Value {
    let config = downloader::http_client::HttpClientConfig::default();
    serde_json::json!({
        "timeout_secs": config.timeout.as_secs(),
        "connect_timeout_secs": config.connect_timeout.as_secs(),
        "user_agent": config.user_agent,
        "follow_redirects": config.follow_redirects,
        "max_redirects": config.max_redirects,
        "danger_accept_invalid_certs": config.danger_accept_invalid_certs
    })
}

// ============ Retry Strategy Commands ============

#[tauri::command]
fn calculate_retry_backoff(current_delay_ms: u64) -> u64 {
    let config = downloader::network::RetryConfig::default();
    let current = std::time::Duration::from_millis(current_delay_ms);
    let next = downloader::network::calculate_backoff(current, &config);
    next.as_millis() as u64
}

#[tauri::command]
fn get_retry_config() -> serde_json::Value {
    let config = downloader::network::RetryConfig::default();
    serde_json::json!({
        "max_immediate_retries": config.max_immediate_retries,
        "max_delayed_retries": config.max_delayed_retries,
        "initial_delay_ms": config.initial_delay.as_millis() as u64,
        "max_delay_ms": config.max_delay.as_millis() as u64,
        "jitter_factor": config.jitter_factor
    })
}

#[tauri::command]
fn analyze_error_strategy(error_type: String) -> String {
    // Map common error types to retry strategies
    match error_type.as_str() {
        "timeout" => "Delayed(5s)".to_string(),
        "connection" | "connect" => "Immediate".to_string(),
        "forbidden" | "403" => "RefreshLink".to_string(),
        "not_found" | "404" => "Fatal(File Not Found)".to_string(),
        "too_many_requests" | "429" => "Delayed(30s)".to_string(),
        "server_error" | "500" => "Delayed(10s)".to_string(),
        "bad_gateway" | "502" => "Delayed(5s)".to_string(),
        "service_unavailable" | "503" => "Delayed(15s)".to_string(),
        _ => "Delayed(3s)".to_string(),
    }
}

// ============ Range Request Commands ============

#[tauri::command]
async fn start_range_download(url: String, start: u64, end: u64) -> Result<Vec<u8>, String> {
    let settings = settings::load_settings();
    let mut config = downloader::http_client::HttpClientConfig::default();
    config.proxy = Some(crate::proxy::ProxyConfig::from_settings(&settings));
    
    let client = downloader::http_client::build_stealth_client(&config)
        .map_err(|e| e.to_string())?;
    
    let response = client
        .get(&url)
        .header("Range", format!("bytes={}-{}", start, end))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    Ok(bytes.to_vec())
}

#[tauri::command]
async fn validate_download_url(url: String) -> Result<serde_json::Value, String> {
    let settings = settings::load_settings();
    let mut config = downloader::http_client::HttpClientConfig::default();
    config.proxy = Some(crate::proxy::ProxyConfig::from_settings(&settings));

    let client = downloader::http_client::build_client(&config)
        .map_err(|e| e.to_string())?;
    
    let scout = downloader::http_client::FirstByteScout::new(client);
    let caps = scout.probe(&url).await?;
    
    Ok(serde_json::json!({
        "supports_range": caps.supports_range,
        "valid_content": caps.valid_content,
        "content_length": caps.content_length,
        "content_type": caps.content_type,
        "etag": caps.etag,
        "last_modified": caps.last_modified,
        "recommended_segments": caps.recommended_segments,
        "ignores_range": caps.ignores_range
    }))
}

// ============ Plugin Extraction Commands ============

#[tauri::command]
async fn extract_stream_url(app_handle: tauri::AppHandle, script: String, page_url: String) -> Result<Option<serde_json::Value>, String> {
    let client = rquest::Client::builder()
        // .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| e.to_string())?;
    
    let host = plugin_vm::lua_host::LuaPluginHost::new(client, app_handle);
    host.init().await.map_err(|e| e.to_string())?;
    host.load_script(&script).await.map_err(|e| e.to_string())?;
    
    match host.extract_stream(&page_url).await {
        Ok(Some(result)) => Ok(Some(serde_json::json!({
            "url": result.url,
            "cookies": result.cookies,
            "headers": result.headers,
            "filename": result.filename
        }))),
        Ok(None) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

// ============ Proxy Bypass Commands ============

#[tauri::command]
fn should_bypass_proxy(url: String) -> bool {
    let config = proxy::ProxyConfig::default();
    config.should_bypass(&url)
}

#[tauri::command]
fn is_proxy_enabled() -> bool {
    let config = proxy::ProxyConfig::default();
    config.is_enabled()
}

// ============ Download Stats Commands ============

#[tauri::command]
fn get_download_stats(file_size: u64, segment_count: u32) -> serde_json::Value {
    let manager = downloader::manager::DownloadManager::new(file_size, segment_count);
    let stats = manager.get_stats();
    serde_json::json!({
        "total_segments": stats.total_segments,
        "active_segments": stats.active_segments,
        "complete_segments": stats.complete_segments,
        "total_speed_bps": stats.total_speed_bps,
        "downloaded_bytes": stats.downloaded_bytes,
        "total_bytes": stats.total_bytes,
        "progress_percent": stats.progress_percent
    })
}

#[tauri::command]
fn get_work_steal_config() -> serde_json::Value {
    let config = downloader::structures::WorkStealConfig::default();
    serde_json::json!({
        "min_split_size": config.min_split_size,
        "steal_ratio": config.steal_ratio,
        "speed_threshold_ratio": config.speed_threshold_ratio
    })
}

// ============ Retry State Commands ============

lazy_static::lazy_static! {
    static ref RETRY_STATE: std::sync::Mutex<downloader::network::RetryState> = 
        std::sync::Mutex::new(downloader::network::RetryState::default());
}

#[tauri::command]
fn get_retry_state() -> serde_json::Value {
    let state = RETRY_STATE.lock().unwrap();
    serde_json::json!({
        "immediate_attempts": state.immediate_attempts,
        "delayed_attempts": state.delayed_attempts,
        "current_delay_ms": state.current_delay.as_millis() as u64,
        "last_error": state.last_error
    })
}

#[tauri::command]
fn reset_retry_state() {
    let mut state = RETRY_STATE.lock().unwrap();
    state.reset();
}

#[tauri::command]
fn analyze_network_error(error_type: String) -> String {
    // Use a mock error to demonstrate analyze_error
    let strategy = match error_type.as_str() {
        "timeout" => downloader::network::RetryStrategy::Delayed(std::time::Duration::from_secs(5)),
        "connection" => downloader::network::RetryStrategy::Immediate,
        "forbidden" => downloader::network::RetryStrategy::RefreshLink,
        "not_found" => downloader::network::RetryStrategy::Fatal("File Not Found".to_string()),
        _ => downloader::network::RetryStrategy::Delayed(std::time::Duration::from_secs(3)),
    };
    format!("{:?}", strategy)
}

// ============ Resume File Commands ============

#[tauri::command]
fn open_file_for_resume(path: String) -> Result<u64, String> {
    let file = downloader::disk::open_for_resume(std::path::Path::new(&path))
        .map_err(|e| e.to_string())?;
    let size = file.metadata()
        .map(|m| m.len())
        .map_err(|e| e.to_string())?;
    Ok(size)
}

// ============ Plugin Config Commands ============

#[tauri::command]
async fn set_plugin_config(app_handle: tauri::AppHandle, script: String, config: std::collections::HashMap<String, String>) -> Result<(), String> {
    let client = rquest::Client::builder()
        // .min_tls_version(rquest::Version::TLS_1_2)
        .build()
        .map_err(|e| e.to_string())?;
    
    let host = plugin_vm::lua_host::LuaPluginHost::new(client, app_handle);
    host.init().await.map_err(|e| e.to_string())?;
    host.load_script(&script).await.map_err(|e| e.to_string())?;
    host.set_config(config).await.map_err(|e| e.to_string())
}

// ============ DiskWriter Stats Commands ============

#[tauri::command]
fn get_disk_writer_config() -> serde_json::Value {
    let config = downloader::disk::DiskWriterConfig::default();
    serde_json::json!({
        "max_pending_writes": config.max_pending_writes,
        "coalesce_threshold": config.coalesce_threshold,
        "use_sparse": config.use_sparse
    })
}

#[tauri::command]
async fn refresh_download_url(state: State<'_, AppState>, app_handle: tauri::AppHandle, id: String, new_url: String) -> Result<(), String> {
    println!("DEBUG: Refreshing URL for {}: {}", id, new_url);
    
    // 1. Update in persistence
    let mut downloads = persistence::load_downloads().unwrap_or_default();
    if let Some(download) = downloads.iter_mut().find(|d| d.id == id) {
        download.url = new_url.clone();
        download.status = "Paused".to_string(); // Reset error state to Paused so it can be resumed
        persistence::save_downloads(&downloads).map_err(|e| e.to_string())?;
    } else {
        return Err("Download not found".to_string());
    }

    // 2. Stop active session if any
    {
        let mut active_downloads = state.downloads.lock().unwrap();
        if let Some(session) = active_downloads.remove(&id) {
            // Signal stop
            let _ = session.stop_tx.send(());
            println!("DEBUG: Stopped active session for refresh: {}", id);
        }
    }
    
    // 3. Emit event
    app_handle.emit("download_refreshed", serde_json::json!({
        "id": id,
        "url": new_url
    })).map_err(|e| e.to_string())?;

    Ok(())
}






#[tauri::command]
async fn install_plugin(app_handle: tauri::AppHandle, url: String, filename: Option<String>) -> Result<String, String> {

    plugin_vm::updater::install_plugin_from_url(&app_handle, url, filename).await
}

#[tauri::command]
async fn move_download_item(id: String, direction: String) -> Result<(), String> {
    persistence::move_download(&id, &direction)
}

#[tauri::command]
fn set_chaos_config(latency_ms: u64, error_rate: u64, enabled: bool) {
    crate::network::chaos::GLOBAL_CHAOS.update(enabled, latency_ms, error_rate);
}

#[tauri::command]
fn get_chaos_config() -> serde_json::Value {
    // Return simple JSON
    serde_json::json!({
        "enabled": crate::network::chaos::GLOBAL_CHAOS.enabled.load(std::sync::atomic::Ordering::Relaxed),
        "latency_ms": crate::network::chaos::GLOBAL_CHAOS.latency_ms.load(std::sync::atomic::Ordering::Relaxed),
        "error_rate": crate::network::chaos::GLOBAL_CHAOS.error_rate_percent.load(std::sync::atomic::Ordering::Relaxed)
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Load settings and apply
    let initial_settings = settings::load_settings();
    speed_limiter::GLOBAL_LIMITER.set_limit(initial_settings.speed_limit_kbps * 1024);
    clipboard::CLIPBOARD_MONITOR.set_enabled(initial_settings.clipboard_monitor);
    
    // Auto-start Tor if enabled
    if initial_settings.use_tor {
        tauri::async_runtime::spawn(async {
            println!("Tor enabled in settings, initializing...");
            if let Err(e) = crate::network::tor::init_tor().await {
                eprintln!("Failed to auto-init Tor: {}", e);
            }
        });
    }

    // Create channel for HTTP server to send download requests
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<http_server::DownloadRequest>();
    
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // System Tray Setup
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let show_i = MenuItem::with_id(app, "show", "Show HyperStream", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &quit_i])?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "quit" => {
                        app.exit(0);
                    }
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| match event {
                    TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left,
                        ..
                    } => {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    _ => {}
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                // Minimize to tray instead of closing for the main window
                if window.label() == "main" {
                    let _ = window.hide();
                    api.prevent_close();
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            start_download, 
            pause_download, 
            get_downloads, 
            remove_download_entry, 
            get_settings, 
            save_settings, 
            open_file, 
            open_folder,
            schedule_download,
            get_scheduled_downloads,
            cancel_scheduled_download,
            crawl_website,
            mux_video_audio,
            check_ffmpeg_installed,
            decrypt_aes_128,
            test_browser_fingerprint,
            get_proxy_config,
            test_proxy,
            acquire_bandwidth,
            set_speed_limit,
            get_speed_limit,
            // Plugin Commands
            get_all_plugins,
            reload_plugins,
            // Old commands
            // get_plugin_metadata, 
            // set_plugin_config,
            // install_plugin,
            generate_lan_pairing_code,
            get_lan_pairing_qr_data,
            get_local_ip,
            is_quiet_hours,
            get_time_info,
            remove_scheduled_download,
            force_start_scheduled_download,
            get_plugin_metadata,
            get_adaptive_thread_count,
            update_thread_count,
            add_bandwidth_sample,
            get_average_bandwidth,
            set_plugin_config,
            install_plugin,
            move_download_item,
            refresh_download_url,
            extract_stream_url,
            should_bypass_proxy,
            is_proxy_enabled,
            get_download_stats,
            get_work_steal_config,
            get_retry_state,
            reset_retry_state,
            analyze_network_error,
            open_file_for_resume,
            get_disk_writer_config,
            add_magnet_link,
            play_torrent,
            get_torrents,
            set_chaos_config,
            get_chaos_config,
            // HLS/Dash Commands
            parse_hls_stream,
            parse_dash_manifest,
            // Network Validation Commands
            analyze_http_status,
            check_captive_portal,
            validate_http_response,
            parse_retry_after_header,
            check_error_content_type,
            get_chrome_user_agent,
            get_default_http_config,
            calculate_retry_backoff,
            get_retry_config,
            analyze_error_strategy,
            start_range_download,
            validate_download_url,
            // Disk Commands
            preallocate_download_file,
            check_file_exists,
            get_file_size,
            read_file_bytes_at_offset,
            get_next_download_time,
            // ZIP Commands
            read_zip_last_bytes,
            preview_zip_partial,
            preview_zip_file,
            // Feeds
            fetch_feed,          // Q5
            get_feeds,
            add_feed,
            remove_feed,
            extract_single_file,
            preview_zip_remote,  // Remote Q3
            download_zip_entry,  // Remote Q3
            export_data,         // Q4
            import_data,         // Q4
            // Feeds
            fetch_feed,           // Q5
            perform_search,
            // mount_drive,
            // unmount_drive,
            upload_to_cloud,
            process_media,
            init_tor_network,
            get_tor_status,
            perform_semantic_search,
            index_all_downloads,
            join_workspace,
            get_plugin_source,
            save_plugin_source,
            delete_plugin,
            // Audio Settings Commands
            get_audio_enabled,
            set_audio_enabled,
            get_audio_volume,
            set_audio_volume,
            play_test_sound,
            // Webhook Commands
            get_webhooks,
            add_webhook,
            update_webhook,
            delete_webhook,
            test_webhook,
            // Archive Commands
            detect_archive,
            extract_archive,
            cleanup_archive,
            check_unrar_available,
            // P2P Commands
            create_p2p_share,
            join_p2p_share,
            list_p2p_sessions,
            close_p2p_session,
            get_p2p_stats
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                window.hide().unwrap();
                api.prevent_close();
            }
        })
        .setup(move |app| {
            let handle = app.handle().clone();
            
            clipboard::CLIPBOARD_MONITOR.start(app.handle().clone());
            scheduler::start_scheduler(app.handle().clone());
            
            tauri::async_runtime::spawn(async move {
                let lan_server = lan_api::LanApiServer::new(8765);
                if let Err(e) = lan_server.start().await {
                    eprintln!("LAN API server error: {}", e);
                }
            });
            
            // Init P2P node
            let p2p_node = tauri::async_runtime::block_on(async {
                network::p2p::P2PNode::new(14735).await.unwrap_or_else(|e| {
                    println!("Warning: P2P failed to start: {}", e);
                    panic!("P2P Init Failed: {}", e);
                })
            });
            let p2p_node = Arc::new(p2p_node);
            
            let p2p_file_map: crate::http_server::FileMap = Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
            
            let torrent_manager = tauri::async_runtime::block_on(async {
                 let path = std::path::PathBuf::from("C:\\Users\\aditya\\Desktop\\Torrents");
                 std::fs::create_dir_all(&path).unwrap_or_default();
                 network::bittorrent::manager::TorrentManager::new(path).await.unwrap_or_else(|e| {
                     println!("Warning: Torrent Manager failed: {}", e);
                     panic!("Torrent Init Failed: {}", e);
                 })
            });
            let torrent_manager = Arc::new(torrent_manager);

            // Spawn HTTP server
            let tx_clone = tx.clone();
            let map_clone = p2p_file_map.clone();
            let tm_clone = torrent_manager.clone();
            tauri::async_runtime::spawn(async move {
                crate::http_server::start_server(tx_clone, map_clone, tm_clone).await;
            });

            // Spawn Game Mode Monitor
            tauri::async_runtime::spawn(async move {
                crate::system_monitor::run_game_mode_monitor().await;
            });
            
            // ============ SYSTEM TRAY ============
            let quit_i = MenuItem::with_id(app.handle(), "quit", "Quit", true, None::<&str>).unwrap();
            let show_i = MenuItem::with_id(app.handle(), "show", "Show HyperStream", true, None::<&str>).unwrap();
            let menu = Menu::with_items(app.handle(), &[&show_i, &quit_i]).unwrap();

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .on_menu_event(|app, event| {
                    match event.id.as_ref() {
                        "quit" => {
                            app.exit(0);
                        }
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    match event {
                        TrayIconEvent::Click {
                            button: tauri::tray::MouseButton::Left,
                            ..
                        } => {
                             let app = tray.app_handle();
                             if let Some(window) = app.get_webview_window("main") {
                                 let _ = window.show();
                                 let _ = window.set_focus();
                             }
                        }
                        _ => {}
                    }
                })
                .build(app.handle());
            // =====================================
            
            // Initialize ChatOps
            let settings_arc = std::sync::Arc::new(std::sync::Mutex::new(crate::settings::load_settings()));
            let chatops_manager = std::sync::Arc::new(crate::network::chatops::ChatOpsManager::new(
                settings_arc.clone(),
            ));
            chatops_manager.start();

            // Manage AppState (Matching struct definition)
            app.handle().manage(AppState { 
                 downloads: Mutex::new(HashMap::new()),
                 p2p_node: p2p_node.clone(),
                 p2p_file_map: p2p_file_map.clone(),
                 torrent_manager: torrent_manager.clone(),
                 connection_manager: network::connection_manager::ConnectionManager::default(),
                 chatops_manager: chatops_manager.clone(),
            });

            // Initialize Plugin Manager
            let plugin_manager = crate::plugin_vm::manager::PluginManager::new(app.handle().clone());
            // Start async load
            let pm_clone = std::sync::Arc::new(plugin_manager);
            app.handle().manage(pm_clone.clone());
            
            tauri::async_runtime::spawn(async move {
                if let Err(e) = pm_clone.load_plugins().await {
                   eprintln!("Failed to load plugins: {}", e);
                }
            });
            
            tauri::async_runtime::spawn(async move {
                while let Some(req) = rx.recv().await {
                    println!("DEBUG: Processing download from extension: {}", req.url);
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

// ============ Plugin Manager Commands ============

#[tauri::command]
async fn get_all_plugins(
    pm: State<'_, std::sync::Arc<crate::plugin_vm::manager::PluginManager>>
) -> Result<Vec<crate::plugin_vm::lua_host::PluginMetadata>, String> {
    Ok(pm.get_plugins_list())
}

#[tauri::command]
async fn reload_plugins(
    pm: State<'_, std::sync::Arc<crate::plugin_vm::manager::PluginManager>>
) -> Result<(), String> {
    pm.load_plugins().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_plugin_source(filename: String) -> Result<String, String> {
    let mut path = std::env::current_dir().unwrap_or_default().join("plugins");
    path.push(format!("{}.lua", filename)); // Append extension if missing? Assuming filename is without ext?
    // Start with safe check
    if !path.exists() {
        return Err("Plugin file not found".to_string());
    }
    std::fs::read_to_string(path).map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_plugin_source(filename: String, content: String) -> Result<(), String> {
    let mut path = std::env::current_dir().unwrap_or_default().join("plugins");
    if !path.exists() {
        std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    }
    path.push(format!("{}.lua", filename));
    std::fs::write(path, content).map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_plugin(filename: String) -> Result<(), String> {
    let mut path = std::env::current_dir().unwrap_or_default().join("plugins");
    path.push(format!("{}.lua", filename));
    if path.exists() {
        std::fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ============ Audio Settings Commands ============
#[tauri::command]
async fn get_audio_enabled() -> bool {
    audio_events::AUDIO_PLAYER.is_enabled().await
}

#[tauri::command]
async fn set_audio_enabled(enabled: bool) -> Result<(), String> {
    audio_events::AUDIO_PLAYER.set_enabled(enabled).await;
    Ok(())
}

#[tauri::command]
async fn get_audio_volume() -> f32 {
    audio_events::AUDIO_PLAYER.get_volume().await
}

#[tauri::command]
async fn set_audio_volume(volume: f32) -> Result<(), String> {
    audio_events::AUDIO_PLAYER.set_volume(volume).await;
    Ok(())
}

#[tauri::command]
async fn play_test_sound(sound_type: String) -> Result<(), String> {
    let event = match sound_type.as_str() {
        "success" => audio_events::SoundEvent::DownloadComplete,
        "error" => audio_events::SoundEvent::DownloadError,
        "start" => audio_events::SoundEvent::DownloadStart,
        _ => return Err(format!("Unknown sound type: {}", sound_type)),
    };
    
    audio_events::AUDIO_PLAYER.play(event).await;
    Ok(())
}

// ============ Webhook Commands ============
#[tauri::command]
async fn get_webhooks() -> Result<Vec<webhooks::WebhookConfig>, String> {
    let settings = settings::load_settings();
    Ok(settings.webhooks.unwrap_or_default())
}

#[tauri::command]
async fn add_webhook(config: webhooks::WebhookConfig) -> Result<(), String> {
    let mut settings = settings::load_settings();
    let mut webhooks = settings.webhooks.unwrap_or_default();
    webhooks.push(config);
    settings.webhooks = Some(webhooks);
    settings::save_settings(&settings)
}

#[tauri::command]
async fn update_webhook(id: String, config: webhooks::WebhookConfig) -> Result<(), String> {
    let mut settings = settings::load_settings();
    let mut webhooks = settings.webhooks.unwrap_or_default();
    
    if let Some(webhook) = webhooks.iter_mut().find(|w| w.id == id) {
        *webhook = config;
        settings.webhooks = Some(webhooks);
        settings::save_settings(&settings)
    } else {
        Err("Webhook not found".to_string())
    }
}

#[tauri::command]
async fn delete_webhook(id: String) -> Result<(), String> {
    let mut settings = settings::load_settings();
    let mut webhooks = settings.webhooks.unwrap_or_default();
    webhooks.retain(|w| w.id != id);
    settings.webhooks = Some(webhooks);
    settings::save_settings(&settings)
}

#[tauri::command]
async fn test_webhook(id: String) -> Result<(), String> {
    let settings = settings::load_settings();
    let webhooks = settings.webhooks.unwrap_or_default();
    
    let config = webhooks.iter()
        .find(|w| w.id == id)
        .ok_or("Webhook not found")?;
    
    let payload = webhooks::WebhookPayload {
        event: "DownloadComplete".to_string(),
        download_id: "test_123".to_string(),
        filename: "test_file.zip".to_string(),
        url: "https://example.com/test.zip".to_string(),
        size: 104857600, // 100 MB
        speed: 10485760, // 10 MB/s
        filepath: Some("C:\\Downloads\\test_file.zip".to_string()),
        timestamp: chrono::Utc::now().timestamp(),
    };
    
    let manager = webhooks::WebhookManager::new();
    manager.load_configs(vec![config.clone()]).await;
    manager.trigger(webhooks::WebhookEvent::DownloadComplete, payload).await;
    
    Ok(())
}

// ============ Archive Commands ============
#[tauri::command]
async fn detect_archive(path: String) -> Option<archive_manager::ArchiveInfo> {
    archive_manager::ArchiveManager::detect_archive(&path)
}

#[tauri::command]
async fn extract_archive(archive_path: String, dest_dir: Option<String>) -> Result<String, String> {
    // Use same directory as archive if dest not specified
    let dest = if let Some(d) = dest_dir {
        d
    } else {
        std::path::Path::new(&archive_path)
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or(".")
            .to_string()
    };
    
    archive_manager::ArchiveManager::extract_archive(&archive_path, &dest)
}

#[tauri::command]
async fn cleanup_archive(archive_path: String) -> Result<(), String> {
    archive_manager::ArchiveManager::cleanup_archive(&archive_path)
}

#[tauri::command]
fn check_unrar_available() -> bool {
    archive_manager::ArchiveManager::check_unrar_available()
}

// ============ P2P Commands ============
#[tauri::command]
async fn create_p2p_share(
    download_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<network::p2p::P2PShareSession, String> {
    let p2p = state.p2p_node.clone();
    p2p.create_share_session(download_id).await
}

#[tauri::command]
async fn join_p2p_share(
    code: String,
    peer_addr: String,
    state: tauri::State<'_, AppState>,
) -> Result<network::p2p::P2PShareSession, String> {
    let p2p = state.p2p_node.clone();
    p2p.join_share_session(code, peer_addr).await
}

#[tauri::command]
fn list_p2p_sessions(state: tauri::State<'_, AppState>) -> Vec<network::p2p::P2PShareSession> {
    state.p2p_node.list_sessions()
}

#[tauri::command]
fn close_p2p_session(session_id: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.p2p_node.close_session(&session_id)
}

#[tauri::command]
fn get_p2p_stats(state: tauri::State<'_, AppState>) -> network::p2p::P2PStats {
    state.p2p_node.get_stats()
}

// Old dummy commands removed
