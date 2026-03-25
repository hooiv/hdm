use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::broadcast;
use crate::core_state::*;
use crate::*;
use crate::mirror_scoring::GLOBAL_MIRROR_SCORER;
use crate::group_scheduler::GLOBAL_GROUP_SCHEDULER;
use crate::download_groups::GroupState;

/// After a download failure or stall: try queue retry; if not retrying, release queue slot,
/// deregister bandwidth, and remove session from AppState. Call once per failed download.
pub(crate) fn handle_download_failure_cleanup(app: &tauri::AppHandle, id: &str) {
    if let Some(app_state) = app.try_state::<crate::core_state::AppState>() {
        app_state.unregister_streaming_source(id);
    }

    if !try_auto_retry(app, id) {
        let mut queue = crate::queue_manager::DOWNLOAD_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
        queue.mark_finished(id);
        drop(queue);
        crate::bandwidth_allocator::ALLOCATOR.deregister(id);
        if let Some(app_state) = app.try_state::<crate::core_state::AppState>() {
            let mut downloads = app_state.downloads.lock().unwrap_or_else(|e| e.into_inner());
            downloads.remove(id);
        }
    } else {
        crate::bandwidth_allocator::ALLOCATOR.deregister(id);
    }
}

/// Try to auto-retry a failed download via the queue system.
/// Returns true if the download was re-queued for retry.
fn try_auto_retry(app: &tauri::AppHandle, id: &str) -> bool {
    use crate::queue_manager::{RETRY_METADATA, DOWNLOAD_QUEUE, QueuedDownload};

    let meta = {
        let mut store = RETRY_METADATA.lock().unwrap_or_else(|e| e.into_inner());
        store.remove(id)
    };

    if let Some(meta) = meta {
        let requeued = {
            let retry_item = QueuedDownload {
                id: id.to_string(),
                url: meta.url,
                path: meta.path,
                priority: meta.priority,
                added_at: chrono::Utc::now().timestamp_millis(),
                custom_headers: meta.custom_headers,
                expected_checksum: meta.expected_checksum,
                fresh_restart: meta.fresh_restart,
                retry_count: meta.retry_count,
                max_retries: meta.max_retries,
                retry_delay_ms: 0,
                depends_on: Vec::new(),
                custom_segments: None,
                group: None,
            };
            let mut queue = DOWNLOAD_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
            queue.requeue_failed(retry_item)
        };

        if requeued {
            eprintln!("[AutoRetry] Re-queued {} for retry (attempt {})", id, meta.retry_count + 1);
            let _ = app.emit("download_retry", serde_json::json!({
                "id": id,
                "attempt": meta.retry_count + 1,
                "max_retries": meta.max_retries,
            }));
            queue_manager::persist_queue();
            return true;
        }
    }
    false
}

pub(crate) fn mark_retry_for_fresh_restart(id: &str) {
    let mut store = queue_manager::RETRY_METADATA.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(meta) = store.get_mut(id) {
        meta.fresh_restart = true;
    }
}

pub(crate) fn queued_retry_requires_fresh_restart(id: &str) -> bool {
    let store = queue_manager::RETRY_METADATA.lock().unwrap_or_else(|e| e.into_inner());
    store.get(id).map(|m| m.fresh_restart).unwrap_or(false)
}

pub(crate) fn clear_retry_metadata(id: &str) {
    let mut store = queue_manager::RETRY_METADATA.lock().unwrap_or_else(|e| e.into_inner());
    store.remove(id);
}

pub(crate) fn get_expected_checksum(id: &str) -> Option<String> {
    let store = queue_manager::RETRY_METADATA.lock().unwrap_or_else(|e| e.into_inner());
    store.get(id).and_then(|m| m.expected_checksum.clone())
}

/// Verify queue-supplied checksum before finalizing success.
/// Emits `integrity_check_passed` or `integrity_check_failed` events.
pub(crate) async fn verify_queued_integrity(app: &tauri::AppHandle, id: &str, path: &str) -> Result<(), String> {
    let expected = get_expected_checksum(id);

    if let Some(expected_checksum) = expected {
        match crate::integrity::verify_file_checksum(path, &expected_checksum).await {
            Ok(_) => {
                let _ = app.emit("integrity_check_passed", serde_json::json!({
                    "id": id,
                }));
            }
            Err(e) => {
                eprintln!("[Queue] Integrity check failed for {}: {}", id, e);
                let _ = app.emit("integrity_check_failed", serde_json::json!({
                    "id": id,
                    "error": e.clone(),
                }));
                return Err(e);
            }
        }
    }

    Ok(())
}

fn corrupt_retry_path(path: &str) -> String {
    let source = std::path::Path::new(path);
    let parent = source.parent().unwrap_or(std::path::Path::new("."));
    let file_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("download");
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    parent
        .join(format!("{}.corrupt-{}", file_name, ts))
        .to_string_lossy()
        .to_string()
}

pub(crate) fn quarantine_corrupt_file(path: &str) -> Result<(), String> {
    let source = std::path::Path::new(path);
    if !source.exists() {
        return Ok(());
    }

    let quarantined = corrupt_retry_path(path);
    match std::fs::rename(source, &quarantined) {
        Ok(_) => {
            eprintln!("[Integrity] Quarantined corrupt file: {} -> {}", path, quarantined);
            Ok(())
        }
        Err(rename_err) => {
            eprintln!(
                "[Integrity] Failed to quarantine {} ({}), removing it for clean retry",
                path,
                rename_err
            );
            std::fs::remove_file(source).map_err(|remove_err| {
                format!(
                    "Failed to reset corrupt file at {}: rename failed ({}), remove failed ({})",
                    path,
                    rename_err,
                    remove_err
                )
            })
        }
    }
}

fn record_integrity_failure(
    app: &tauri::AppHandle,
    id: &str,
    url: &str,
    path: &str,
    total_size: u64,
    started_at: &str,
    elapsed: std::time::Duration,
    segments_used: u32,
    error_message: &str,
) {
    let expected_checksum = get_expected_checksum(id);
    let _ = app.emit("download_error", serde_json::json!({
        "id": id,
        "error": error_message,
    }));
    crate::media::sounds::play_error();

    let _ = persistence::upsert_download(persistence::SavedDownload {
        id: id.to_string(),
        url: url.to_string(),
        path: path.to_string(),
        filename: extract_filename(path).to_string(),
        total_size,
        downloaded_bytes: 0,
        status: "Error".to_string(),
        segments: None,
        last_active: Some(chrono::Utc::now().to_rfc3339()),
        error_message: Some(error_message.to_string()),
        expected_checksum,
    });

    let avg_speed = if elapsed.as_secs() > 0 {
        total_size / elapsed.as_secs()
    } else {
        0
    };

    let _ = crate::download_history::record(crate::download_history::HistoryEntry {
        id: id.to_string(),
        url: url.to_string(),
        path: path.to_string(),
        filename: extract_filename(path).to_string(),
        total_size,
        downloaded_bytes: total_size,
        status: "Error".to_string(),
        started_at: started_at.to_string(),
        finished_at: chrono::Local::now().to_rfc3339(),
        avg_speed_bps: avg_speed,
        duration_secs: elapsed.as_secs(),
        segments_used,
        error_message: Some(error_message.to_string()),
        source_type: Some("http".to_string()),
    });

    if let Some(log) = app.try_state::<std::sync::Arc<crate::event_sourcing::SharedLog>>() {
        let _ = log.append(crate::event_sourcing::LedgerEvent {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            aggregate_id: id.to_string(),
            event_type: "DownloadError".to_string(),
            payload: serde_json::json!({
                "error": error_message,
                "downloaded_bytes": total_size,
                "total_size": total_size,
                "integrity_failure": true,
            }),
        });
    }
}

fn finalize_http_success_side_effects(
    app: &tauri::AppHandle,
    id: String,
    url: String,
    path: String,
    total_size: u64,
    started_at: String,
    elapsed: std::time::Duration,
    segments_used: u32,
    md5: Option<String>,
    etag: Option<String>,
    chatops: std::sync::Arc<crate::network::chatops::ChatOpsManager>,
) {
    let expected_checksum = get_expected_checksum(&id);
    clear_retry_metadata(&id);
    crate::media::sounds::play_complete();
    crate::cas_manager::register_cas(etag.as_deref(), md5.as_deref(), &path);

    if let Some(log) = app.try_state::<std::sync::Arc<crate::event_sourcing::SharedLog>>() {
        let _ = log.append(crate::event_sourcing::LedgerEvent {
            timestamp: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
            aggregate_id: id.clone(),
            event_type: "DownloadCompleted".to_string(),
            payload: serde_json::json!({
                "total_size": total_size,
                "duration_secs": elapsed.as_secs(),
                "path": path,
            }),
        });
    }

    let id_webhook = id.clone();
    let url_webhook = url.clone();
    let path_webhook = path.clone();
    tokio::spawn(async move {
        let settings = settings::load_settings();
        if let Some(webhooks) = settings.webhooks {
            let manager = webhooks::WebhookManager::new();
            manager.load_configs(webhooks).await;
            let payload = webhooks::WebhookPayload {
                event: "DownloadComplete".to_string(),
                download_id: id_webhook,
                filename: extract_filename(&path_webhook).to_string(),
                url: url_webhook,
                size: total_size,
                speed: 0,
                filepath: Some(path_webhook),
                timestamp: chrono::Utc::now().timestamp(),
            };
            manager.trigger(webhooks::WebhookEvent::DownloadComplete, payload).await;
        }
    });

    crate::mqtt_client::publish_event(
        "DownloadComplete",
        &id,
        extract_filename(&path),
        "Complete",
    );

    let filename_chatops = extract_filename(&path).to_string();
    tokio::spawn(async move {
        chatops.notify_completion(&filename_chatops).await;
    });

    let path_archive = path.clone();
    tokio::spawn(async move {
        let settings = settings::load_settings();
        if settings.auto_extract_archives {
            if let Some(archive_info) = archive_manager::ArchiveManager::detect_archive(&path_archive) {
                println!("📦 Detected archive: {:?}", archive_info.archive_type);

                let dest = std::path::Path::new(&path_archive)
                    .parent()
                    .and_then(|p| p.to_str())
                    .unwrap_or(".")
                    .to_string();

                match archive_manager::ArchiveManager::extract_archive(&path_archive, &dest) {
                    Ok(msg) => {
                        println!("✅ {}", msg);

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

    let _ = persistence::upsert_download(persistence::SavedDownload {
        id: id.clone(),
        url: url.clone(),
        path: path.clone(),
        filename: extract_filename(&path).to_string(),
        total_size,
        downloaded_bytes: total_size,
        status: "Complete".to_string(),
        segments: None,
        last_active: Some(chrono::Utc::now().to_rfc3339()),
        error_message: None,
        expected_checksum,
    });

    let avg_speed = if elapsed.as_secs() > 0 {
        total_size / elapsed.as_secs()
    } else {
        0
    };
    let _ = crate::download_history::record(crate::download_history::HistoryEntry {
        id: id.clone(),
        url,
        path: path.clone(),
        filename: extract_filename(&path).to_string(),
        total_size,
        downloaded_bytes: total_size,
        status: "Complete".to_string(),
        started_at,
        finished_at: chrono::Local::now().to_rfc3339(),
        avg_speed_bps: avg_speed,
        duration_secs: elapsed.as_secs(),
        segments_used,
        error_message: None,
        source_type: Some("http".to_string()),
    });

    {
        let verify_id = id.clone();
        let verify_path = path.clone();
        let verify_md5 = md5.clone();
        let verify_app = app.clone();
        tokio::spawn(async move {
            if let Some(result) = crate::integrity::auto_verify(
                &verify_id,
                &verify_path,
                verify_md5.as_deref(),
            ).await {
                let _ = verify_app.emit("integrity_check", serde_json::json!({
                    "id": verify_id,
                    "verified": result.verified,
                    "method": result.method,
                    "algorithm": result.algorithm,
                    "message": result.message,
                }));
            }
        });
    }

    {
        let settings_snap = crate::settings::load_settings();
        if settings_snap.auto_sort_downloads {
            match crate::file_categorizer::categorize_and_move(&path, &settings_snap.download_dir) {
                Ok((cat_result, new_path)) => {
                    let moved = new_path != path;
                    let _ = app.emit("file_categorized", serde_json::json!({
                        "id": id,
                        "filename": extract_filename(&path),
                        "category": cat_result.category_name,
                        "icon": cat_result.icon,
                        "color": cat_result.color,
                        "should_move": cat_result.should_move,
                        "target_dir": cat_result.target_dir,
                        "moved": moved,
                        "new_path": if moved { Some(&new_path) } else { None },
                    }));
                }
                Err(e) => {
                    eprintln!("[{}] Auto-sort failed: {}", id, e);
                    let cat_result = crate::file_categorizer::categorize(extract_filename(&path));
                    let _ = app.emit("file_categorized", serde_json::json!({
                        "id": id,
                        "filename": extract_filename(&path),
                        "category": cat_result.category_name,
                        "icon": cat_result.icon,
                        "color": cat_result.color,
                        "should_move": false,
                        "target_dir": cat_result.target_dir,
                    }));
                }
            }
        } else {
            let cat_result = crate::file_categorizer::categorize(extract_filename(&path));
            let _ = app.emit("file_categorized", serde_json::json!({
                "id": id,
                "filename": extract_filename(&path),
                "category": cat_result.category_name,
                "icon": cat_result.icon,
                "color": cat_result.color,
                "should_move": false,
                "target_dir": cat_result.target_dir,
            }));
        }
    }

    if crate::settings::load_settings().scan_after_download {
        let scan_path = path.clone();
        let scan_id = id.clone();
        let scan_app = app.clone();
        tokio::spawn(async move {
            let scanner = crate::virus_scanner::VirusScanner::new();
            if scanner.is_available() {
                let result = scanner.scan_file(std::path::Path::new(&scan_path)).await;
                let (status, threat) = match &result {
                    crate::virus_scanner::ScanResult::Clean => ("clean", None),
                    crate::virus_scanner::ScanResult::Infected { threat_name } => ("infected", Some(threat_name.as_str())),
                    crate::virus_scanner::ScanResult::Error { message } => ("error", Some(message.as_str())),
                    crate::virus_scanner::ScanResult::NotScanned => ("not_scanned", None),
                };
                let _ = scan_app.emit("virus_scan_result", serde_json::json!({
                    "id": scan_id,
                    "status": status,
                    "threat": threat,
                }));
            }
        });
    }
}

/// Extract filename from a path string, handling both Unix and Windows separators.
/// Falls back to the full path string if no filename component can be extracted.
pub(crate) fn extract_filename(path: &str) -> &str {
    std::path::Path::new(path)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or(path)
}

/// Resolve filename collisions by appending `(1)`, `(2)`, etc. to the stem.
///
/// Given `/downloads/video.mp4`:
///   - If `video.mp4` exists → tries `video(1).mp4`, `video(2).mp4`, …
///   - Returns the first path that doesn't exist (capped at 9999 attempts).
///   - If the file doesn't exist at all, returns the original path unchanged.
///
/// This mimics IDM's smart rename behaviour and prevents silent overwrites.
pub(crate) fn resolve_filename_collision(path: &str) -> String {
    let p = std::path::Path::new(path);
    if !p.exists() {
        return path.to_string();
    }

    let parent = p.parent().unwrap_or(std::path::Path::new("."));
    let stem = p
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("download");
    let ext = p.extension().and_then(|e| e.to_str());

    for i in 1..=9999u32 {
        let candidate = match ext {
            Some(e) => parent.join(format!("{}({}).{}", stem, i, e)),
            None => parent.join(format!("{}({})", stem, i)),
        };
        if !candidate.exists() {
            return candidate.to_string_lossy().to_string();
        }
    }

    // Extremely unlikely fallback — use timestamp
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let fallback = match ext {
        Some(e) => parent.join(format!("{}_{}.{}", stem, ts, e)),
        None => parent.join(format!("{}_{}", stem, ts)),
    };
    fallback.to_string_lossy().to_string()
}

pub(crate) async fn start_download_impl(
    app: &tauri::AppHandle,
    state: &AppState,
    id: String, 
    url: String, 
    path: String,
    _resume_override: Option<u64>,
    custom_headers: Option<std::collections::HashMap<String, String>>,
    force: bool,
    group_id: Option<String>,
) -> Result<(), String> {
    println!("DEBUG: Starting download ID: {}", id);
    
    // Play start sound
    crate::media::sounds::play_startup();
    
    // Load settings once for the entire download initialization
    let settings = settings::load_settings();
    let segment_retry_config = crate::downloader::network::retry_config_from(
        settings.segment_retry_max_immediate,
        settings.segment_retry_max_delayed,
        settings.segment_retry_initial_delay_secs as u64,
        settings.segment_retry_max_delay_secs as u64,
        settings.segment_retry_jitter,
    );
    
    // INTEGRATION POINT 1: Check group dependencies before proceeding
    let member_id = if let Some(ref gid) = group_id {
        let scheduler = GLOBAL_GROUP_SCHEDULER.lock()
            .map_err(|e| format!("Failed to lock group scheduler: {}", e))?;
        
        if !scheduler.has_group(gid) {
            return Err(format!("Group {} not found", gid));
        }
        
        if let Some(group) = scheduler.get_group(gid) {
            // Try to find a pending member with satisfied dependencies
            let ready_members = scheduler.get_ready_members(gid);
            if ready_members.is_empty() {
                return Err(format!("No ready members in group {} (all have unsatisfied dependencies or are already started)", gid));
            }
            
            // Use the first ready member
            ready_members[0].clone()
        } else {
            return Err(format!("Group {} not found", gid));
        }
    } else {
        String::new()
    };
    
    let group_context = if let Some(ref gid) = group_id {
        Some((gid.clone(), member_id.clone()))
    } else {
        None
    };
    
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

    let fresh_restart = queued_retry_requires_fresh_restart(&id);
    if fresh_restart {
        quarantine_corrupt_file(&path)?;
    }

    // 1. Check for saved download (Resume logic)
    let saved_downloads = persistence::load_downloads().unwrap_or_default();
    let saved = if fresh_restart {
        None
    } else {
        saved_downloads.iter().find(|d| d.id == id)
    };
    let resume_from: u64 = if fresh_restart {
        0
    } else {
        saved.map(|s| s.downloaded_bytes).unwrap_or(0)
    };
    
    if resume_from > 0 {
        println!("DEBUG: Resuming from byte {}", resume_from);
    }

    // AUTO-SORT / CATEGORY LOGIC
    // We only change path if it's a new download (not resuming) OR if we force checks (safer to only do new)
    // But `resume_from > 0` implies file exists at `path`. If we change `path` on resume, it breaks.
    // So ONLY apply category rules if strict resume_from == 0 OR we check if file exists at old path.
    // Simplest: only apply on start (resume_from == 0).
    
    let final_path = if resume_from == 0 && settings.use_category_folders && !fresh_restart {
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
    
    // Use final_path for the rest — apply collision avoidance for new downloads
    let path = if resume_from == 0 && !fresh_restart {
        resolve_filename_collision(&final_path)
    } else {
        final_path
    };

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
    let probe = downloader::initialization::determine_total_size(&client, &actual_url).await?;
    let (mut total_size, mut etag, mut md5, mut supports_range) = (probe.total_size, probe.etag, probe.md5, probe.supports_range);

    // Git LFS Accelerator check
    if total_size > 0 && total_size < 1024 * 5 {
        if let Ok(res) = client.get(&actual_url).send().await {
            if let Ok(text) = res.text().await {
                if let Some(new_url) = crate::git_lfs::resolve_lfs_pointer(&actual_url, &text).await {
                    println!("DEBUG: Git LFS pointer detected! Swapping to real binaries via Batch API.");
                    actual_url = new_url;
                    // Re-determine size
                    let sz_res = downloader::initialization::determine_total_size(&client, &actual_url).await;
                    if let Ok(probe2) = sz_res {
                        total_size = probe2.total_size;
                        etag = probe2.etag;
                        md5 = probe2.md5;
                        supports_range = probe2.supports_range;
                    }
                }
            }
        }
    }

    // Check CAS Deduplication (skip when force-download is requested)
    if !force {
    if let Some(existing_path) = crate::cas_manager::check_cas(etag.as_deref(), md5.as_deref()) {
        println!("CAS Match Found! Hardlinking from {}", existing_path);
        // Attempt to hardlink
        if std::fs::hard_link(&existing_path, &path).is_ok() {
            println!("Hardlink successful for {}", path);

            if let Err(integrity_error) = verify_queued_integrity(app, &id, &path).await {
                let started_at = chrono::Local::now().to_rfc3339();
                mark_retry_for_fresh_restart(&id);
                record_integrity_failure(
                    app,
                    &id,
                    &url,
                    &path,
                    total_size,
                    &started_at,
                    std::time::Duration::ZERO,
                    0,
                    &integrity_error,
                );
                handle_download_failure_cleanup(app, &id);
                return Ok(());
            }

            // Register success... emit completion... and return
            let payload = Payload {
                id: id.clone(),
                downloaded: total_size,
                total: total_size,
                speed_bps: 0,
                segments: vec![],
            };
            let _ = app.emit("download_progress", payload.clone());
            let _ = crate::http_server::get_event_sender().send(serde_json::to_value(&payload).unwrap_or(serde_json::json!(null)));

            finalize_http_success_side_effects(
                app,
                id.clone(),
                actual_url.clone(),
                path.clone(),
                total_size,
                chrono::Local::now().to_rfc3339(),
                std::time::Duration::ZERO,
                0,
                md5.clone(),
                etag.clone(),
                state.chatops_manager.clone(),
            );

            {
                let mut queue = crate::queue_manager::DOWNLOAD_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
                queue.mark_finished_success(&id);
            }

            if let Some(action) = crate::scheduler::handle_download_complete(&id) {
                crate::scheduler::execute_end_action(app, &action);
            }

            return Ok(());
        } else {
            println!("Failed to create hardlink, falling back to download");
        }
    }
    } // end !force CAS block
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
    // Force single segment if server doesn't support Range requests
    let per_download_segments = {
        let mut overrides = crate::queue_manager::DOWNLOAD_OVERRIDES.lock().unwrap_or_else(|e| e.into_inner());
        overrides.remove(&id).and_then(|o| o.custom_segments)
    };
    let effective_segments = if !supports_range {
        println!("[download] Server does not support Range — falling back to single segment for {}", actual_url);
        1
    } else if let Some(custom) = per_download_segments {
        println!("[download] Using per-download segment override: {} for {}", custom, id);
        custom.clamp(1, 64)
    } else {
        settings.segments
    };
    let manager = downloader::initialization::setup_manager(total_size, saved, resume_from, effective_segments);
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
            group_context: group_context.clone(),
        });
    }

    // Log download started event
    if let Some(log) = app.try_state::<std::sync::Arc<crate::event_sourcing::SharedLog>>() {
        let _ = log.append(crate::event_sourcing::LedgerEvent {
            timestamp: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
            aggregate_id: id.clone(),
            event_type: "DownloadStarted".to_string(),
            payload: serde_json::json!({
                "url": url,
                "path": path,
                "total_size": total_size,
                "segments": settings.segments,
                "resume_from": resume_from,
            }),
        });
    }

    // 7. Disk Writer
    let (tx, rx) = mpsc::channel::<WriteRequest>();
    let file_writer_clone = file_mutex.clone();
    let disk_io_error = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let disk_io_error_writer = disk_io_error.clone();
    thread::spawn(move || {
        let mut writer = DiskWriter::new(file_writer_clone, rx);
        // Share the writer's actual I/O error flag with the monitor so it can
        // detect disk failures in real-time (not just after writer.run() returns).
        // We copy the writer's flag reference into the shared outer flag location
        // by polling it periodically from a background thread.
        let writer_flag = writer.io_error_flag();
        let error_bridge = disk_io_error_writer.clone();
        let bridge_flag = writer_flag.clone();
        std::thread::spawn(move || {
            // Poll the writer's flag every 100ms and propagate to the shared flag
            while !error_bridge.load(std::sync::atomic::Ordering::Relaxed) {
                if bridge_flag.load(std::sync::atomic::Ordering::Relaxed) {
                    error_bridge.store(true, std::sync::atomic::Ordering::Release);
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        });
        writer.run();
        // Final propagation after writer exits
        if writer_flag.load(std::sync::atomic::Ordering::Acquire) {
            disk_io_error_writer.store(true, std::sync::atomic::Ordering::Release);
        }
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
    let group_context_monitor = group_context.clone();
    let mut stop_rx_monitor = stop_tx.subscribe();
    let stop_tx_monitor = stop_tx.clone();
    let chatops_monitor = state.chatops_manager.clone();
    let disk_io_error_monitor = disk_io_error.clone();
    // Additional clones for dynamic worker spawning
    let tx_monitor = tx.clone();
    let client_monitor = client.clone();
    let cm_monitor = state.connection_manager.clone();
    let adaptive_splitting_enabled = settings.adaptive_splitting && supports_range;
    let max_dynamic_threads = settings.max_threads.max(settings.segments);
    let dynamic_retry_config = segment_retry_config.clone();
    let stall_timeout = std::time::Duration::from_secs(settings.stall_timeout_secs.max(1).min(86400) as u64);
    // Configure PID controller from settings
    if settings.min_threads > 0 && settings.max_threads > 0 {
        crate::adaptive_threads::THREAD_CONTROLLER.configure(settings.min_threads, settings.max_threads);
    }
    
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(33)); // ~30fps
        let monitor_start = std::time::Instant::now();
        let monitor_start_iso = chrono::Local::now().to_rfc3339();
        // Stall & failure detection state
        let mut last_progress_bytes: u64 = 0;
        let mut stall_since: Option<std::time::Instant> = None;
        let mut last_pid_update = std::time::Instant::now();
        let mut last_split_check = std::time::Instant::now();
        // Per-segment speed tracking: segment_id -> (last_cursor, last_time, ema_speed)
        let mut seg_speed_state: std::collections::HashMap<u32, (u64, std::time::Instant, f64)> = std::collections::HashMap::new();
        const SPEED_EMA_ALPHA: f64 = 0.3; // Smoothing factor for per-segment EMA
        loop {
            tokio::select! {
                _ = stop_rx_monitor.recv() => break,
                _ = interval.tick() => {
                    // Check for disk I/O errors — abort download if disk writer failed
                    if disk_io_error_monitor.load(std::sync::atomic::Ordering::Acquire) {
                        eprintln!("[{}] Disk I/O error detected, aborting download", id_monitor);
                        let _ = window_monitor.emit("download_error", serde_json::json!({
                            "id": id_monitor,
                            "error": "Disk write error — the download has been stopped to prevent data corruption."
                        }));
                        let _ = stop_tx_monitor.send(());
                        
                        // INTEGRATION POINT 4b: Mark group member as failed on disk error
                        if let Some((gid, mid)) = &group_context_monitor {
                            if let Ok(mut scheduler) = GLOBAL_GROUP_SCHEDULER.lock() {
                                if let Err(e) = scheduler.fail_member(gid, mid, "Disk write error") {
                                    eprintln!("[{}] Failed to mark member {} failed in group {}: {}", 
                                              id_monitor, mid, gid, e);
                                }
                            }
                        }
                        // Record in download history
                        let elapsed = monitor_start.elapsed();
                        let _ = crate::download_history::record(crate::download_history::HistoryEntry {
                            id: id_monitor.clone(),
                            url: url_monitor.clone(),
                            path: path_monitor.clone(),
                            filename: extract_filename(&path_monitor).to_string(),
                            total_size,
                            downloaded_bytes: downloaded_monitor.load(Ordering::Relaxed),
                            status: "Error".to_string(),
                            started_at: monitor_start_iso.clone(),
                            finished_at: chrono::Local::now().to_rfc3339(),
                            avg_speed_bps: 0,
                            duration_secs: elapsed.as_secs(),
                            segments_used: 0,
                            error_message: Some("Disk write error".to_string()),
                            source_type: Some("http".to_string()),
                        });
                        // Log event sourcing
                        if let Some(log) = window_monitor.try_state::<std::sync::Arc<crate::event_sourcing::SharedLog>>() {
                            let _ = log.append(crate::event_sourcing::LedgerEvent {
                                timestamp: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
                                aggregate_id: id_monitor.clone(),
                                event_type: "DownloadError".to_string(),
                                payload: serde_json::json!({
                                    "error": "Disk write error",
                                    "downloaded_bytes": downloaded_monitor.load(Ordering::Relaxed),
                                    "total_size": total_size,
                                }),
                            });
                        }
                        handle_download_failure_cleanup(&window_monitor, &id_monitor);
                        break;
                    }

                    let d = downloaded_monitor.load(Ordering::Relaxed);
                    
                    // Get segment snapshot for visualization
                    // We only lock here, once per 33ms, instead of per-chunk
                    // Note: get_segments_snapshot internally locks.
                    let mut segments = manager_monitor.lock().unwrap_or_else(|e| e.into_inner()).get_segments_snapshot();

                    // Compute per-segment speed using EMA over cursor deltas
                    let now_speed = std::time::Instant::now();
                    for seg in segments.iter_mut() {
                        let cursor = seg.downloaded_cursor;
                        if let Some((last_cursor, last_time, ema)) = seg_speed_state.get_mut(&seg.id) {
                            let dt = now_speed.duration_since(*last_time).as_secs_f64();
                            if dt > 0.01 {
                                let bytes_delta = cursor.saturating_sub(*last_cursor);
                                let instant_speed = bytes_delta as f64 / dt;
                                // EMA: smoothed = α * new + (1-α) * old
                                *ema = SPEED_EMA_ALPHA * instant_speed + (1.0 - SPEED_EMA_ALPHA) * *ema;
                                seg.speed_bps = *ema as u64;
                                *last_cursor = cursor;
                                *last_time = now_speed;
                            } else {
                                seg.speed_bps = *ema as u64;
                            }
                        } else {
                            seg_speed_state.insert(seg.id, (cursor, now_speed, 0.0));
                            seg.speed_bps = 0;
                        }
                        // Zero speed for non-downloading segments
                        if seg.state != crate::downloader::structures::SegmentState::Downloading {
                            seg.speed_bps = 0;
                            if let Some((_, _, ema)) = seg_speed_state.get_mut(&seg.id) {
                                *ema = 0.0;
                            }
                        }
                    }
                    
                    // --- Failure & stall detection ---
                    use crate::downloader::structures::SegmentState;
                    if total_size > 0 && d < total_size && !segments.is_empty() {
                        let any_active = segments.iter().any(|s|
                            s.state == SegmentState::Downloading || s.state == SegmentState::Idle
                        );

                        // All segments settled (Complete or Error) but download isn't finished
                        if !any_active {
                            let error_count = segments.iter().filter(|s| s.state == SegmentState::Error).count();
                            let error_msg = format!(
                                "Download failed: {}/{} segments errored, {} of {} bytes downloaded",
                                error_count, segments.len(), d, total_size
                            );
                            eprintln!("[{}] {}", id_monitor, error_msg);
                            let _ = window_monitor.emit("download_error", serde_json::json!({
                                "id": id_monitor,
                                "error": error_msg,
                            }));
                            crate::media::sounds::play_error();
                            let _ = stop_tx_monitor.send(());

                            // INTEGRATION POINT 4: Mark group member as failed
                            if let Some((gid, mid)) = &group_context_monitor {
                                if let Ok(mut scheduler) = GLOBAL_GROUP_SCHEDULER.lock() {
                                    if let Err(e) = scheduler.fail_member(gid, mid, &error_msg) {
                                        eprintln!("[{}] Failed to mark member {} failed in group {}: {}", 
                                                  id_monitor, mid, gid, e);
                                    }
                                }
                            }

                            // Persist as Error with segment state for future resume
                            let segs_snap = segments.clone();
                            let _ = persistence::upsert_download(persistence::SavedDownload {
                                id: id_monitor.clone(),
                                url: url_monitor.clone(),
                                path: path_monitor.clone(),
                                filename: extract_filename(&path_monitor).to_string(),
                                total_size,
                                downloaded_bytes: d,
                                status: "Error".to_string(),
                                segments: Some(segs_snap),
                                last_active: Some(chrono::Utc::now().to_rfc3339()),
                                error_message: Some(error_msg.clone()),
                                expected_checksum: get_expected_checksum(&id_monitor),
                            });
                            let elapsed = monitor_start.elapsed();
                            let _ = crate::download_history::record(crate::download_history::HistoryEntry {
                                id: id_monitor.clone(),
                                url: url_monitor.clone(),
                                path: path_monitor.clone(),
                                filename: extract_filename(&path_monitor).to_string(),
                                total_size,
                                downloaded_bytes: d,
                                status: "Error".to_string(),
                                started_at: monitor_start_iso.clone(),
                                finished_at: chrono::Local::now().to_rfc3339(),
                                avg_speed_bps: if elapsed.as_secs() > 0 { d / elapsed.as_secs() } else { 0 },
                                duration_secs: elapsed.as_secs(),
                                segments_used: segments.len() as u32,
                                error_message: Some(error_msg.clone()),
                                source_type: Some("http".to_string()),
                            });
                            // Log event sourcing
                            if let Some(log) = window_monitor.try_state::<std::sync::Arc<crate::event_sourcing::SharedLog>>() {
                                let _ = log.append(crate::event_sourcing::LedgerEvent {
                                    timestamp: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
                                    aggregate_id: id_monitor.clone(),
                                    event_type: "DownloadError".to_string(),
                                    payload: serde_json::json!({
                                        "error": error_msg,
                                        "downloaded_bytes": d,
                                        "total_size": total_size,
                                        "error_segments": error_count,
                                        "total_segments": segments.len(),
                                    }),
                                });
                            }
                            handle_download_failure_cleanup(&window_monitor, &id_monitor);
                            break;
                        }

                        // Stall detection: no progress for stall_timeout
                        if d > last_progress_bytes {
                            last_progress_bytes = d;
                            stall_since = None;
                        } else {
                            if stall_since.is_none() {
                                stall_since = Some(std::time::Instant::now());
                            }
                            if let Some(since) = stall_since {
                                if since.elapsed() > stall_timeout {
                                    let error_msg = format!(
                                        "Download stalled: no progress for {}s ({} of {} bytes)",
                                        stall_timeout.as_secs(), d, total_size
                                    );
                                    eprintln!("[{}] {}", id_monitor, error_msg);
                                    let _ = window_monitor.emit("download_error", serde_json::json!({
                                        "id": id_monitor,
                                        "error": error_msg,
                                    }));
                                    crate::media::sounds::play_error();
                                    let _ = stop_tx_monitor.send(());

                                    let segs_snap = segments.clone();
                                    let _ = persistence::upsert_download(persistence::SavedDownload {
                                        id: id_monitor.clone(),
                                        url: url_monitor.clone(),
                                        path: path_monitor.clone(),
                                        filename: extract_filename(&path_monitor).to_string(),
                                        total_size,
                                        downloaded_bytes: d,
                                        status: "Error".to_string(),
                                        segments: Some(segs_snap),
                                        last_active: Some(chrono::Utc::now().to_rfc3339()),
                                        error_message: Some(error_msg.clone()),
                                        expected_checksum: get_expected_checksum(&id_monitor),
                                    });
                                    let elapsed = monitor_start.elapsed();
                                    let _ = crate::download_history::record(crate::download_history::HistoryEntry {
                                        id: id_monitor.clone(),
                                        url: url_monitor.clone(),
                                        path: path_monitor.clone(),
                                        filename: extract_filename(&path_monitor).to_string(),
                                        total_size,
                                        downloaded_bytes: d,
                                        status: "Error".to_string(),
                                        started_at: monitor_start_iso.clone(),
                                        finished_at: chrono::Local::now().to_rfc3339(),
                                        avg_speed_bps: if elapsed.as_secs() > 0 { d / elapsed.as_secs() } else { 0 },
                                        duration_secs: elapsed.as_secs(),
                                        segments_used: segments.len() as u32,
                                        error_message: Some(error_msg.clone()),
                                        source_type: Some("http".to_string()),
                                    });
                                    // Log event sourcing
                                    if let Some(log) = window_monitor.try_state::<std::sync::Arc<crate::event_sourcing::SharedLog>>() {
                                        let _ = log.append(crate::event_sourcing::LedgerEvent {
                                            timestamp: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
                                            aggregate_id: id_monitor.clone(),
                                            event_type: "DownloadError".to_string(),
                                            payload: serde_json::json!({
                                                "error": error_msg,
                                                "downloaded_bytes": d,
                                                "total_size": total_size,
                                                "stall_timeout_secs": stall_timeout.as_secs(),
                                            }),
                                        });
                                    }
                                    handle_download_failure_cleanup(&window_monitor, &id_monitor);
                                    break;
                                }
                            }
                        }
                    }
                    // --- End failure & stall detection ---
                    
                    // Feed adaptive threads system with bandwidth data
                    {
                        let speed: u64 = segments.iter()
                            .filter(|s| s.state == crate::downloader::structures::SegmentState::Downloading)
                            .map(|s| s.speed_bps)
                            .sum();
                        crate::adaptive_threads::BANDWIDTH_MONITOR.add_sample(speed);
                        // Update PID controller every ~5s
                        if last_pid_update.elapsed() >= std::time::Duration::from_secs(5) && speed > 0 {
                            let max_speed = crate::adaptive_threads::BANDWIDTH_MONITOR.get_average_speed().max(speed) * 2;
                            crate::adaptive_threads::THREAD_CONTROLLER.update(speed, max_speed);
                            last_pid_update = std::time::Instant::now();
                        }
                    }

                    // ── DYNAMIC SEGMENT SPLITTING (IDM-style acceleration) ──
                    // Every 5 seconds, check if the PID controller recommends
                    // more connections than we currently have. If so, split the
                    // slowest segments and spawn new workers.
                    if adaptive_splitting_enabled
                        && total_size > 0
                        && d < total_size
                        && last_split_check.elapsed() >= std::time::Duration::from_secs(5)
                    {
                        last_split_check = std::time::Instant::now();
                        let recommended = crate::adaptive_threads::recommended_threads().max(1);
                        let target = recommended.min(max_dynamic_threads);

                        let splits = {
                            let m = manager_monitor.lock().unwrap_or_else(|e| e.into_inner());
                            m.find_splittable_segments(target)
                        };

                        for work in splits {
                            let seg = work.new_segment;
                            let seg_id = seg.id;
                            let start_pos = seg.start_byte;
                            let end_pos = seg.end_byte;

                            println!(
                                "[DynamicSplit] Spawning worker for segment {} ({}-{}, {} bytes)",
                                seg_id, start_pos, end_pos, end_pos - start_pos
                            );

                            // Clone resources for the new worker
                            let mgr = manager_monitor.clone();
                            let url_w = url_monitor.clone();
                            let tx_w = tx_monitor.clone();
                            let cl_w = client_monitor.clone();
                            let dl_w = downloaded_monitor.clone();
                            let cm_w = cm_monitor.clone();
                            let mut stop_w = stop_tx_monitor.subscribe();
                            let stop_tx_w = stop_tx_monitor.clone();
                            let id_w = id_monitor.clone();
                            // path_w and app_w were unused
                            let dio_w = disk_io_error_monitor.clone();
                            let retry_config = dynamic_retry_config.clone();

                            tokio::spawn(async move {
                                let mut current_pos = start_pos;
                                let end = end_pos;
                                let mut seg_id_dyn = seg_id;
                                let mut retry_state = crate::downloader::network::RetryState::from_config(&retry_config);
                                let mut bytes_since_cursor_update: u64 = 0;
                                const CURSOR_UPDATE_THRESHOLD: u64 = 256 * 1024;
                                let mut end_dyn = end;

                                loop {
                                    if stop_w.try_recv().is_ok() {
                                        let m = mgr.lock().unwrap_or_else(|e| e.into_inner());
                                        let mut segs = m.segments.write().unwrap_or_else(|e| e.into_inner());
                                        if let Some(s) = segs.iter_mut().find(|s| s.id == seg_id_dyn) {
                                            s.downloaded_cursor = current_pos;
                                            s.state = crate::downloader::structures::SegmentState::Paused;
                                        }
                                        break;
                                    }

                                    if dio_w.load(std::sync::atomic::Ordering::Acquire) {
                                        let m = mgr.lock().unwrap_or_else(|e| e.into_inner());
                                        let mut segs = m.segments.write().unwrap_or_else(|e| e.into_inner());
                                        if let Some(s) = segs.iter_mut().find(|s| s.id == seg_id_dyn) {
                                            s.downloaded_cursor = current_pos;
                                            s.state = crate::downloader::structures::SegmentState::Error;
                                        }
                                        break;
                                    }

                                    if current_pos >= end_dyn {
                                        let stolen = {
                                            let m = mgr.lock().unwrap_or_else(|e| e.into_inner());
                                            m.on_segment_complete(seg_id_dyn)
                                        };
                                        if let Some(w) = stolen {
                                            seg_id_dyn = w.new_segment.id;
                                            current_pos = w.new_segment.start_byte;
                                            end_dyn = w.new_segment.end_byte;
                                            retry_state = crate::downloader::network::RetryState::from_config(&retry_config);
                                            bytes_since_cursor_update = 0;
                                            continue;
                                        } else {
                                            break;
                                        }
                                    }

                                    let range_header = format!("bytes={}-{}", current_pos, end_dyn - 1);
                                    let _permit = cm_w.acquire(&url_w).await.ok();

                                    let res = tokio::select! {
                                        _ = stop_w.recv() => {
                                            let m = mgr.lock().unwrap_or_else(|e| e.into_inner());
                                            let mut segs = m.segments.write().unwrap_or_else(|e| e.into_inner());
                                            if let Some(s) = segs.iter_mut().find(|s| s.id == seg_id_dyn) {
                                                s.downloaded_cursor = current_pos;
                                                s.state = crate::downloader::structures::SegmentState::Paused;
                                            }
                                            break;
                                        }
                                        r = cl_w.get(&url_w).header("Range", &range_header).send() => r
                                    };

                                    let response = match res {
                                        Ok(r) => r,
                                        Err(e) => {
                                            let strategy = crate::downloader::network::analyze_error(&e);
                                            retry_state.last_error = Some(format!("{}", e));
                                            match strategy {
                                                crate::downloader::network::RetryStrategy::Fatal(_) => {
                                                    let m = mgr.lock().unwrap_or_else(|e| e.into_inner());
                                                    let mut segs = m.segments.write().unwrap_or_else(|e| e.into_inner());
                                                    if let Some(s) = segs.iter_mut().find(|s| s.id == seg_id_dyn) {
                                                        s.downloaded_cursor = current_pos;
                                                        s.state = crate::downloader::structures::SegmentState::Error;
                                                    }
                                                    break;
                                                }
                                                crate::downloader::network::RetryStrategy::Immediate => {
                                                    retry_state.immediate_attempts += 1;
                                                    if retry_state.immediate_attempts > retry_config.max_immediate_retries {
                                                        let m = mgr.lock().unwrap_or_else(|e| e.into_inner());
                                                        let mut segs = m.segments.write().unwrap_or_else(|e| e.into_inner());
                                                        if let Some(s) = segs.iter_mut().find(|s| s.id == seg_id_dyn) {
                                                            s.downloaded_cursor = current_pos;
                                                            s.state = crate::downloader::structures::SegmentState::Error;
                                                        }
                                                        break;
                                                    }
                                                    continue;
                                                }
                                                crate::downloader::network::RetryStrategy::Delayed(delay) => {
                                                    retry_state.delayed_attempts += 1;
                                                    if retry_state.delayed_attempts > retry_config.max_delayed_retries {
                                                        let m = mgr.lock().unwrap_or_else(|e| e.into_inner());
                                                        let mut segs = m.segments.write().unwrap_or_else(|e| e.into_inner());
                                                        if let Some(s) = segs.iter_mut().find(|s| s.id == seg_id_dyn) {
                                                            s.downloaded_cursor = current_pos;
                                                            s.state = crate::downloader::structures::SegmentState::Error;
                                                        }
                                                        break;
                                                    }
                                                    let backoff = crate::downloader::network::calculate_backoff(
                                                        delay.max(retry_state.current_delay), &retry_config,
                                                    );
                                                    retry_state.current_delay = backoff;
                                                    tokio::time::sleep(delay).await;
                                                    continue;
                                                }
                                                crate::downloader::network::RetryStrategy::RefreshLink => {
                                                    continue;
                                                }
                                            }
                                        }
                                    };

                                    // Handle 403/410 and rate limiting for dynamic workers
                                    if response.status() == rquest::StatusCode::FORBIDDEN || response.status() == rquest::StatusCode::GONE {
                                        let _ = stop_tx_w.send(());
                                        return;
                                    }
                                    if response.status() == rquest::StatusCode::TOO_MANY_REQUESTS || response.status() == rquest::StatusCode::SERVICE_UNAVAILABLE {
                                        retry_state.delayed_attempts += 1;
                                        if retry_state.delayed_attempts > retry_config.max_delayed_retries {
                                            let m = mgr.lock().unwrap_or_else(|e| e.into_inner());
                                            let mut segs = m.segments.write().unwrap_or_else(|e| e.into_inner());
                                            if let Some(s) = segs.iter_mut().find(|s| s.id == seg_id_dyn) {
                                                s.downloaded_cursor = current_pos;
                                                s.state = crate::downloader::structures::SegmentState::Error;
                                            }
                                            break;
                                        }
                                        let wait = if let Some(h) = response.headers().get("Retry-After") {
                                            if let Ok(s) = h.to_str() {
                                                crate::downloader::network::parse_retry_after(s)
                                                    .unwrap_or(std::time::Duration::from_secs(30))
                                            } else {
                                                std::time::Duration::from_secs(30)
                                            }
                                        } else {
                                            crate::downloader::network::calculate_backoff(retry_state.current_delay, &retry_config)
                                        };
                                        retry_state.current_delay = wait;
                                        tokio::time::sleep(wait).await;
                                        continue;
                                    }

                                    retry_state.reset_with_delay(retry_config.initial_delay);

                                    let mut stream = response.bytes_stream();
                                    loop {
                                        tokio::select! {
                                            _ = stop_w.recv() => {
                                                let m = mgr.lock().unwrap_or_else(|e| e.into_inner());
                                                let mut segs = m.segments.write().unwrap_or_else(|e| e.into_inner());
                                                if let Some(s) = segs.iter_mut().find(|s| s.id == seg_id_dyn) {
                                                    s.downloaded_cursor = current_pos;
                                                    s.state = crate::downloader::structures::SegmentState::Paused;
                                                }
                                                return;
                                            }
                                            item = stream.next() => {
                                                match item {
                                                    Some(Ok(chunk)) => {
                                                        let remaining = end_dyn.saturating_sub(current_pos) as usize;
                                                        let safe_chunk = if chunk.len() > remaining {
                                                            &chunk[..remaining]
                                                        } else {
                                                            &chunk[..]
                                                        };
                                                        let len = safe_chunk.len() as u64;
                                                        if len == 0 { break; }

                                                        crate::speed_limiter::GLOBAL_LIMITER.acquire(len).await;
                                                        crate::bandwidth_allocator::ALLOCATOR.acquire(&id_w, len).await;

                                                        if tx_w.send(WriteRequest { offset: current_pos, data: safe_chunk.to_vec(), segment_id: seg_id_dyn }).is_err() {
                                                            return;
                                                        }
                                                        current_pos += len;
                                                        dl_w.fetch_add(len, Ordering::Relaxed);

                                                        bytes_since_cursor_update += len;
                                                        if bytes_since_cursor_update >= CURSOR_UPDATE_THRESHOLD {
                                                            bytes_since_cursor_update = 0;
                                                            let m = mgr.lock().unwrap_or_else(|e| e.into_inner());
                                                            let mut segs = m.segments.write().unwrap_or_else(|e| e.into_inner());
                                                            if let Some(s) = segs.iter_mut().find(|s| s.id == seg_id_dyn) {
                                                                s.downloaded_cursor = current_pos;
                                                            }
                                                        }
                                                    }
                                                    Some(Err(_stream_err)) => {
                                                        retry_state.delayed_attempts += 1;
                                                        if retry_state.delayed_attempts > retry_config.max_delayed_retries {
                                                            let m = mgr.lock().unwrap_or_else(|e| e.into_inner());
                                                            let mut segs = m.segments.write().unwrap_or_else(|e| e.into_inner());
                                                            if let Some(s) = segs.iter_mut().find(|s| s.id == seg_id_dyn) {
                                                                s.downloaded_cursor = current_pos;
                                                                s.state = crate::downloader::structures::SegmentState::Error;
                                                            }
                                                            return;
                                                        }
                                                        let backoff = crate::downloader::network::calculate_backoff(retry_state.current_delay, &retry_config);
                                                        retry_state.current_delay = backoff;
                                                        tokio::time::sleep(backoff).await;
                                                        break;
                                                    }
                                                    None => break,
                                                }
                                            }
                                        }
                                    }
                                }
                            });
                        }
                    }
                    // ── END DYNAMIC SEGMENT SPLITTING ──

                    // Compress to tuple format
                    let slim_segments: Vec<SlimSegment> = segments.iter().map(|s| (
                        s.id,
                        s.start_byte,
                        s.end_byte,
                        s.downloaded_cursor,
                        s.state as u8,
                        s.speed_bps
                    )).collect();

                    let total_speed: u64 = segments.iter().map(|s| s.speed_bps).sum();
                    let payload = Payload { 
                        id: id_monitor.clone(), 
                        downloaded: d, 
                        total: total_size,
                        speed_bps: total_speed,
                        segments: slim_segments.clone()
                    };
                    let _ = window_monitor.emit("download_progress", payload.clone());
                    let _ = crate::http_server::get_event_sender().send(serde_json::to_value(&payload).unwrap_or(serde_json::json!(null)));

                    // INTEGRATION POINT 2: Update group member progress
                    if let Some((gid, mid)) = &group_context_monitor {
                        let progress = if total_size > 0 {
                            ((d as f64 / total_size as f64) * 100.0).clamp(0.0, 100.0)
                        } else {
                            0.0
                        };
                        if let Ok(mut scheduler) = GLOBAL_GROUP_SCHEDULER.lock() {
                            scheduler.update_member_progress(gid, mid, progress);
                        }
                    }

                    if total_size > 0 && d >= total_size {
                        let seg_count = manager_monitor.lock().unwrap_or_else(|e| e.into_inner())
                            .segments.read().unwrap_or_else(|e| e.into_inner()).len() as u32;

                        if let Err(integrity_error) = verify_queued_integrity(&window_monitor, &id_monitor, &path_monitor).await {
                            mark_retry_for_fresh_restart(&id_monitor);
                            let _ = stop_tx_monitor.send(());
                            
                            // INTEGRATION POINT 4c: Mark group member as failed on integrity failure
                            if let Some((gid, mid)) = &group_context_monitor {
                                if let Ok(mut scheduler) = GLOBAL_GROUP_SCHEDULER.lock() {
                                    if let Err(e) = scheduler.fail_member(gid, mid, &integrity_error) {
                                        eprintln!("[{}] Failed to mark member {} failed in group {}: {}", 
                                                  id_monitor, mid, gid, e);
                                    }
                                }
                            }
                            
                            record_integrity_failure(
                                &window_monitor,
                                &id_monitor,
                                &url_monitor,
                                &path_monitor,
                                total_size,
                                &monitor_start_iso,
                                monitor_start.elapsed(),
                                seg_count,
                                &integrity_error,
                            );
                            handle_download_failure_cleanup(&window_monitor, &id_monitor);
                            break;
                        }

                        finalize_http_success_side_effects(
                            &window_monitor,
                            id_monitor.clone(),
                            url_monitor.clone(),
                            path_monitor.clone(),
                            total_size,
                            monitor_start_iso.clone(),
                            monitor_start.elapsed(),
                            seg_count,
                            md5_monitor.clone(),
                            etag_monitor.clone(),
                            chatops_monitor.clone(),
                        );
                        
                        // INTEGRATION POINT 3: Mark group member as complete
                        if let Some((gid, mid)) = &group_context_monitor {
                            if let Ok(mut scheduler) = GLOBAL_GROUP_SCHEDULER.lock() {
                                if let Err(e) = scheduler.complete_member(gid, mid) {
                                    eprintln!("[{}] Failed to mark member {} complete in group {}: {}", 
                                              id_monitor, mid, gid, e);
                                }
                            }
                            // Trigger group engine to start ready downloads and emit events
                            crate::group_engine::on_download_complete(&window_monitor, &id_monitor);
                        }
                        
                        // Signal save loop and workers to stop
                        let _ = stop_tx_monitor.send(());
                        // Notify queue manager that a slot opened up (success path resolves deps)
                        {
                            let mut queue = crate::queue_manager::DOWNLOAD_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
                            queue.mark_finished_success(&id_monitor);
                        }
                        crate::bandwidth_allocator::ALLOCATOR.deregister(&id_monitor);
                        // Clean up session from in-memory state to prevent memory leak
                        if let Some(app_state) = window_monitor.try_state::<crate::core_state::AppState>() {
                            let mut downloads = app_state.downloads.lock().unwrap_or_else(|e| e.into_inner());
                            downloads.remove(&id_monitor);
                            app_state.unregister_streaming_source(&id_monitor);
                        }

                        if let Some(action) = crate::scheduler::handle_download_complete(&id_monitor) {
                            crate::scheduler::execute_end_action(&window_monitor, &action);
                        }
                        
                        break;
                    }
                }
            }
        }
    });

    // 9. Register with bandwidth allocator (default config)
    crate::bandwidth_allocator::ALLOCATOR.register(&id, crate::bandwidth_allocator::BandwidthConfig::default());

    // 10. Spawn Worker Threads
    let mut handles = Vec::new();
    
    // Apply per-host connection limits from site rules
    state.connection_manager.configure_for_url(&actual_url);

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
        let disk_io_error_worker = disk_io_error.clone();
        let retry_config = segment_retry_config.clone();

        let handle = tokio::spawn(async move {
            let (start, mut end, mut seg_id) = {
                let m = manager_clone.lock().unwrap_or_else(|e| e.into_inner());
                let mut segs = m.segments.write().unwrap_or_else(|e| e.into_inner());
                let seg = &mut segs[i];
                seg.state = crate::downloader::structures::SegmentState::Downloading;
                // Use downloaded_cursor (not start_byte) so resumed segments
                // continue from where they left off instead of re-downloading.
                (seg.downloaded_cursor, seg.end_byte, seg.id)
            };

            if end == 0 || start >= end { return; }

            let mut current_pos = start;
            let mut retry_state = crate::downloader::network::RetryState::from_config(&retry_config);
            let mut bytes_since_cursor_update: u64 = 0;
            let segment_start_time = std::time::Instant::now();
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

                // Check for disk I/O errors — stop feeding data to a dead writer
                if disk_io_error_worker.load(std::sync::atomic::Ordering::Acquire) {
                    let m = manager_clone.lock().unwrap_or_else(|e| e.into_inner());
                    let mut segs = m.segments.write().unwrap_or_else(|e| e.into_inner());
                    if let Some(seg) = segs.iter_mut().find(|s| s.id == seg_id) {
                        seg.downloaded_cursor = current_pos;
                        seg.state = crate::downloader::structures::SegmentState::Error;
                    }
                    break;
                }

                if current_pos >= end {
                    // Calculate latency and record success
                    let elapsed_ms = segment_start_time.elapsed().as_millis() as f64;
                    GLOBAL_MIRROR_SCORER.record_success(&url_clone, elapsed_ms);
                    
                    // Work-stealing: mark this segment complete and try to take work from a slower segment
                    let stolen = {
                        let m = manager_clone.lock().unwrap_or_else(|e| e.into_inner());
                        m.on_segment_complete(seg_id)
                    };
                    if let Some(work) = stolen {
                        // Continue downloading the stolen segment
                        seg_id = work.new_segment.id;
                        current_pos = work.new_segment.start_byte;
                        end = work.new_segment.end_byte;
                        retry_state = crate::downloader::network::RetryState::from_config(&retry_config);
                        bytes_since_cursor_update = 0;
                        println!("[Worker] Segment complete, stole new segment {} ({}-{})", seg_id, current_pos, end);
                        continue;
                    } else {
                        // No work to steal — worker exits
                        break;
                    }
                }

                let range_header = format!("bytes={}-{}", current_pos, end - 1);
                
                // Acquire permit via ConnectionManager
                let _permit = cm_clone.acquire(&url_clone).await.ok();

                // Chaos Mode Check: Inject latency or failure here
                if let Err(_e) = crate::network::chaos::check_chaos().await {
                     retry_state.immediate_attempts += 1;
                     if retry_state.immediate_attempts <= retry_config.max_immediate_retries {
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
                        let strategy = crate::downloader::network::analyze_error(&e);
                        retry_state.last_error = Some(format!("{}", e));
                        match strategy {
                            crate::downloader::network::RetryStrategy::Fatal(msg) => {
                                eprintln!("[seg {}] Fatal error: {}", seg_id, msg);
                                GLOBAL_MIRROR_SCORER.record_failure(&url_clone);
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
                            crate::downloader::network::RetryStrategy::Immediate => {
                                retry_state.immediate_attempts += 1;
                                if retry_state.immediate_attempts > retry_config.max_immediate_retries {
                                    eprintln!("[seg {}] Exceeded immediate retries ({})", seg_id, retry_config.max_immediate_retries);
                                    GLOBAL_MIRROR_SCORER.record_failure(&url_clone);
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
                                continue;
                            }
                            crate::downloader::network::RetryStrategy::Delayed(delay) => {
                                retry_state.delayed_attempts += 1;
                                if retry_state.delayed_attempts > retry_config.max_delayed_retries {
                                    eprintln!("[seg {}] Exceeded delayed retries ({})", seg_id, retry_config.max_delayed_retries);
                                    GLOBAL_MIRROR_SCORER.record_failure(&url_clone);
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
                                let backoff = crate::downloader::network::calculate_backoff(
                                    delay.max(retry_state.current_delay),
                                    &retry_config,
                                );
                                retry_state.current_delay = backoff;
                                tokio::time::sleep(delay).await;
                                continue;
                            }
                            crate::downloader::network::RetryStrategy::RefreshLink => {
                                // Treat like 403 — trigger hot-swap
                                continue;
                            }
                        }
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
                         last_active: Some(chrono::Utc::now().to_rfc3339()),
                         error_message: None,
                         expected_checksum: get_expected_checksum(&id_worker),
                     };
                     
                     let _ = persistence::upsert_download(saved);
                     
                     // 3. Notify UI
                     let payload = Payload {
                         id: id_worker.clone(),
                         downloaded: total_downloaded,
                         total: 0, 
                         speed_bps: 0,
                         segments: vec![],
                     };
                     let _ = app_handle_clone.emit("download_progress", payload.clone());
                     let _ = crate::http_server::get_event_sender().send(serde_json::to_value(&payload).unwrap_or(serde_json::json!(null)));
                     
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
                     retry_state.delayed_attempts += 1;
                     if retry_state.delayed_attempts > retry_config.max_delayed_retries {
                         eprintln!("[seg {}] Rate-limited too many times ({}), giving up", seg_id, retry_config.max_delayed_retries);
                         GLOBAL_MIRROR_SCORER.record_failure(&url_clone);
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
                     let wait_time = if let Some(h) = response.headers().get("Retry-After") {
                         if let Ok(s) = h.to_str() {
                             crate::downloader::network::parse_retry_after(s).unwrap_or(std::time::Duration::from_secs(30))
                         } else {
                             std::time::Duration::from_secs(30)
                         }
                     } else {
                         crate::downloader::network::calculate_backoff(retry_state.current_delay, &retry_config)
                     };
                     retry_state.current_delay = wait_time;

                     tokio::time::sleep(wait_time).await;
                     continue;
                }

                // Reset retry state on successful connection — transient blips don't accumulate
                retry_state.reset_with_delay(retry_config.initial_delay);

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

                                    // Apply global speed limit (token-bucket throttle)
                                    crate::speed_limiter::GLOBAL_LIMITER.acquire(len).await;

                                    // Apply per-download bandwidth allocation
                                    crate::bandwidth_allocator::ALLOCATOR.acquire(&id_worker, len).await;

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
                                Some(Err(stream_err)) => {
                                    retry_state.delayed_attempts += 1;
                                    retry_state.last_error = Some(format!("Stream error: {}", stream_err));
                                    if retry_state.delayed_attempts > retry_config.max_delayed_retries {
                                        GLOBAL_MIRROR_SCORER.record_failure(&url_clone);
                                        let m = manager_clone.lock().unwrap_or_else(|e| e.into_inner());
                                        let mut segs = m.segments.write().unwrap_or_else(|e| e.into_inner());
                                        if let Some(seg) = segs.iter_mut().find(|s| s.id == seg_id) {
                                            seg.downloaded_cursor = current_pos;
                                            seg.state = crate::downloader::structures::SegmentState::Error;
                                        }
                                        return;
                                    }
                                    let backoff = crate::downloader::network::calculate_backoff(retry_state.current_delay, &retry_config);
                                    retry_state.current_delay = backoff;
                                    tokio::time::sleep(backoff).await;
                                    break; // Break inner stream loop, retry outer request loop
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
                        last_active: Some(chrono::Utc::now().to_rfc3339()),
                        error_message: None,
                        expected_checksum: get_expected_checksum(&id_save),
                    };
                    // Silent save, ignore errors
                    let _ = persistence::upsert_download(saved);
                }
            }
        }
    });
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corrupt_retry_path_keeps_parent_and_marks_file() {
        let original = std::path::Path::new("/tmp/archive.zip");
        let quarantined = corrupt_retry_path(original.to_str().unwrap());
        let quarantined_path = std::path::Path::new(&quarantined);

        assert_eq!(quarantined_path.parent(), original.parent());
        assert!(quarantined_path.file_name().unwrap().to_string_lossy().starts_with("archive.zip.corrupt-"));
    }
}
// Group Integration Tests
#[cfg(test)]
mod group_integration_tests {
    use super::super::*;
    use crate::download_groups::{DownloadGroup, GroupState, ExecutionStrategy};
    use crate::group_scheduler::{GroupScheduler, GLOBAL_GROUP_SCHEDULER, ExecutionState};

    #[test]
    fn test_group_dependency_checking_blocks_unmet_dependencies() {
        let mut scheduler = GroupScheduler::new();
        let mut group = DownloadGroup::new("Test Group");
        
        // Create two members with dependency
        let m1 = group.add_member("http://example.com/file1.txt", None);
        let m2 = group.add_member("http://example.com/file2.txt", Some(vec![m1.clone()]));
        
        // m2 has unmet dependency on m1
        assert!(!scheduler.can_start_member(&group.id, &m2));
        
        // m1 should be ready (no dependencies)
        assert!(scheduler.can_start_member(&group.id, &m1));
        
        // Schedule the group
        assert!(scheduler.schedule_group(group.clone()).is_ok());
        
        // Verify readiness check works
        assert!(!scheduler.can_start_member(&group.id, &m2));
        assert!(scheduler.can_start_member(&group.id, &m1));
    }

    #[test]
    fn test_group_progress_clamping() {
        let mut scheduler = GroupScheduler::new();
        let mut group = DownloadGroup::new("Progress Test");
        let member_id = group.add_member("http://example.com/file.txt", None);
        
        scheduler.schedule_group(group).unwrap();
        
        // Update progress above 100% - should clamp
        scheduler.update_member_progress(&group.id, &member_id, 150.0);
        
        if let Some(group) = scheduler.get_group(&group.id) {
            if let Some(member) = group.members.get(&member_id) {
                assert_eq!(member.progress_percent, 100.0);
                assert_eq!(member.state, GroupState::Completed);
            }
        }
        
        // Test negative progress - should clamp to 0
        let mut scheduler2 = GroupScheduler::new();
        let mut group2 = DownloadGroup::new("Progress Test 2");
        let member_id2 = group2.add_member("http://example.com/file2.txt", None);
        scheduler2.schedule_group(group2).unwrap();
        
        scheduler2.update_member_progress(&group.id, &member_id2, -50.0);
        
        if let Some(group) = scheduler2.get_group(&group.id) {
            if let Some(member) = group.members.get(&member_id2) {
                assert_eq!(member.progress_percent, 0.0);
            }
        }
    }

    #[test]
    fn test_member_completion_auto_completes_group() {
        let mut scheduler = GroupScheduler::new();
        let mut group = DownloadGroup::new("Single Member Group");
        let member_id = group.add_member("http://example.com/file.txt", None);
        let group_id = group.id.clone();
        
        scheduler.schedule_group(group).unwrap();
        
        // Initially, group should not be complete
        assert!(!scheduler.is_group_complete(&group_id));
        
        // Complete the member
        assert!(scheduler.complete_member(&group_id, &member_id).is_ok());
        
        // Now group should be complete
        assert!(scheduler.is_group_complete(&group_id));
        
        // Group state should be Completed
        if let Some(group) = scheduler.get_group(&group_id) {
            assert_eq!(group.state, GroupState::Completed);
        }
    }

    #[test]
    fn test_multiple_members_group_partial_completion() {
        let mut scheduler = GroupScheduler::new();
        let mut group = DownloadGroup::new("Multi Member Group");
        
        // Add three independent members
        let m1 = group.add_member("http://example.com/file1.txt", None);
        let m2 = group.add_member("http://example.com/file2.txt", None);
        let m3 = group.add_member("http://example.com/file3.txt", None);
        let group_id = group.id.clone();
        
        scheduler.schedule_group(group).unwrap();
        
        // Complete first member
        scheduler.complete_member(&group_id, &m1).unwrap();
        assert!(!scheduler.is_group_complete(&group_id));
        
        // Complete second member
        scheduler.complete_member(&group_id, &m2).unwrap();
        assert!(!scheduler.is_group_complete(&group_id));
        
        // Complete third member
        scheduler.complete_member(&group_id, &m3).unwrap();
        assert!(scheduler.is_group_complete(&group_id));
    }

    #[test]
    fn test_failure_in_one_member_doesnt_affect_others() {
        let mut scheduler = GroupScheduler::new();
        let mut group = DownloadGroup::new("Failure Test Group");
        
        // Add two independent members
        let m1 = group.add_member("http://example.com/file1.txt", None);
        let m2 = group.add_member("http://example.com/file2.txt", None);
        let group_id = group.id.clone();
        
        scheduler.schedule_group(group).unwrap();
        
        // Mark first member as failed
        scheduler.fail_member(&group_id, &m1, "Network error").unwrap();
        
        // First member should be in Error state
        if let Some(group) = scheduler.get_group(&group_id) {
            assert_eq!(group.members[&m1].state, GroupState::Error);
            // Second member should still be Pending
            assert_eq!(group.members[&m2].state, GroupState::Pending);
        }
        
        // Group should be in Error state but second member should still be startable
        assert!(scheduler.can_start_member(&group_id, &m2));
    }

    #[test]
    fn test_dependency_ordering_sequential() {
        let mut scheduler = GroupScheduler::new();
        let mut group = DownloadGroup::new("Sequential Group");
        group.strategy = ExecutionStrategy::Sequential;
        
        let m1 = group.add_member("http://example.com/file1.txt", None);
        let m2 = group.add_member("http://example.com/file2.txt", Some(vec![m1.clone()]));
        let m3 = group.add_member("http://example.com/file3.txt", Some(vec![m2.clone()]));
        let group_id = group.id.clone();
        
        scheduler.schedule_group(group).unwrap();
        
        // Only m1 should be ready initially
        assert_eq!(scheduler.get_ready_members(&group_id), vec![m1.clone()]);
        
        // Complete m1
        scheduler.complete_member(&group_id, &m1).unwrap();
        
        // Now m2 should be ready
        assert_eq!(scheduler.get_ready_members(&group_id), vec![m2.clone()]);
        
        // Complete m2
        scheduler.complete_member(&group_id, &m2).unwrap();
        
        // Now m3 should be ready
        assert_eq!(scheduler.get_ready_members(&group_id), vec![m3.clone()]);
    }

    #[test]
    fn test_group_member_progress_tracking() {
        let mut scheduler = GroupScheduler::new();
        let mut group = DownloadGroup::new("Progress Tracking Group");
        let member_id = group.add_member("http://example.com/file.txt", None);
        let group_id = group.id.clone();
        
        scheduler.schedule_group(group).unwrap();
        
        // Update progress incrementally
        let progress_steps = vec![10.0, 25.0, 50.0, 75.0, 99.5, 100.0];
        
        for progress in progress_steps {
            scheduler.update_member_progress(&group_id, &member_id, progress);
            
            if let Some(group) = scheduler.get_group(&group_id) {
                if let Some(member) = group.members.get(&member_id) {
                    if progress < 100.0 {
                        assert_eq!(member.state, GroupState::Pending);
                    } else {
                        assert_eq!(member.state, GroupState::Completed);
                    }
                    assert!((member.progress_percent - progress.clamp(0.0, 100.0)).abs() < 0.01);
                }
            }
        }
    }

    #[test]
    fn test_group_with_complex_dependencies() {
        let mut scheduler = GroupScheduler::new();
        let mut group = DownloadGroup::new("Complex Dependencies");
        
        // Create a diamond dependency pattern:
        //     A (root)
        //    / \
        //   B   C
        //    \ /
        //     D
        let a = group.add_member("http://example.com/a.txt", None);
        let b = group.add_member("http://example.com/b.txt", Some(vec![a.clone()]));
        let c = group.add_member("http://example.com/c.txt", Some(vec![a.clone()]));
        let d = group.add_member("http://example.com/d.txt", Some(vec![b.clone(), c.clone()]));
        let group_id = group.id.clone();
        
        scheduler.schedule_group(group).unwrap();
        
        // Only A should be ready
        assert_eq!(scheduler.get_ready_members(&group_id), vec![a.clone()]);
        
        // Complete A
        scheduler.complete_member(&group_id, &a).unwrap();
        
        // Now B and C should be ready
        let mut ready = scheduler.get_ready_members(&group_id);
        ready.sort();
        let mut expected = vec![b.clone(), c.clone()];
        expected.sort();
        assert_eq!(ready, expected);
        
        // Complete B and C
        scheduler.complete_member(&group_id, &b).unwrap();
        scheduler.complete_member(&group_id, &c).unwrap();
        
        // Now D should be ready
        assert_eq!(scheduler.get_ready_members(&group_id), vec![d.clone()]);
    }

    #[test]
    fn test_get_completed_and_pending_members() {
        let mut scheduler = GroupScheduler::new();
        let mut group = DownloadGroup::new("Status Test Group");
        
        let m1 = group.add_member("http://example.com/file1.txt", None);
        let m2 = group.add_member("http://example.com/file2.txt", None);
        let m3 = group.add_member("http://example.com/file3.txt", None);
        let group_id = group.id.clone();
        
        scheduler.schedule_group(group).unwrap();
        
        // All should be pending initially
        let pending = scheduler.get_pending_members(&group_id);
        assert_eq!(pending.len(), 3);
        assert!(scheduler.get_completed_members(&group_id).is_empty());
        
        // Complete m1
        scheduler.complete_member(&group_id, &m1).unwrap();
        
        // Should have 1 completed and 2 pending
        assert_eq!(scheduler.get_completed_members(&group_id).len(), 1);
        let pending = scheduler.get_pending_members(&group_id);
        assert_eq!(pending.len(), 2);
    }
}
