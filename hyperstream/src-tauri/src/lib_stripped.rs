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
pub mod mqtt_client;
mod doi_resolver;
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
mod metadata_scrubber;
mod ephemeral_server;
mod wayback;
mod docker_pull;
mod power_manager;
pub mod cas_manager;
mod warc_archiver;
mod git_lfs;
mod sandbox;
mod notarize;
mod mirror_hunter;
mod usb_flasher;
mod api_replay;
mod c2pa_validator;
mod bandwidth_arb;
mod stego_vault;
mod tui_dashboard;
mod auto_extract;
mod ipfs_gateway;
mod sql_query;
mod dlna_cast;
mod qos_manager;
mod mod_optimizer;
mod rclone_bridge;
mod subtitle_gen;
mod virtual_drive;
mod geofence;

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














pub async fn start_download_impl(
    app: &tauri::AppHandle,
    state: &AppState,
    id: String, 
    url: String, 
    path: String,
    _resume_override: Option<u64>,
    custom_headers: Option<std::collections::HashMap<String, String>>
) -> Result<(), String> {
    
    // Play start sound
    crate::media::sounds::play_startup();
    
    // VPN Auto-Connect (Tier 1)
    {
        let settings = settings::load_settings();
        if settings.vpn_auto_connect {
            if let Some(ref vpn_name) = settings.vpn_connection_name {
                if !vpn_name.trim().is_empty() {
                    let status = std::process::Command::new("rasdial")
                        .arg(vpn_name)
                        .status()
                        .map_err(|e| format!("Failed to execute rasdial: {}", e))?;
                    
                    if !status.success() {
                        eprintln!("WARNING: VPN auto-connect to '{}' failed with status: {:?}", vpn_name, status);
                    } else {
                    }
                }
            }
        }
    }
    
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
    
    // Trigger MQTT for download start
    crate::mqtt_client::publish_event(
        "DownloadStart",
        &id,
        path.split('\\').last().unwrap_or(&path),
        "Downloading"
    );

    // 1. Check for saved download (Resume logic)
    let saved_downloads = persistence::load_downloads().unwrap_or_default();
    let saved = saved_downloads.iter().find(|d| d.id == id);
    let resume_from: u64 = saved.map(|s| s.downloaded_bytes).unwrap_or(0);
    
    if resume_from > 0 {
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
        network::masq::build_impersonator_client(network::masq::BrowserProfile::Chrome, Some(&proxy_config), custom_headers.clone())
    } else {
        network::masq::build_client(Some(&proxy_config), custom_headers.clone())
    }.map_err(|e| e.to_string())?;

    let mut actual_url = url.clone();
    let (mut total_size, mut etag, mut md5) = downloader::initialization::determine_total_size(&client, &actual_url).await?;

    // Git LFS Accelerator check
    if total_size > 0 && total_size < 1024 * 5 {
        if let Ok(res) = client.get(&actual_url).send().await {
            if let Ok(text) = res.text().await {
                if let Some(new_url) = crate::git_lfs::resolve_lfs_pointer(&actual_url, &text).await {
                    actual_url = new_url;
                    // Re-determine size
                    let sz_res = downloader::initialization::determine_total_size(&client, &actual_url).await;
                    if let Ok((sz, et, md)) = sz_res {
                        total_size = sz;
                        etag = et;
                        md5 = md;
                    }
                }
            }
        }
    }

    // Check CAS Deduplication
    if let Some(existing_path) = crate::cas_manager::check_cas(etag.as_deref(), md5.as_deref()) {
        println!("CAS Match Found! Hardlinking from {}", existing_path);
        // Attempt to hardlink
        if std::fs::hard_link(&existing_path, &path).is_ok() {
            println!("Hardlink successful for {}", path);
            
            // Register success... emit completion... and return
            let _ = app.emit("download_progress", Payload {
                id: id.clone(),
                downloaded: total_size,
                total: total_size,
                segments: vec![],
            });
            
            // Persistence
            let mut saved_downloads = persistence::load_downloads().unwrap_or_default();
            if let Some(d) = saved_downloads.iter_mut().find(|d| d.id == id) {
                d.status = "Completed".to_string();
                d.downloaded_bytes = total_size;
            }
            let _ = persistence::save_downloads(&saved_downloads);
            
            crate::media::sounds::play_complete();
            return Ok(());
        } else {
            println!("Failed to create hardlink, falling back to download");
        }
    }
    // 3. Initialize File
    let file = downloader::initialization::setup_file(&path, resume_from, total_size)?;
    let file_mutex = file;

    // Register P2P
    {
        let mut map = state.p2p_file_map.lock().unwrap();
        map.insert(id.clone(), StreamingSource::FileSystem(std::path::PathBuf::from(&path)));
    }
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
    let url_monitor = actual_url.clone();
    let path_monitor = path.clone();
    let etag_monitor = etag.clone();
    let md5_monitor = md5.clone();
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
                        crate::cas_manager::register_cas(etag_monitor.as_deref(), md5_monitor.as_deref(), &path_monitor);
                        
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
                        
                        // Trigger MQTT for download complete
                        crate::mqtt_client::publish_event(
                            "DownloadComplete",
                            &id_monitor,
                            path_monitor.split('\\').last().unwrap_or(&path_monitor),
                            "Complete"
                        );
                        
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
                     
                     // Trigger MQTT for download error (Hot-Swap needed)
                     crate::mqtt_client::publish_event(
                         "DownloadError",
                         &id_worker,
                         path_worker.split('\\').last().unwrap_or(&path_worker),
                         "WaitingForRefresh"
                     );
                     
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























// ============ Spider / Site Grabber Commands ============




// ============ ZIP Preview Commands ============














// ============ HLS/DASH Stream Parser Commands ============
















// ============ Muxer Commands ============










// ============ Proxy Configuration Commands ============





// ============ Virtual Drive Commands ============
// 

// 

// ============ Cloud Commands ============


// ============ Media Commands ============



// ============ Import/Export Commands (Disabled) ============
/*







*/


// ============ Virus Scanning Commands (Disabled) ============
/*



*/

// ============ Speed Limiter Commands ============







// ============ LAN API Commands ============








// ============ Scheduler Helper Commands ============














// ============ Plugin System Commands ============



// ============ Network Validation Commands ============





// ============ Disk Operation Commands ============







// ============ Adaptive Thread Commands ============

lazy_static::lazy_static! {
    static ref THREAD_CONTROLLER: adaptive_threads::AdaptiveThreadController = 
        adaptive_threads::AdaptiveThreadController::new(2, 16);
    static ref BANDWIDTH_MONITOR: adaptive_threads::BandwidthMonitor = 
        adaptive_threads::BandwidthMonitor::new(5);
}









// ============ More Scheduler Commands ============



// ============ FDM Import Command (Disabled) ============
/*

*/

// ============ Feeds Commands ============










// ============ Tray & Setup ============



// ============ ZIP Extraction Commands ============



// ============ HTTP Client Commands ============







// ============ Stealth HTTP Client Commands ============





// ============ Retry Strategy Commands ============







// ============ Range Request Commands ============





// ============ Plugin Extraction Commands ============



// ============ Proxy Bypass Commands ============





// ============ Download Stats Commands ============





// ============ Retry State Commands ============

lazy_static::lazy_static! {
    static ref RETRY_STATE: std::sync::Mutex<downloader::network::RetryState> = 
        std::sync::Mutex::new(downloader::network::RetryState::default());
}







// ============ Resume File Commands ============



// ============ Plugin Config Commands ============



// ============ DiskWriter Stats Commands ============














































































#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Load settings and apply
    let initial_settings = settings::load_settings();
    speed_limiter::GLOBAL_LIMITER.set_limit(initial_settings.speed_limit_kbps * 1024);
    clipboard::CLIPBOARD_MONITOR.set_enabled(initial_settings.clipboard_monitor);
    
    // Load custom sounds from settings (Z1)
    let custom_sound_settings = initial_settings.clone();
    tauri::async_runtime::spawn(async move {
        audio_events::AUDIO_PLAYER.load_custom_sounds_from_settings(&custom_sound_settings).await;
    });
    
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
            get_p2p_stats,
            // P2P Upload Limit
            set_p2p_upload_limit,
            get_p2p_upload_limit,
            // Custom Sound Files
            set_custom_sound_path,
            clear_custom_sound_path,
            get_custom_sound_paths,
            // Metadata Scrubber
            scrub_metadata,
            get_file_metadata,
            // Ephemeral Web Server
            start_ephemeral_share,
            stop_ephemeral_share,
            list_ephemeral_shares,
            // Wayback Machine
            check_wayback_availability,
            get_wayback_url,
            // DOI Resolver
            doi_resolver::resolve_doi,
            // Docker Image Puller
            docker_pull::fetch_docker_manifest,
            download_as_warc,
            run_in_sandbox,
            notarize_file,
            verify_notarization,
            find_mirrors,
            list_usb_drives,
            flash_to_usb,
            replay_request,
            fuzz_url,
            validate_c2pa,
            arbitrage_download,
            stego_hide,
            stego_extract,
            launch_tui_dashboard,
            auto_extract_archive,
            download_ipfs,
            query_file,
            discover_dlna,
            cast_to_dlna,
            set_download_priority,
            get_qos_stats,
            optimize_mods,
            rclone_list_remotes,
            rclone_transfer,
            generate_subtitles,
            mount_drive,
            unmount_drive,
            list_virtual_drives,
            set_geofence_rule,
            get_geofence_rules
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
            
            // Smart Sleep + Battery Polling
            let battery_app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
                loop {
                    interval.tick().await;
                    let settings = crate::settings::load_settings();
                    let state = battery_app_handle.state::<AppState>();
                    
                    let active_count = {
                        let downloads = state.downloads.lock().unwrap();
                        downloads.len()
                    };
                    
                    if settings.prevent_sleep_during_download {
                        crate::power_manager::prevent_sleep(active_count > 0);
                    } else {
                        crate::power_manager::prevent_sleep(false);
                    }
                    
                    if settings.pause_on_low_battery {
                        if let Some(pct) = crate::power_manager::get_battery_percentage() {
                            if pct <= 15 && active_count > 0 {
                                println!("🔋 Battery critical ({}%). Pausing all downloads.", pct);
                                let mut to_pause = Vec::new();
                                {
                                    let downloads = state.downloads.lock().unwrap();
                                    to_pause = downloads.keys().cloned().collect();
                                }
                                
                                let mut saved_downloads = crate::persistence::load_downloads().unwrap_or_default();
                                let mut did_pause = false;
                                
                                for id in to_pause {
                                    let mut downloads = state.downloads.lock().unwrap();
                                    if let Some(session) = downloads.remove(&id) {
                                        let _ = session.stop_tx.send(());
                                        if let Some(d) = saved_downloads.iter_mut().find(|d| d.id == id) {
                                            d.status = "Paused".to_string();
                                        }
                                        did_pause = true;
                                    }
                                }
                                
                                if did_pause {
                                    let _ = crate::persistence::save_downloads(&saved_downloads);
                                }
                            }
                        }
                    }
                }
            });
            
            tauri::async_runtime::spawn(async move {
                while let Some(req) = rx.recv().await {
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











// ============ Audio Settings Commands ============










// ============ Webhook Commands ============










// ============ Archive Commands ============








// ============ P2P Commands ============










// Old dummy commands removed

// ============ P2P Upload Limit Commands (G1) ============




// ============ Custom Sound File Commands (Z1) ============






// ============ Metadata Scrubber Commands ============




// ============ Ephemeral Web Server Commands ============






// ============ Wayback Machine Commands ============



