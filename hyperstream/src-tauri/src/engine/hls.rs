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
                crate::media::sounds::play_complete();
                return Ok(());
            }
        }
    }

    // 4. open output file (resume support uses total downloaded bytes)
    let saved_downloads = persistence::load_downloads().unwrap_or_default();
    let saved = saved_downloads.iter().find(|d| d.id == id);
    let resume_from = saved.map(|s| s.downloaded_bytes).unwrap_or(0);
    // Smart filename collision avoidance for new downloads
    let path = if resume_from == 0 {
        crate::engine::session::resolve_filename_collision(&path)
    } else {
        path
    };
    let file = initialization::setup_file(&path, resume_from, total_size)?;
    let file_mutex = file;

    // 5. register HLS session in state
    let (stop_tx, _) = broadcast::channel(1);
    let session = HlsSession {
        manifest_url: chosen_url.clone(),
        segments: stream.segments.clone(),
        segment_sizes: sizes.clone(),
        downloaded: Arc::new(std::sync::atomic::AtomicU64::new(resume_from)),
        stop_tx: stop_tx.clone(),
        file_writer: file_mutex.clone(),
    };
    {
        let mut map = state.hls_sessions.lock().unwrap_or_else(|e| e.into_inner());
        map.insert(id.clone(), session);
    }

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
    let downloaded_atomic = Arc::new(std::sync::atomic::AtomicU64::new(resume_from));
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
        let path_clone = path.clone();
        let key_map: Arc<Mutex<HashMap<String, Vec<u8>>>> = Arc::new(Mutex::new(HashMap::new()));

        futures.push(tokio::spawn(async move {
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
                    return Err(());
                }
            };
            let mut stream = resp.bytes_stream();
            let mut local_pos = seg_offset;
            while let Some(item) = futures::stream::StreamExt::next(&mut stream).await {
                if let Ok(chunk) = item {
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
                        return Err(());
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
                }
                // check stop
                if stop_rx.try_recv().is_ok() {
                    break;
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
            Ok(())
        }));

        // throttle concurrency
        if futures.len() >= concurrency {
            let _ = futures::stream::StreamExt::next(&mut futures).await;
        }
    }

    // wait for remaining futures
    while futures.next().await.is_some() {}

    // cleaned up; remove session state
    {
        let mut map = state.hls_sessions.lock().unwrap_or_else(|e| e.into_inner());
        map.remove(&id);
    }

    // mark persistence as complete
    let _ = persistence::upsert_download(persistence::SavedDownload {
        id: id.clone(),
        url: chosen_url.clone(),
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
    let elapsed = hls_start.elapsed();
    let avg_speed = if elapsed.as_secs() > 0 { total_size / elapsed.as_secs() } else { 0 };
    let _ = crate::download_history::record(crate::download_history::HistoryEntry {
        id: id.clone(),
        url: chosen_url.clone(),
        path: path.clone(),
        filename: crate::engine::session::extract_filename(&path).to_string(),
        total_size,
        downloaded_bytes: total_size,
        status: "Complete".to_string(),
        started_at: hls_start_iso,
        finished_at: chrono::Local::now().to_rfc3339(),
        avg_speed_bps: avg_speed,
        duration_secs: elapsed.as_secs(),
        segments_used: 0,
        error_message: None,
        source_type: Some("hls".to_string()),
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
                "source": "hls",
            }),
        });
    }

    // Webhooks
    {
        let id2 = id.clone();
        let path2 = path.clone();
        let url2 = chosen_url.clone();
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
                    eprintln!("[{}] HLS auto-sort failed: {}", id, e);
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
            connection_manager: ConnectionManager::new(),
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
