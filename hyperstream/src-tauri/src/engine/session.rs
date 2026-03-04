use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::broadcast;
use crate::core_state::*;
use crate::*;

/// Extract filename from a path string, handling both Unix and Windows separators.
/// Falls back to the full path string if no filename component can be extracted.
fn extract_filename(path: &str) -> &str {
    std::path::Path::new(path)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or(path)
}

pub(crate) async fn start_download_impl(
    app: &tauri::AppHandle,
    state: &AppState,
    id: String, 
    url: String, 
    path: String,
    _resume_override: Option<u64>,
    custom_headers: Option<std::collections::HashMap<String, String>>
) -> Result<(), String> {
    println!("DEBUG: Starting download ID: {}", id);
    
    // Play start sound
    crate::media::sounds::play_startup();
    
    // Load settings once for the entire download initialization
    let settings = settings::load_settings();
    
    // VPN Auto-Connect (Tier 1)
    {
        if settings.vpn_auto_connect {
            if let Some(ref vpn_name) = settings.vpn_connection_name {
                if !vpn_name.trim().is_empty() {
                    println!("DEBUG: Auto-connecting to VPN: {}", vpn_name);
                    #[cfg(target_os = "windows")]
                    {
                        let status = tokio::process::Command::new("rasdial")
                            .arg(vpn_name)
                            .status()
                            .await
                            .map_err(|e| format!("Failed to execute rasdial: {}", e))?;
                        
                        if !status.success() {
                            eprintln!("WARNING: VPN auto-connect to '{}' failed with status: {:?}", vpn_name, status);
                        } else {
                            println!("DEBUG: Successfully connected to VPN: {}", vpn_name);
                        }
                    }
                    #[cfg(target_os = "linux")]
                    {
                        let status = tokio::process::Command::new("nmcli")
                            .args(["connection", "up", vpn_name])
                            .status()
                            .await
                            .map_err(|e| format!("Failed to execute nmcli: {}", e))?;
                        
                        if !status.success() {
                            eprintln!("WARNING: VPN auto-connect to '{}' failed with status: {:?}", vpn_name, status);
                        } else {
                            println!("DEBUG: Successfully connected to VPN: {}", vpn_name);
                        }
                    }
                    #[cfg(target_os = "macos")]
                    {
                        let status = tokio::process::Command::new("networksetup")
                            .args(["-connectpppoeservice", vpn_name])
                            .status()
                            .await
                            .map_err(|e| format!("Failed to execute networksetup: {}", e))?;
                        
                        if !status.success() {
                            eprintln!("WARNING: VPN auto-connect to '{}' failed with status: {:?}", vpn_name, status);
                        } else {
                            println!("DEBUG: Successfully connected to VPN: {}", vpn_name);
                        }
                    }
                }
            }
        }
    }
    
    // Trigger webhooks for download start
    {
        if let Some(ref webhooks) = settings.webhooks {
            let manager = webhooks::WebhookManager::new();
            manager.load_configs(webhooks.clone()).await;
            let payload = webhooks::WebhookPayload {
                event: "DownloadStart".to_string(),
                download_id: id.clone(),
                filename: extract_filename(&path).to_string(),
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
        extract_filename(&path),
        "Downloading"
    );

    // Broadcast to LAN devices
    crate::lan_api::broadcast_download(url.clone());

    // 1. Check for saved download (Resume logic)
    let saved_downloads = persistence::load_downloads().unwrap_or_default();
    let saved = saved_downloads.iter().find(|d| d.id == id);
    let resume_from: u64 = saved.map(|s| s.downloaded_bytes).unwrap_or(0);
    
    if resume_from > 0 {
        println!("DEBUG: Resuming from byte {}", resume_from);
    }

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
                    
                    // Create dir so canonicalize works, then validate path is within download dir
                    std::fs::create_dir_all(&category_path).ok();
                    if let (Ok(canon_dl), Ok(canon_cat)) = (
                        dunce::canonicalize(&settings.download_dir),
                        dunce::canonicalize(&category_path),
                    ) {
                        if !canon_cat.starts_with(&canon_dl) {
                            eprintln!("WARNING: Category path {:?} escapes download dir, ignoring rule", category_path);
                            continue;
                        }
                    }
                    
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
                    println!("DEBUG: Git LFS pointer detected! Swapping to real binaries via Batch API.");
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
        let mut map = state.p2p_file_map.lock().unwrap_or_else(|e| e.into_inner());
        map.insert(id.clone(), StreamingSource::FileSystem(std::path::PathBuf::from(&path)));
    }
    // P2P file advertising removed (not needed in simplified P2P)

    // 4. Initialize Manager
    let manager = downloader::initialization::setup_manager(total_size, saved, resume_from, settings.segments);
    let downloaded_total = Arc::new(AtomicU64::new(resume_from));
    // let last_progress_emit = Arc::new(Mutex::new(std::time::Instant::now())); // Removed

    // 5. Setup Stop Signal
    let (stop_tx, _) = broadcast::channel(1);

    // 6. Store Session
    {
        let mut downloads = state.downloads.lock().unwrap_or_else(|e| e.into_inner());
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
    let stop_tx_monitor = stop_tx.clone();
    let chatops_monitor = state.chatops_manager.clone();
    
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
                    let segments = manager_monitor.lock().unwrap_or_else(|e| e.into_inner()).get_segments_snapshot();
                    
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

                    if total_size > 0 && d >= total_size {
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
                                    filename: extract_filename(&path_webhook).to_string(),
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
                            extract_filename(&path_monitor),
                            "Complete"
                        );

                        // Notify ChatOps (Telegram) on completion
                        let chatops = chatops_monitor.clone();
                        let filename_chatops = extract_filename(&path_monitor).to_string();
                        tokio::spawn(async move {
                            chatops.notify_completion(&filename_chatops).await;
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
                        
                        // Persist final "Complete" status
                        let saved = persistence::SavedDownload {
                            id: id_monitor.clone(),
                            url: url_monitor.clone(),
                            path: path_monitor.clone(),
                            filename: extract_filename(&path_monitor).to_string(),
                            total_size,
                            downloaded_bytes: total_size,
                            status: "Complete".to_string(),
                            segments: None,
                        };
                        let _ = persistence::upsert_download(saved);
                        // Signal save loop and workers to stop
                        let _ = stop_tx_monitor.send(());
                        
                        break;
                    }
                }
            }
        }
    });

    // 9. Spawn Worker Threads
    let mut handles = Vec::new();
    
    // We need to clone manager segments to iterate
    let segments_count = manager.lock().unwrap_or_else(|e| e.into_inner()).segments.read().unwrap_or_else(|e| e.into_inner()).len();

    for i in 0..segments_count {
        let manager_clone = manager.clone();
        let url_clone = actual_url.clone();
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
        let total_size_worker = total_size; // u64 is Copy

        let handle = tokio::spawn(async move {
            let (start, end, seg_id) = {
                let m = manager_clone.lock().unwrap_or_else(|e| e.into_inner());
                let mut segs = m.segments.write().unwrap_or_else(|e| e.into_inner());
                let seg = &mut segs[i];
                seg.state = crate::downloader::structures::SegmentState::Downloading;
                (seg.start_byte, seg.end_byte, seg.id)
            };

            if end == 0 || start >= end { return; }

            let mut current_pos = start;
            let mut retry_count = 0;
            const MAX_RETRIES: u32 = 5;
            let mut bytes_since_cursor_update: u64 = 0;
            const CURSOR_UPDATE_THRESHOLD: u64 = 256 * 1024; // Update cursor every 256KB

            loop {
                // Check for stop signal
                if stop_rx.try_recv().is_ok() {
                    let m = manager_clone.lock().unwrap_or_else(|e| e.into_inner());
                    let mut segs = m.segments.write().unwrap_or_else(|e| e.into_inner());
                    if let Some(seg) = segs.iter_mut().find(|s| s.id == seg_id) {
                        seg.downloaded_cursor = current_pos;
                        seg.state = crate::downloader::structures::SegmentState::Paused;
                    }
                    break;
                }

                if current_pos >= end {
                    let m = manager_clone.lock().unwrap_or_else(|e| e.into_inner());
                    let mut segs = m.segments.write().unwrap_or_else(|e| e.into_inner());
                    if let Some(seg) = segs.iter_mut().find(|s| s.id == seg_id) {
                        seg.state = crate::downloader::structures::SegmentState::Complete;
                    }
                    break;
                }

                let range_header = format!("bytes={}-{}", current_pos, end - 1);
                
                // Acquire permit via ConnectionManager
                let _permit = cm_clone.acquire(&url_clone).await.ok();

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
                        let m = manager_clone.lock().unwrap_or_else(|e| e.into_inner());
                        let mut segs = m.segments.write().unwrap_or_else(|e| e.into_inner());
                        if let Some(seg) = segs.iter_mut().find(|s| s.id == seg_id) {
                            seg.downloaded_cursor = current_pos;
                            seg.state = crate::downloader::structures::SegmentState::Paused;
                        }
                        break;
                    }
                    r = res_future => r
                };

                let response = match res {
                    Ok(r) => r,
                    Err(e) => {
                        println!("DEBUG: Thread (seg {}) error: {}", seg_id, e);
                        retry_count += 1;
                        if retry_count > MAX_RETRIES { 
                            {
                                let m = manager_clone.lock().unwrap_or_else(|e| e.into_inner());
                                let mut segs = m.segments.write().unwrap_or_else(|e| e.into_inner());
                                if let Some(seg) = segs.iter_mut().find(|s| s.id == seg_id) {
                                    seg.downloaded_cursor = current_pos;
                                    seg.state = crate::downloader::structures::SegmentState::Error;
                                }
                            }
                            crate::media::sounds::play_error();
                            break; 
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        continue;
                    }
                };

                // Check for 403 Forbidden / 410 Gone (Link Expired)
                if response.status() == rquest::StatusCode::FORBIDDEN || response.status() == rquest::StatusCode::GONE {
                     println!("Thread (seg {}) error: Link Expired (403/410). Requesting Hot-Swap.", seg_id);
                     
                     // 1. Stop all threads
                     let _ = stop_tx_clone.send(());

                     // 2. Persist status as "WaitingForRefresh"
                     let segments = manager_clone.lock().unwrap_or_else(|e| e.into_inner()).get_segments_snapshot();
                     let total_downloaded = segments.iter().map(|s| s.downloaded_cursor.saturating_sub(s.start_byte)).sum();
                     
                     let filename_s = std::path::Path::new(&path_worker).file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_else(|| "download".to_string());
                        
                     let saved = persistence::SavedDownload {
                         id: id_worker.clone(),
                         url: url_worker.clone(),
                         path: path_worker.clone(),
                         filename: filename_s,
                         total_size: total_size_worker,
                         downloaded_bytes: total_downloaded,
                         status: "WaitingForRefresh".to_string(),
                         segments: Some(segments),
                     };
                     
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
                                 filename: extract_filename(&path_error).to_string(),
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
                         extract_filename(&path_worker),
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
                            let m = manager_clone.lock().unwrap_or_else(|e| e.into_inner());
                            let mut segs = m.segments.write().unwrap_or_else(|e| e.into_inner());
                            if let Some(seg) = segs.iter_mut().find(|s| s.id == seg_id) {
                                seg.downloaded_cursor = current_pos;
                                seg.state = crate::downloader::structures::SegmentState::Paused;
                            }
                            return; // Exit thread
                        }
                        item = stream.next() => {
                            match item {
                                Some(Ok(chunk)) => {
                                    // Truncate chunk to segment boundary to prevent writing into adjacent segments
                                    let remaining = end.saturating_sub(current_pos) as usize;
                                    let safe_chunk = if chunk.len() > remaining {
                                        &chunk[..remaining]
                                    } else {
                                        &chunk[..]
                                    };
                                    let len = safe_chunk.len() as u64;
                                    if len == 0 { break; }
                                    if tx_clone.send(WriteRequest { offset: current_pos, data: safe_chunk.to_vec(), segment_id: seg_id }).is_err() {
                                        eprintln!("Thread (seg {}): Disk writer channel closed, stopping segment.", seg_id);
                                        return; // Exit worker gracefully instead of panicking
                                    }
                                    current_pos += len;
                                    
                                    // Update global progress ATOMICALLY (Lock-Free)
                                    downloaded_clone.fetch_add(len, Ordering::Relaxed);
                                    
                                    // Periodically update segment cursor for accurate progress/resume
                                    bytes_since_cursor_update += len;
                                    if bytes_since_cursor_update >= CURSOR_UPDATE_THRESHOLD {
                                        bytes_since_cursor_update = 0;
                                        let m = manager_clone.lock().unwrap_or_else(|e| e.into_inner());
                                        let mut segs = m.segments.write().unwrap_or_else(|e| e.into_inner());
                                        if let Some(seg) = segs.iter_mut().find(|s| s.id == seg_id) {
                                            seg.downloaded_cursor = current_pos;
                                        }
                                    }
                                    
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
                    let segments = manager_save.lock().unwrap_or_else(|e| e.into_inner()).get_segments_snapshot();
                    let total_downloaded = segments.iter().map(|s| s.downloaded_cursor.saturating_sub(s.start_byte)).sum();
                    
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
