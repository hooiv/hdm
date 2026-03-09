use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use tokio::sync::broadcast;
use reqwest::Client;
use futures::{stream::FuturesUnordered, StreamExt};
use tauri::{Emitter, Manager};
use crate::core_state::{AppState, HlsSession};
use crate::media::{HlsParser, HlsSegment};
use crate::downloader::initialization;
use crate::downloader::network;
use crate::settings;
use crate::persistence;

const HLS_STOPPED_ERROR: &str = "Download stopped";

fn remove_hls_session(state: &AppState, id: &str) {
    let mut map = state.hls_sessions.lock().unwrap_or_else(|e| e.into_inner());
    map.remove(id);
}

fn record_hls_failure(
    app: &tauri::AppHandle,
    id: &str,
    url: &str,
    path: &str,
    total_size: u64,
    downloaded_bytes: u64,
    started_at: &str,
    elapsed: std::time::Duration,
    segments_used: u32,
    error_message: &str,
) {
    let _ = app.emit("download_error", serde_json::json!({
        "id": id,
        "error": error_message,
    }));
    crate::media::sounds::play_error();

    let _ = persistence::upsert_download(persistence::SavedDownload {
        id: id.to_string(),
        url: url.to_string(),
        path: path.to_string(),
        filename: crate::engine::session::extract_filename(path).to_string(),
        total_size,
        downloaded_bytes,
        status: "Error".to_string(),
        segments: None,
        last_active: Some(chrono::Utc::now().to_rfc3339()),
        error_message: Some(error_message.to_string()),
    });

    let avg_speed = if elapsed.as_secs() > 0 {
        downloaded_bytes / elapsed.as_secs()
    } else {
        0
    };

    let _ = crate::download_history::record(crate::download_history::HistoryEntry {
        id: id.to_string(),
        url: url.to_string(),
        path: path.to_string(),
        filename: crate::engine::session::extract_filename(path).to_string(),
        total_size,
        downloaded_bytes,
        status: "Error".to_string(),
        started_at: started_at.to_string(),
        finished_at: chrono::Local::now().to_rfc3339(),
        avg_speed_bps: avg_speed,
        duration_secs: elapsed.as_secs(),
        segments_used,
        error_message: Some(error_message.to_string()),
        source_type: Some("hls".to_string()),
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
                "downloaded_bytes": downloaded_bytes,
                "total_size": total_size,
                "source": "hls",
            }),
        });
    }
}

fn handle_hls_failure(
    app: &tauri::AppHandle,
    state: &AppState,
    id: &str,
    url: &str,
    path: &str,
    total_size: u64,
    downloaded_bytes: u64,
    started_at: &str,
    elapsed: std::time::Duration,
    segments_used: u32,
    error_message: &str,
) {
    record_hls_failure(
        app,
        id,
        url,
        path,
        total_size,
        downloaded_bytes,
        started_at,
        elapsed,
        segments_used,
        error_message,
    );
    remove_hls_session(state, id);
    crate::engine::session::handle_download_failure_cleanup(app, id);
}

async fn finalize_hls_success(
    app: &tauri::AppHandle,
    state: &AppState,
    id: &str,
    url: &str,
    path: &str,
    total_size: u64,
    started_at: &str,
    elapsed: std::time::Duration,
) -> Result<(), String> {
    if let Err(integrity_error) = crate::engine::session::verify_queued_integrity(app, id, path).await {
        crate::engine::session::mark_retry_for_fresh_restart(id);
        handle_hls_failure(
            app,
            state,
            id,
            url,
            path,
            total_size,
            total_size,
            started_at,
            elapsed,
            0,
            &integrity_error,
        );
        return Ok(());
    }

    crate::engine::session::clear_retry_metadata(id);
    crate::media::sounds::play_complete();
    crate::cas_manager::register_cas(Some(url), None, path);

    let _ = persistence::upsert_download(persistence::SavedDownload {
        id: id.to_string(),
        url: url.to_string(),
        path: path.to_string(),
        filename: crate::engine::session::extract_filename(path).to_string(),
        total_size,
        downloaded_bytes: total_size,
        status: "Complete".to_string(),
        segments: None,
        last_active: Some(chrono::Utc::now().to_rfc3339()),
        error_message: None,
    });

    let avg_speed = if elapsed.as_secs() > 0 { total_size / elapsed.as_secs() } else { 0 };
    let _ = crate::download_history::record(crate::download_history::HistoryEntry {
        id: id.to_string(),
        url: url.to_string(),
        path: path.to_string(),
        filename: crate::engine::session::extract_filename(path).to_string(),
        total_size,
        downloaded_bytes: total_size,
        status: "Complete".to_string(),
        started_at: started_at.to_string(),
        finished_at: chrono::Local::now().to_rfc3339(),
        avg_speed_bps: avg_speed,
        duration_secs: elapsed.as_secs(),
        segments_used: 0,
        error_message: None,
        source_type: Some("hls".to_string()),
    });

    if let Some(log) = app.try_state::<std::sync::Arc<crate::event_sourcing::SharedLog>>() {
        let _ = log.append(crate::event_sourcing::LedgerEvent {
            timestamp: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
            aggregate_id: id.to_string(),
            event_type: "DownloadCompleted".to_string(),
            payload: serde_json::json!({
                "total_size": total_size,
                "duration_secs": elapsed.as_secs(),
                "path": path,
                "source": "hls",
            }),
        });
    }

    {
        let id2 = id.to_string();
        let path2 = path.to_string();
        let url2 = url.to_string();
        tokio::spawn(async move {
            let settings = crate::settings::load_settings();
            if let Some(webhooks) = settings.webhooks {
                let manager = crate::webhooks::WebhookManager::new();
                manager.load_configs(webhooks).await;
                let payload = crate::webhooks::WebhookPayload {
                    event: "DownloadComplete".to_string(),
                    download_id: id2,
                    filename: crate::engine::session::extract_filename(&path2).to_string(),
                    url: url2,
                    size: total_size,
                    speed: 0,
                    filepath: Some(path2),
                    timestamp: chrono::Utc::now().timestamp(),
                };
                manager.trigger(crate::webhooks::WebhookEvent::DownloadComplete, payload).await;
            }
        });
    }

    crate::mqtt_client::publish_event(
        "DownloadComplete",
        id,
        crate::engine::session::extract_filename(path),
        "Complete",
    );

    {
        let chatops = state.chatops_manager.clone();
        let filename = crate::engine::session::extract_filename(path).to_string();
        tokio::spawn(async move {
            chatops.notify_completion(&filename).await;
        });
    }

    {
        let path_archive = path.to_string();
        let id_archive = id.to_string();
        tokio::spawn(async move {
            let settings = crate::settings::load_settings();
            if settings.auto_extract_archives {
                if let Some(_archive_info) = crate::archive_manager::ArchiveManager::detect_archive(&path_archive) {
                    let dest = std::path::Path::new(&path_archive)
                        .parent()
                        .and_then(|p| p.to_str())
                        .unwrap_or(".")
                        .to_string();
                    match crate::archive_manager::ArchiveManager::extract_archive(&path_archive, &dest) {
                        Ok(msg) => {
                            println!("[{}] HLS auto-extract: {}", id_archive, msg);
                            if settings.cleanup_archives_after_extract {
                                if let Err(e) = crate::archive_manager::ArchiveManager::cleanup_archive(&path_archive) {
                                    eprintln!("[{}] Cleanup failed: {}", id_archive, e);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("[{}] HLS auto-extract failed: {}", id_archive, e);
                        }
                    }
                }
            }
        });
    }

    {
        let settings_snap = crate::settings::load_settings();
        if settings_snap.auto_sort_downloads {
            match crate::file_categorizer::categorize_and_move(path, &settings_snap.download_dir) {
                Ok((cat_result, new_path)) => {
                    let moved = new_path != path;
                    let _ = app.emit("file_categorized", serde_json::json!({
                        "id": id,
                        "filename": crate::engine::session::extract_filename(path),
                        "category": cat_result.category_name,
                        "icon": cat_result.icon,
                        "color": cat_result.color,
                        "should_move": cat_result.should_move,
                        "moved": moved,
                        "new_path": if moved { Some(&new_path) } else { None },
                    }));
                }
                Err(e) => {
                    eprintln!("[{}] HLS auto-sort failed: {}", id, e);
                    let cat_result = crate::file_categorizer::categorize(
                        crate::engine::session::extract_filename(path),
                    );
                    let _ = app.emit("file_categorized", serde_json::json!({
                        "id": id,
                        "filename": crate::engine::session::extract_filename(path),
                        "category": cat_result.category_name,
                        "icon": cat_result.icon,
                        "color": cat_result.color,
                        "should_move": false,
                    }));
                }
            }
        }
    }

    {
        let mut queue = crate::queue_manager::DOWNLOAD_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
        queue.mark_finished(id);
    }
    remove_hls_session(state, id);

    if let Some(action) = crate::scheduler::handle_download_complete(id) {
        crate::scheduler::execute_end_action(app, &action);
    }

    Ok(())
}

/// Download an HLS stream into a single file.  This function is analogous to
/// `start_download_impl` but specialised for segmented media.
///
/// Unlike a regular HTTP download we don't issue ranged requests against the
/// same URL; we fetch each playlist segment individually and stitch them
/// together into the destination file.  To support pause/resume we record the
/// number of bytes already written and on restart skip ahead to the correct
/// place in the playlist.
///
/// `force` behaves the same as in the regular downloader (ignore CAS, etc.).
pub(crate) async fn start_hls_download_impl(
    app: &tauri::AppHandle,
    state: &AppState,
    id: String,
    manifest_url: String,
    path: String,
    force: bool,
    custom_headers: Option<HashMap<String, String>>,
) -> Result<(), String> {
    // play start sound
    crate::media::sounds::play_startup();

    let hls_start = std::time::Instant::now();
    let hls_start_iso = chrono::Local::now().to_rfc3339();

    let settings = settings::load_settings();
    let fresh_restart = crate::engine::session::queued_retry_requires_fresh_restart(&id);
    if fresh_restart {
        crate::engine::session::quarantine_corrupt_file(&path)?;
    }

    // choose a HTTP client (respect proxy/masq, DPI, headers, etc.)
    let proxy_config = crate::proxy::ProxyConfig::from_settings(&settings);
    let client = if settings.dpi_evasion {
        crate::network::masq::build_impersonator_client(crate::network::masq::BrowserProfile::Chrome, Some(&proxy_config), custom_headers.clone())
    } else {
        crate::network::masq::build_client(Some(&proxy_config), custom_headers.clone())
    }.map_err(|e| e.to_string())?;

    // HlsParser uses reqwest::Client; build a plain one for manifest parsing
    let parser_client = reqwest::Client::builder()
        .build()
        .map_err(|e| e.to_string())?;
    let parser = HlsParser::new(parser_client);

    // 1. parse playlist and follow variants if necessary
    let mut stream = parser.parse(&manifest_url).await?;
    let mut chosen_url = manifest_url.clone();
    if stream.is_master {
        // pick highest bandwidth by default (parser already sorted desc)
        if let Some(best) = stream.variants.get(0) {
            chosen_url = best.url.clone();
            stream = parser.parse(&chosen_url).await?;
        } else {
            return Err("master playlist contained no variants".into());
        }
    }

    if stream.segments.is_empty() {
        return Err("HLS stream contains no media segments".into());
    }

    // 2. compute segment sizes (HEAD requests) and cumulative total
    let mut sizes: Vec<u64> = Vec::with_capacity(stream.segments.len());
    let mut total_size: u64 = 0;
    for seg in &stream.segments {
        // use HEAD to cheaply determine size; some servers don't reply so fall
        // back to GET with range/bytes=0-0 if necessary
        let mut len = 0u64;
        if let Ok(res) = client.head(&seg.url).send().await {
            if let Some(h) = res.headers().get(rquest::header::CONTENT_LENGTH) {
                if let Ok(s) = h.to_str() {
                    if let Ok(n) = s.parse::<u64>() {
                        len = n;
                    }
                }
            }
        }
        if len == 0 {
            // fallback
            if let Ok(res) = client.get(&seg.url).header("Range", "bytes=0-0").send().await {
                if let Some(h) = res.headers().get(rquest::header::CONTENT_RANGE) {
                    // format: bytes 0-0/12345
                    if let Ok(s) = h.to_str() {
                        if let Some(idx) = s.rfind('/') {
                            if let Ok(n) = s[idx+1..].parse::<u64>() {
                                len = n;
                            }
                        }
                    }
                }
            }
        }
        sizes.push(len);
        total_size = total_size.saturating_add(len);
    }

    // 3. attempt CAS dedupe as regular downloads
    if !force {
        // dedupe based on manifest URL; quick check to skip entire fetch
        if let Some(existing_path) = crate::cas_manager::check_cas(Some(&manifest_url), None) {
            if std::fs::hard_link(&existing_path, &path).is_ok() {
                return finalize_hls_success(
                    app,
                    state,
                    &id,
                    &chosen_url,
                    &path,
                    total_size,
                    &hls_start_iso,
                    hls_start.elapsed(),
                )
                .await;
            }
        }
    }

    // 4. open output file (resume support uses total downloaded bytes)
    let saved_downloads = persistence::load_downloads().unwrap_or_default();
    let saved = if fresh_restart {
        None
    } else {
        saved_downloads.iter().find(|d| d.id == id)
    };
    let resume_from = if fresh_restart {
        0
    } else {
        saved.map(|s| s.downloaded_bytes).unwrap_or(0)
    };
    // Smart filename collision avoidance for new downloads
    let path = if resume_from == 0 && !fresh_restart {
        crate::engine::session::resolve_filename_collision(&path)
    } else {
        path
    };
    let file = initialization::setup_file(&path, resume_from, total_size)?;
    let file_mutex = file;
    let downloaded_atomic = Arc::new(std::sync::atomic::AtomicU64::new(resume_from));

    // 5. register HLS session in state
    let (stop_tx, _) = broadcast::channel(1);
    let session = HlsSession {
        manifest_url: chosen_url.clone(),
        output_path: path.clone(),
        segments: stream.segments.clone(),
        segment_sizes: sizes.clone(),
        downloaded: downloaded_atomic.clone(),
        stop_tx: stop_tx.clone(),
        file_writer: file_mutex.clone(),
    };
    {
        let mut map = state.hls_sessions.lock().unwrap_or_else(|e| e.into_inner());
        map.insert(id.clone(), session);
    }

    let id_save = id.clone();
    let url_save = chosen_url.clone();
    let path_save = path.clone();
    let filename_save = crate::engine::session::extract_filename(&path).to_string();
    let downloaded_save = downloaded_atomic.clone();
    let mut stop_rx_save = stop_tx.subscribe();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = stop_rx_save.recv() => break,
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(5)) => {
                    let saved = persistence::SavedDownload {
                        id: id_save.clone(),
                        url: url_save.clone(),
                        path: path_save.clone(),
                        filename: filename_save.clone(),
                        total_size,
                        downloaded_bytes: downloaded_save.load(std::sync::atomic::Ordering::Relaxed),
                        status: "Downloading".to_string(),
                        segments: None,
                        last_active: Some(chrono::Utc::now().to_rfc3339()),
                        error_message: None,
                    };
                    let _ = persistence::upsert_download(saved);
                }
            }
        }
    });

    // 6. spawn disk writer thread
    let (tx, rx) = std::sync::mpsc::channel::<crate::downloader::disk::WriteRequest>();
    let file_writer_clone = file_mutex.clone();
    let disk_io_error = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let disk_io_error_writer = disk_io_error.clone();
    std::thread::spawn(move || {
        let mut writer = crate::downloader::disk::DiskWriter::new(file_writer_clone, rx);
        let writer_flag = writer.io_error_flag();
        let error_bridge = disk_io_error_writer.clone();
        loop {
            // periodically copy flag
            std::thread::sleep(std::time::Duration::from_millis(500));
            if error_bridge.load(std::sync::atomic::Ordering::Acquire) {
                break;
            }
            if writer_flag.load(std::sync::atomic::Ordering::Acquire) {
                error_bridge.store(true, std::sync::atomic::Ordering::Release);
                break;
            }
        }
    });

    // 7. spawn worker tasks (concurrent segments)
    let mut start_index = 0usize;
    let mut offset_in_segment = 0u64;
    {
        let mut acc = 0u64;
        for (i, sz) in sizes.iter().enumerate() {
            if acc + sz > resume_from {
                start_index = i;
                offset_in_segment = resume_from.saturating_sub(acc);
                break;
            }
            acc += *sz;
        }
    }

    let concurrency = settings.segments.max(1) as usize;
    let mut futures = futures::stream::FuturesUnordered::new();

    for idx in start_index..stream.segments.len() {
        let seg = stream.segments[idx].clone();
        let seg_size = sizes[idx];
        let mut seg_offset = if idx == start_index { offset_in_segment } else { 0 };
        let global_base: u64 = sizes[..idx].iter().sum::<u64>();
        let tx_clone = tx.clone();
        let client_clone = client.clone();
        let mut stop_rx = stop_tx.subscribe();
        let downloaded_clone = downloaded_atomic.clone();
        let app_clone = app.clone();
        let id_clone = id.clone();
        let key_map: Arc<Mutex<HashMap<String, Vec<u8>>>> = Arc::new(Mutex::new(HashMap::new()));
        let disk_io_error_clone = disk_io_error.clone();

        futures.push(tokio::spawn(async move {
            if disk_io_error_clone.load(std::sync::atomic::Ordering::Acquire) {
                return Err("Disk I/O error".to_string());
            }

            // fetch decryption key if needed
            let mut key_bytes_opt: Option<Vec<u8>> = None;
            if let Some(key_uri) = &seg.key_uri {
                // Check cache first, drop lock before any await
                let cached = {
                    let km = key_map.lock().unwrap();
                    km.get(key_uri).cloned()
                };
                if let Some(k) = cached {
                    key_bytes_opt = Some(k);
                } else {
                    if let Ok(resp) = client_clone.get(key_uri).send().await {
                        if let Ok(kbytes) = resp.bytes().await {
                            let v = kbytes.to_vec();
                            key_bytes_opt = Some(v.clone());
                            let mut km = key_map.lock().unwrap();
                            km.insert(key_uri.clone(), v);
                        }
                    }
                }
            }

            // build request; we can request the whole segment and skip seg_offset
            let mut req = client_clone.get(&seg.url);
            if seg_offset > 0 {
                let range = format!("bytes={}-", seg_offset);
                req = req.header("Range", range);
            }
            let resp = match req.send().await {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("HLS seg {} request failed: {}", seg.url, e);
                    return Err(format!("Segment request failed: {}", e));
                }
            };
            let mut stream = resp.bytes_stream();
            let mut local_pos = seg_offset;
            while let Some(item) = futures::stream::StreamExt::next(&mut stream).await {
                let chunk = match item {
                    Ok(chunk) => chunk,
                    Err(e) => return Err(format!("Segment stream failed: {}", e)),
                };
                let mut data = chunk.to_vec();
                // decrypt if necessary
                if let Some(key_bytes) = &key_bytes_opt {
                    // compute iv
                    let iv = if let Some(ivhex) = &seg.key_iv {
                        crate::media::decrypt::decode_hex(ivhex).unwrap_or_else(|_| {
                            let mut iv = [0u8;16];
                            iv[8..].copy_from_slice(&seg.sequence.to_be_bytes());
                            iv.to_vec()
                        })
                    } else {
                        let mut iv = [0u8;16];
                        iv[8..].copy_from_slice(&seg.sequence.to_be_bytes());
                        iv.to_vec()
                    };
                    if let Ok(dec) = crate::media::decrypt::decrypt_aes128(&data, &key_bytes, &iv) {
                        data = dec;
                    }
                }

                // write data to disk at global offset
                let global_off = global_base + local_pos;
                if tx_clone.send(crate::downloader::disk::WriteRequest { offset: global_off, data: data.clone(), segment_id: 0 }).is_err() {
                    return Err("Disk writer channel closed".to_string());
                }
                let len = data.len() as u64;
                downloaded_clone.fetch_add(len, std::sync::atomic::Ordering::Relaxed);
                local_pos += len;
                // send immediate progress update
                let payload = crate::core_state::Payload {
                    id: id_clone.clone(),
                    downloaded: downloaded_clone.load(std::sync::atomic::Ordering::Relaxed),
                    total: total_size,
                    segments: vec![],
                };
                let _ = app_clone.emit("download_progress", payload.clone());
                let _ = crate::http_server::get_event_sender().send(serde_json::to_value(&payload).unwrap_or(serde_json::json!(null)));

                if stop_rx.try_recv().is_ok() {
                    return Err(HLS_STOPPED_ERROR.to_string());
                }
                if disk_io_error_clone.load(std::sync::atomic::Ordering::Acquire) {
                    return Err("Disk I/O error".to_string());
                }
            }
            // emit progress once segment finishes (redundant but keeps behavior similar to HTTP)
            let payload = crate::core_state::Payload {
                id: id_clone.clone(),
                downloaded: downloaded_clone.load(std::sync::atomic::Ordering::Relaxed),
                total: total_size,
                segments: vec![],
            };
            let _ = app_clone.emit("download_progress", payload.clone());
            let _ = crate::http_server::get_event_sender().send(serde_json::to_value(&payload).unwrap_or(serde_json::json!(null)));
            Ok::<(), String>(())
        }));

        // throttle concurrency
        if futures.len() >= concurrency {
            if let Some(result) = futures::stream::StreamExt::next(&mut futures).await {
                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        let _ = stop_tx.send(());
                        while futures.next().await.is_some() {}
                        if e == HLS_STOPPED_ERROR {
                            remove_hls_session(state, &id);
                            return Ok(());
                        }
                        let downloaded = downloaded_atomic.load(std::sync::atomic::Ordering::Relaxed);
                        handle_hls_failure(
                            app,
                            state,
                            &id,
                            &chosen_url,
                            &path,
                            total_size,
                            downloaded,
                            &hls_start_iso,
                            hls_start.elapsed(),
                            stream.segments.len() as u32,
                            &e,
                        );
                        return Ok(());
                    }
                    Err(e) => {
                        let _ = stop_tx.send(());
                        while futures.next().await.is_some() {}
                        let downloaded = downloaded_atomic.load(std::sync::atomic::Ordering::Relaxed);
                        handle_hls_failure(
                            app,
                            state,
                            &id,
                            &chosen_url,
                            &path,
                            total_size,
                            downloaded,
                            &hls_start_iso,
                            hls_start.elapsed(),
                            stream.segments.len() as u32,
                            &format!("HLS worker join failed: {}", e),
                        );
                        return Ok(());
                    }
                }
            }
        }
    }

    // wait for remaining futures
    while let Some(result) = futures.next().await {
        match result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                if e == HLS_STOPPED_ERROR {
                    remove_hls_session(state, &id);
                    return Ok(());
                }
                let _ = stop_tx.send(());
                let downloaded = downloaded_atomic.load(std::sync::atomic::Ordering::Relaxed);
                handle_hls_failure(
                    app,
                    state,
                    &id,
                    &chosen_url,
                    &path,
                    total_size,
                    downloaded,
                    &hls_start_iso,
                    hls_start.elapsed(),
                    stream.segments.len() as u32,
                    &e,
                );
                return Ok(());
            }
            Err(e) => {
                let _ = stop_tx.send(());
                let downloaded = downloaded_atomic.load(std::sync::atomic::Ordering::Relaxed);
                handle_hls_failure(
                    app,
                    state,
                    &id,
                    &chosen_url,
                    &path,
                    total_size,
                    downloaded,
                    &hls_start_iso,
                    hls_start.elapsed(),
                    stream.segments.len() as u32,
                    &format!("HLS worker join failed: {}", e),
                );
                return Ok(());
            }
        }
    }

    if disk_io_error.load(std::sync::atomic::Ordering::Acquire) {
        let _ = stop_tx.send(());
        let downloaded = downloaded_atomic.load(std::sync::atomic::Ordering::Relaxed);
        handle_hls_failure(
            app,
            state,
            &id,
            &chosen_url,
            &path,
            total_size,
            downloaded,
            &hls_start_iso,
            hls_start.elapsed(),
            stream.segments.len() as u32,
            "Disk I/O error during download",
        );
        return Ok(());
    }

    let _ = stop_tx.send(());
    finalize_hls_success(
        app,
        state,
        &id,
        &chosen_url,
        &path,
        total_size,
        &hls_start_iso,
        hls_start.elapsed(),
    ).await
}


#[cfg(test)]
mod tests {
    use super::*;
    use tokio::runtime::Runtime;
    use std::sync::Arc;
    use std::fs;
    use warp::Filter;
    use crate::core_state::AppState;
    use crate::network::connection_manager::ConnectionManager;
    use crate::http_server;

    // helpers to build an AppState with minimal stubs
    fn make_test_state() -> AppState {
        AppState {
            downloads: Mutex::new(HashMap::new()),
            hls_sessions: Mutex::new(HashMap::new()),
            dash_sessions: Mutex::new(HashMap::new()),
            p2p_node: Arc::new(network::p2p::P2PNode::new()),
            p2p_file_map: http_server::FileMap::new(),
            torrent_manager: None,
            connection_manager: ConnectionManager::default(),
            chatops_manager: Arc::new(network::chatops::ChatOpsManager::new()),
        }
    }

    #[tokio::test]
    async fn test_parse_simple_media_playlist() {
        let client = Client::new();
        let parser = HlsParser::new(client);
        let manifest = "#EXTM3U\n#EXTINF:5,\nseg1.ts\n#EXTINF:5,\nseg2.ts\n";
        // call parser.process_media_playlist directly using base URL
        let base = url::Url::parse("http://localhost/").unwrap();
        let stream = parser.process_media_playlist(m3u8_rs::MediaPlaylist {
            version: None,
            media_sequence: 0,
            target_duration: 5.0,
            segments: vec![
                m3u8_rs::MediaSegment {
                    uri: "seg1.ts".to_string(),
                    duration: 5.0,
                    key: None,
                    byte_range: None,
                    discontinuity: false,
                    unknown: Vec::new(),
                },
                m3u8_rs::MediaSegment {
                    uri: "seg2.ts".to_string(),
                    duration: 5.0,
                    key: None,
                    byte_range: None,
                    discontinuity: false,
                    unknown: Vec::new(),
                },
            ],
            end_list: true,
            ..Default::default()
        }, &base);
        assert_eq!(stream.segments.len(), 2);
        assert!(!stream.is_master);
    }

    #[tokio::test]
    async fn test_hls_download_small_server() {
        // create a tiny HTTP server serving a playlist and two small segments
        let seg1 = b"AAAA".to_vec();
        let seg2 = b"BBBB".to_vec();
        let playlist = format!("#EXTM3U\n#EXTINF:0,\nhttp://127.0.0.1:3030/seg1.ts\n#EXTINF:0,\nhttp://127.0.0.1:3030/seg2.ts\n");
        let routes = warp::path("seg1.ts").map(move || warp::reply::with_header(seg1.clone(), "Content-Type", "video/mp2t"))
            .or(warp::path("seg2.ts").map(move || warp::reply::with_header(seg2.clone(), "Content-Type", "video/mp2t")))
            .or(warp::path("playlist.m3u8").map(move || warp::reply::with_header(playlist.clone(), "Content-Type", "application/vnd.apple.mpegurl")));

        let (_addr, server) = warp::serve(routes).bind_ephemeral(([127,0,0,1], 3030));
        tokio::task::spawn(server);

        let app = tauri::AppHandle::default(); // dummy handle, not used
        let state = make_test_state();
        let id = "test_hls".to_string();
        let out = std::env::temp_dir().join("hls_test.ts");
        // remove if exists
        let _ = fs::remove_file(&out);

        let result = start_hls_download_impl(&app, &state, id.clone(), "http://127.0.0.1:3030/playlist.m3u8".to_string(), out.to_string_lossy().to_string(), false, None).await;
        assert!(result.is_ok());
        // file should exist and contain concatenation
        let data = fs::read(&out).unwrap();
        assert_eq!(data, b"AAAABBBB");
    }
}
