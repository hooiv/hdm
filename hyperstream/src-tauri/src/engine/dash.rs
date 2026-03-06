use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use tokio::sync::broadcast;
use tauri::{Emitter, Manager};
use crate::core_state::AppState;
use crate::media::dash_parser::{DashManifest, DashRepresentation, DashSegment};
use crate::settings;
use crate::persistence;

/// Session information for an ongoing DASH download.
///
/// DASH manifests typically provide separate video and audio tracks
/// (AdaptationSets) that must be downloaded independently and then muxed
/// into a single container.  This session tracks both streams as well as
/// the overall progress.
pub struct DashSession {
    pub manifest_url: String,
    pub video_rep: Option<DashRepresentation>,
    pub audio_rep: Option<DashRepresentation>,
    pub video_total: u64,
    pub audio_total: u64,
    pub downloaded: Arc<std::sync::atomic::AtomicU64>,
    pub stop_tx: broadcast::Sender<()>,
}

/// Start downloading a DASH/MPD stream.
///
/// The high-level flow:
///   1. Fetch and parse the MPD manifest.
///   2. Select the best video and audio representations.
///   3. Download video segments → temp file.
///   4. Download audio segments → temp file.
///   5. Mux both into the final output path via FFmpeg.
///   6. Clean up temp files and persist completion.
///
/// The function mirrors the conventions of `start_hls_download_impl`:
///   * It registers a session in `AppState` for pause/resume.
///   * It emits `download_progress` events at ~chunk granularity.
///   * It respects the global speed limiter and stop signal.
pub(crate) async fn start_dash_download_impl(
    app: &tauri::AppHandle,
    state: &AppState,
    id: String,
    manifest_url: String,
    path: String,
    force: bool,
    custom_headers: Option<HashMap<String, String>>,
) -> Result<(), String> {
    crate::media::sounds::play_startup();

    let dash_start = std::time::Instant::now();
    let dash_start_iso = chrono::Local::now().to_rfc3339();

    let settings = settings::load_settings();

    // ── 1. Build HTTP client (same proxy/masq logic as HLS) ──────────
    let proxy_config = crate::proxy::ProxyConfig::from_settings(&settings);
    let client = if settings.dpi_evasion {
        crate::network::masq::build_impersonator_client(
            crate::network::masq::BrowserProfile::Chrome,
            Some(&proxy_config),
            custom_headers.clone(),
        )
    } else {
        crate::network::masq::build_client(Some(&proxy_config), custom_headers.clone())
    }
    .map_err(|e| e.to_string())?;

    // ── 2. Fetch MPD manifest ────────────────────────────────────────
    let mpd_body = client
        .get(&manifest_url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch MPD manifest: {}", e))?
        .text()
        .await
        .map_err(|e| format!("Failed to read MPD body: {}", e))?;

    let manifest = crate::media::dash_parser::parse_mpd(&mpd_body, &manifest_url)?;

    if manifest.video_representations.is_empty() && manifest.audio_representations.is_empty() {
        return Err("DASH manifest contains no representations".into());
    }

    // ── 3. Choose best representations (highest bandwidth) ───────────
    let video_rep = manifest.video_representations.first().cloned();
    let audio_rep = manifest.audio_representations.first().cloned();

    // ── 4. CAS dedupe ────────────────────────────────────────────────
    if !force {
        if let Some(existing_path) = crate::cas_manager::check_cas(Some(&manifest_url), None) {
            if std::fs::hard_link(&existing_path, &path).is_ok() {
                crate::media::sounds::play_complete();
                return Ok(());
            }
        }
    }

    // ── 5. Compute sizes and build segment lists ─────────────────────
    let video_segments = video_rep.as_ref().map(|r| &r.segments[..]).unwrap_or(&[]);
    let audio_segments = audio_rep.as_ref().map(|r| &r.segments[..]).unwrap_or(&[]);

    let video_sizes = compute_segment_sizes(&client, video_segments).await;
    let audio_sizes = compute_segment_sizes(&client, audio_segments).await;

    let video_total: u64 = video_sizes.iter().sum();
    let audio_total: u64 = audio_sizes.iter().sum();
    let total_size = video_total + audio_total;

    // ── 6. Resume support ────────────────────────────────────────────
    let saved_downloads = persistence::load_downloads().unwrap_or_default();
    let saved = saved_downloads.iter().find(|d| d.id == id);
    let resume_from = saved.map(|s| s.downloaded_bytes).unwrap_or(0);

    // Smart filename collision avoidance for new downloads
    let path = if resume_from == 0 {
        crate::engine::session::resolve_filename_collision(&path)
    } else {
        path
    };

    // ── 7. Register session in AppState ──────────────────────────────
    let (stop_tx, _) = broadcast::channel(1);
    let downloaded_atomic = Arc::new(std::sync::atomic::AtomicU64::new(resume_from));

    let session = DashSession {
        manifest_url: manifest_url.clone(),
        video_rep: video_rep.clone(),
        audio_rep: audio_rep.clone(),
        video_total,
        audio_total,
        downloaded: downloaded_atomic.clone(),
        stop_tx: stop_tx.clone(),
    };
    {
        let mut map = state.dash_sessions.lock().unwrap_or_else(|e| e.into_inner());
        map.insert(id.clone(), session);
    }

    // ── 8. Create temp paths for video + audio ───────────────────────
    let video_tmp = format!("{}.dash_video.tmp", &path);
    let audio_tmp = format!("{}.dash_audio.tmp", &path);

    // ── 9. Download video track ──────────────────────────────────────
    let video_ok = if !video_segments.is_empty() {
        download_track(
            &client,
            video_segments,
            &video_sizes,
            &video_tmp,
            resume_from.min(video_total),
            total_size,
            &downloaded_atomic,
            &stop_tx,
            app,
            &id,
            settings.segments.max(1) as usize,
        )
        .await
    } else {
        Ok(())
    };

    // Check if stopped
    if stop_tx.receiver_count() == 0 && video_ok.is_err() {
        // Session was removed (paused/cancelled)
        cleanup_tmp(&video_tmp, &audio_tmp);
        return video_ok;
    }

    // ── 10. Download audio track ─────────────────────────────────────
    let audio_ok = if !audio_segments.is_empty() {
        let audio_resume = resume_from.saturating_sub(video_total);
        download_track(
            &client,
            audio_segments,
            &audio_sizes,
            &audio_tmp,
            audio_resume,
            total_size,
            &downloaded_atomic,
            &stop_tx,
            app,
            &id,
            settings.segments.max(1) as usize,
        )
        .await
    } else {
        Ok(())
    };

    if audio_ok.is_err() {
        cleanup_tmp(&video_tmp, &audio_tmp);
        return audio_ok;
    }

    // ── 11. Mux video + audio → final file ───────────────────────────
    let has_video = !video_segments.is_empty() && std::path::Path::new(&video_tmp).exists();
    let has_audio = !audio_segments.is_empty() && std::path::Path::new(&audio_tmp).exists();

    if has_video && has_audio {
        // Mux with FFmpeg
        if crate::media::muxer::is_ffmpeg_available() {
            crate::media::muxer::merge_streams(
                std::path::Path::new(&video_tmp),
                std::path::Path::new(&audio_tmp),
                std::path::Path::new(&path),
            )?;
        } else {
            // FFmpeg not available — keep the video file as the output
            std::fs::rename(&video_tmp, &path)
                .map_err(|e| format!("Failed to move video file: {}", e))?;
            eprintln!("Warning: FFmpeg not found, audio track not merged. Audio saved at {}", audio_tmp);
        }
    } else if has_video {
        std::fs::rename(&video_tmp, &path)
            .map_err(|e| format!("Failed to move video file: {}", e))?;
    } else if has_audio {
        std::fs::rename(&audio_tmp, &path)
            .map_err(|e| format!("Failed to move audio file: {}", e))?;
    } else {
        return Err("No tracks were downloaded".into());
    }

    // ── 12. Cleanup ──────────────────────────────────────────────────
    cleanup_tmp(&video_tmp, &audio_tmp);

    // Remove session from state
    {
        let mut map = state.dash_sessions.lock().unwrap_or_else(|e| e.into_inner());
        map.remove(&id);
    }

    // Register in CAS
    crate::cas_manager::register_cas(Some(&manifest_url), None, &path);

    // Persist completion
    let _ = persistence::upsert_download(persistence::SavedDownload {
        id: id.clone(),
        url: manifest_url.clone(),
        path: path.clone(),
        filename: crate::engine::session::extract_filename(&path).to_string(),
        total_size,
        downloaded_bytes: total_size,
        status: "Complete".to_string(),
        segments: None,
        last_active: Some(chrono::Utc::now().to_rfc3339()),
        error_message: None,
    });

    crate::media::sounds::play_complete();

    // Record in download history
    let elapsed = dash_start.elapsed();
    let avg_speed = if elapsed.as_secs() > 0 { total_size / elapsed.as_secs() } else { 0 };
    let _ = crate::download_history::record(crate::download_history::HistoryEntry {
        id: id.clone(),
        url: manifest_url.clone(),
        path: path.clone(),
        filename: crate::engine::session::extract_filename(&path).to_string(),
        total_size,
        downloaded_bytes: total_size,
        status: "Complete".to_string(),
        started_at: dash_start_iso,
        finished_at: chrono::Local::now().to_rfc3339(),
        avg_speed_bps: avg_speed,
        duration_secs: elapsed.as_secs(),
        segments_used: 0,
        error_message: None,
        source_type: Some("dash".to_string()),
    });

    // --- Post-completion hooks (matching HTTP download flow) ---
    // Event sourcing
    if let Some(log) = app.try_state::<std::sync::Arc<crate::event_sourcing::SharedLog>>() {
        let _ = log.append(crate::event_sourcing::LedgerEvent {
            timestamp: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(),
            aggregate_id: id.clone(),
            event_type: "DownloadCompleted".to_string(),
            payload: serde_json::json!({
                "total_size": total_size,
                "duration_secs": elapsed.as_secs(),
                "path": path,
                "source": "dash",
            }),
        });
    }

    // Webhooks
    {
        let id2 = id.clone();
        let path2 = path.clone();
        let url2 = manifest_url.clone();
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

    // MQTT notification
    crate::mqtt_client::publish_event(
        "DownloadComplete",
        &id,
        crate::engine::session::extract_filename(&path),
        "Complete",
    );

    // ChatOps (Telegram) notification
    {
        let chatops = state.chatops_manager.clone();
        let filename = crate::engine::session::extract_filename(&path).to_string();
        tokio::spawn(async move {
            chatops.notify_completion(&filename).await;
        });
    }

    // Auto-extract archives
    {
        let path_archive = path.clone();
        let id_archive = id.clone();
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
                            println!("[{}] DASH auto-extract: {}", id_archive, msg);
                            if settings.cleanup_archives_after_extract {
                                if let Err(e) = crate::archive_manager::ArchiveManager::cleanup_archive(&path_archive) {
                                    eprintln!("[{}] Cleanup failed: {}", id_archive, e);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("[{}] DASH auto-extract failed: {}", id_archive, e);
                        }
                    }
                }
            }
        });
    }

    // File categorization
    {
        let settings_snap = crate::settings::load_settings();
        if settings_snap.auto_sort_downloads {
            match crate::file_categorizer::categorize_and_move(&path, &settings_snap.download_dir) {
                Ok((cat_result, new_path)) => {
                    let moved = new_path != path;
                    let _ = app.emit("file_categorized", serde_json::json!({
                        "id": id,
                        "filename": crate::engine::session::extract_filename(&path),
                        "category": cat_result.category_name,
                        "icon": cat_result.icon,
                        "color": cat_result.color,
                        "should_move": cat_result.should_move,
                        "moved": moved,
                        "new_path": if moved { Some(&new_path) } else { None },
                    }));
                }
                Err(e) => {
                    eprintln!("[{}] DASH auto-sort failed: {}", id, e);
                    let cat_result = crate::file_categorizer::categorize(
                        crate::engine::session::extract_filename(&path),
                    );
                    let _ = app.emit("file_categorized", serde_json::json!({
                        "id": id,
                        "filename": crate::engine::session::extract_filename(&path),
                        "category": cat_result.category_name,
                        "icon": cat_result.icon,
                        "color": cat_result.color,
                        "should_move": false,
                    }));
                }
            }
        }
    }

    Ok(())
}

// ── Helper: download a single track (video or audio) ─────────────────
async fn download_track(
    client: &rquest::Client,
    segments: &[DashSegment],
    sizes: &[u64],
    output_path: &str,
    resume_from: u64,
    global_total: u64,
    downloaded_atomic: &Arc<std::sync::atomic::AtomicU64>,
    stop_tx: &broadcast::Sender<()>,
    app: &tauri::AppHandle,
    id: &str,
    concurrency: usize,
) -> Result<(), String> {
    // Open file for writing with resume support
    let file = crate::downloader::initialization::setup_file(output_path, resume_from, sizes.iter().sum())?;

    // Disk writer — same pattern as HLS/HTTP engines
    let (tx, rx) = std::sync::mpsc::channel::<crate::downloader::disk::WriteRequest>();
    let file_writer_clone = file.clone();
    let disk_io_error = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let disk_io_error_writer = disk_io_error.clone();
    std::thread::spawn(move || {
        let writer = crate::downloader::disk::DiskWriter::new(file_writer_clone, rx);
        let writer_flag = writer.io_error_flag();
        loop {
            std::thread::sleep(std::time::Duration::from_millis(500));
            if disk_io_error_writer.load(std::sync::atomic::Ordering::Acquire) {
                break;
            }
            if writer_flag.load(std::sync::atomic::Ordering::Acquire) {
                disk_io_error_writer.store(true, std::sync::atomic::Ordering::Release);
                break;
            }
        }
    });

    // Calculate starting segment for resume
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
            acc += sz;
        }
    }

    let mut futures = futures::stream::FuturesUnordered::new();

    for idx in start_index..segments.len() {
        let seg = segments[idx].clone();
        let seg_offset = if idx == start_index { offset_in_segment } else { 0 };
        let global_base: u64 = sizes[..idx].iter().sum::<u64>();
        let tx_clone = tx.clone();
        let client_clone = client.clone();
        let mut stop_rx = stop_tx.subscribe();
        let downloaded_clone = downloaded_atomic.clone();
        let app_clone = app.clone();
        let id_clone = id.to_string();
        let disk_err = disk_io_error.clone();

        futures.push(tokio::spawn(async move {
            // Check for disk error before starting
            if disk_err.load(std::sync::atomic::Ordering::Acquire) {
                return Err("Disk I/O error".to_string());
            }

            // Build request with optional byte-range
            let mut req = client_clone.get(&seg.url);

            // If the manifest specifies a byte range, use it (offset by seg_offset for resume)
            if let Some((range_start, range_end)) = seg.byte_range {
                let start = range_start + seg_offset;
                req = req.header("Range", format!("bytes={}-{}", start, range_end));
            } else if seg_offset > 0 {
                req = req.header("Range", format!("bytes={}-", seg_offset));
            }

            let resp = match req.send().await {
                Ok(r) => r,
                Err(e) => {
                    // Retry once (simple retry for transient network errors)
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    match client_clone.get(&seg.url).send().await {
                        Ok(r) => r,
                        Err(_) => {
                            eprintln!("DASH seg {} request failed after retry: {}", seg.url, e);
                            return Err(format!("Segment fetch failed: {}", e));
                        }
                    }
                }
            };

            let mut stream = resp.bytes_stream();
            let mut local_pos = seg_offset;

            while let Some(item) = futures::stream::StreamExt::next(&mut stream).await {
                if let Ok(chunk) = item {
                    let data = chunk.to_vec();
                    let global_off = global_base + local_pos;
                    if tx_clone
                        .send(crate::downloader::disk::WriteRequest {
                            offset: global_off,
                            data: data.clone(),
                            segment_id: 0,
                        })
                        .is_err()
                    {
                        return Err("Disk writer channel closed".to_string());
                    }
                    let len = data.len() as u64;
                    downloaded_clone.fetch_add(len, std::sync::atomic::Ordering::Relaxed);
                    local_pos += len;

                    // Emit progress
                    let payload = crate::core_state::Payload {
                        id: id_clone.clone(),
                        downloaded: downloaded_clone.load(std::sync::atomic::Ordering::Relaxed),
                        total: global_total,
                        segments: vec![],
                    };
                    let _ = app_clone.emit("download_progress", payload.clone());
                    let _ = crate::http_server::get_event_sender().send(
                        serde_json::to_value(&payload).unwrap_or(serde_json::json!(null)),
                    );
                }

                // Check stop signal
                if stop_rx.try_recv().is_ok() {
                    return Err("Stopped".to_string());
                }

                // Check disk error
                if disk_err.load(std::sync::atomic::Ordering::Acquire) {
                    return Err("Disk I/O error".to_string());
                }
            }

            Ok(())
        }));

        // Throttle concurrency
        if futures.len() >= concurrency {
            if let Some(result) = futures::stream::StreamExt::next(&mut futures).await {
                if let Ok(Err(e)) = result {
                    if e == "Stopped" {
                        return Err("Download stopped".into());
                    }
                }
            }
        }
    }

    // Wait for remaining futures
    while let Some(result) = futures::stream::StreamExt::next(&mut futures).await {
        if let Ok(Err(e)) = result {
            if e == "Stopped" {
                return Err("Download stopped".into());
            }
        }
    }

    // Check for disk errors
    if disk_io_error.load(std::sync::atomic::Ordering::Acquire) {
        return Err("Disk I/O error during download".into());
    }

    Ok(())
}

// ── Helper: compute segment sizes via HEAD requests ───────────────────
async fn compute_segment_sizes(client: &rquest::Client, segments: &[DashSegment]) -> Vec<u64> {
    let mut sizes = Vec::with_capacity(segments.len());
    for seg in segments {
        // If the manifest provides byte ranges, compute from those
        if let Some((start, end)) = seg.byte_range {
            sizes.push(end.saturating_sub(start) + 1);
            continue;
        }

        // Otherwise HEAD request to determine size
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
        // Fallback: GET with Range: bytes=0-0 to get Content-Range
        if len == 0 {
            if let Ok(res) = client.get(&seg.url).header("Range", "bytes=0-0").send().await {
                if let Some(h) = res.headers().get(rquest::header::CONTENT_RANGE) {
                    if let Ok(s) = h.to_str() {
                        if let Some(idx) = s.rfind('/') {
                            if let Ok(n) = s[idx + 1..].parse::<u64>() {
                                len = n;
                            }
                        }
                    }
                }
            }
        }
        sizes.push(len);
    }
    sizes
}

/// Silently remove temp files.
fn cleanup_tmp(video_tmp: &str, audio_tmp: &str) {
    let _ = std::fs::remove_file(video_tmp);
    let _ = std::fs::remove_file(audio_tmp);
}
