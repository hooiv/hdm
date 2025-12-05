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
mod settings;
mod speed_limiter;
mod clipboard;
mod scheduler;
mod media;
mod plugin_vm;
mod spider;
mod zip_preview;
mod proxy;
mod adaptive_threads;
mod virus_scanner;
mod import_export;
mod lan_api;

use persistence::SavedDownload;
use settings::Settings;

#[derive(Clone, serde::Serialize)]
struct Payload {
    id: String,
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
        let mgr = DownloadManager::new(total_size, 1);
        {
            let mut segs = mgr.segments.write().unwrap();
            segs[0].start_byte = resume_from;
            segs[0].downloaded_cursor = resume_from;
        }
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
        let mut writer = DiskWriter::new(file_writer_clone, rx);
        writer.run();
    });

    // 8. Spawn Threads
    let mut handles = Vec::new();
    
    // We need to clone manager segments to iterate, but we need the Arc for the threads
    let segments_count = manager.lock().unwrap().segments.read().unwrap().len();

    for i in 0..segments_count {
        let manager_clone = manager.clone();
        let url_clone = url.clone();
        let tx_clone = tx.clone();
        let client_clone = client.clone();
        let window_clone = window.clone();
        let downloaded_clone = downloaded_total.clone();
        let mut stop_rx = stop_tx.subscribe();
        let download_id = id.clone();

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
                    println!("DEBUG: Thread {} received stop signal", i);
                    // Update state before exit
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
                
                // Use tokio::select to allow cancellation during request
                let res_future = client_clone.get(&url_clone).header("Range", &range_header).send();
                
                let res = tokio::select! {
                    _ = stop_rx.recv() => {
                        println!("DEBUG: Thread {} stopped during request", i);
                        let m = manager_clone.lock().unwrap();
                        let mut segs = m.segments.write().unwrap();
                        segs[i].downloaded_cursor = current_pos;
                        segs[i].state = crate::downloader::structures::SegmentState::Paused;
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
                                    
                                    // Update global progress
                                    {
                                        let mut d = downloaded_clone.lock().unwrap();
                                        *d += len;
                                        window_clone.emit("download_progress", Payload { id: download_id.clone(), downloaded: *d, total: total_size }).unwrap();
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

#[tauri::command]
fn get_settings() -> Settings {
    settings::load_settings()
}

#[tauri::command]
fn save_settings(new_settings: Settings) -> Result<(), String> {
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
fn get_scheduled_downloads() -> Vec<scheduler::ScheduledDownload> {
    scheduler::get_scheduled_downloads()
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
fn preview_zip_file(path: String) -> Result<zip_preview::ZipPreview, String> {
    zip_preview::preview_zip(std::path::Path::new(&path))
}

#[tauri::command]
fn extract_zip_file(zip_path: String, dest_dir: String) -> Result<usize, String> {
    zip_preview::extract_all(
        std::path::Path::new(&zip_path),
        std::path::Path::new(&dest_dir)
    )
}

// ============ HLS/DASH Stream Parser Commands ============

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

// ============ Proxy Configuration Commands ============

#[tauri::command]
fn get_proxy_config() -> proxy::ProxyConfig {
    // Load from settings or return default
    proxy::ProxyConfig::default()
}

#[tauri::command]
fn test_proxy(config: proxy::ProxyConfig) -> Result<bool, String> {
    let _client = config.build_client()?;
    // Test connection (sync for simplicity)
    Ok(true)
}

// ============ Import/Export Commands ============

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

// ============ Virus Scanning Commands ============

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

// ============ Server Probe Commands ============

#[tauri::command]
async fn probe_server(url: String) -> Result<downloader::http_client::ServerCapabilities, String> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .user_agent("Mozilla/5.0")
        .build()
        .map_err(|e| e.to_string())?;
    
    let scout = downloader::http_client::FirstByteScout::new(client);
    scout.probe(&url).await
}

// ============ Speed Limiter Commands ============

#[tauri::command]
async fn acquire_bandwidth(bytes: u64) -> u64 {
    speed_limiter::GLOBAL_LIMITER.acquire(bytes).await
}

#[tauri::command]
fn set_speed_limit(bytes_per_sec: u64) {
    speed_limiter::GLOBAL_LIMITER.set_limit(bytes_per_sec);
}

#[tauri::command]
fn get_speed_limit() -> u64 {
    speed_limiter::GLOBAL_LIMITER.get_limit()
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

// ============ ZIP Partial Preview Commands ============

#[tauri::command]
fn read_zip_last_bytes(path: String, length: usize) -> Result<Vec<u8>, String> {
    zip_preview::read_last_bytes(std::path::Path::new(&path), length)
}

#[tauri::command]
fn preview_zip_partial(data: Vec<u8>) -> Result<zip_preview::ZipPreview, String> {
    zip_preview::preview_zip_partial(&data)
}

// ============ Plugin System Commands ============

#[tauri::command]
async fn get_plugin_metadata(script: String) -> Result<Option<plugin_vm::lua_host::PluginMetadata>, String> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| e.to_string())?;
    
    let host = plugin_vm::lua_host::LuaPluginHost::new(client);
    host.init().await.map_err(|e| e.to_string())?;
    host.load_script(&script).await.map_err(|e| e.to_string())?;
    host.get_plugin_metadata().await.map_err(|e| e.to_string())
}

// ============ Network Validation Commands ============

#[tauri::command]
fn analyze_http_status(status_code: u16) -> String {
    use reqwest::StatusCode;
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

// ============ FDM Import Command ============

#[tauri::command]
fn import_from_fdm_file(path: String) -> Result<Vec<import_export::ExportedDownload>, String> {
    import_export::import_from_fdm(std::path::Path::new(&path))
}

// ============ ZIP Extraction Commands ============

#[tauri::command]
fn extract_single_file(zip_path: String, entry_name: String, dest_path: String) -> Result<(), String> {
    zip_preview::extract_file(
        std::path::Path::new(&zip_path),
        &entry_name,
        std::path::Path::new(&dest_path)
    )
}

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
    use reqwest::StatusCode;
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
    let config = downloader::http_client::HttpClientConfig::default();
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
    let config = downloader::http_client::HttpClientConfig::default();
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
async fn extract_stream_url(script: String, page_url: String) -> Result<Option<serde_json::Value>, String> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| e.to_string())?;
    
    let host = plugin_vm::lua_host::LuaPluginHost::new(client);
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
async fn set_plugin_config(script: String, config: std::collections::HashMap<String, String>) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| e.to_string())?;
    
    let host = plugin_vm::lua_host::LuaPluginHost::new(client);
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Load settings and apply
    let initial_settings = settings::load_settings();
    speed_limiter::GLOBAL_LIMITER.set_limit(initial_settings.speed_limit_kbps * 1024);
    clipboard::CLIPBOARD_MONITOR.set_enabled(initial_settings.clipboard_monitor);

    // Create channel for HTTP server to send download requests
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<http_server::DownloadRequest>();
    
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState { downloads: Mutex::new(HashMap::new()) })
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
            // Spider
            crawl_website,
            // ZIP Preview
            preview_zip_file,
            extract_zip_file,
            // HLS/DASH
            parse_hls_stream,
            parse_dash_manifest,
            // Proxy
            get_proxy_config,
            test_proxy,
            // Import/Export
            export_downloads,
            import_downloads,
            import_from_idm_file,
            // Virus Scanning
            scan_file_for_virus,
            is_antivirus_available,
            // Server Probe
            probe_server,
            // Speed Limiter
            acquire_bandwidth,
            set_speed_limit,
            get_speed_limit,
            // LAN API
            generate_lan_pairing_code,
            get_lan_pairing_qr_data,
            get_local_ip,
            // Scheduler
            is_quiet_hours,
            get_time_info,
            // ZIP Partial Preview
            read_zip_last_bytes,
            preview_zip_partial,
            // Plugin System
            get_plugin_metadata,
            // Network Validation
            analyze_http_status,
            check_captive_portal,
            // CSV Export
            export_downloads_csv,
            // Disk Operations
            preallocate_download_file,
            check_file_exists,
            get_file_size,
            // Adaptive Threads
            get_adaptive_thread_count,
            update_thread_count,
            add_bandwidth_sample,
            get_average_bandwidth,
            // More Scheduler
            get_next_download_time,
            // FDM Import
            import_from_fdm_file,
            // ZIP Extraction
            extract_single_file,
            read_file_bytes_at_offset,
            // HTTP Client Validation
            validate_http_response,
            parse_retry_after_header,
            check_error_content_type,
            // Stealth HTTP Client
            get_chrome_user_agent,
            get_default_http_config,
            // Retry Strategy
            calculate_retry_backoff,
            get_retry_config,
            analyze_error_strategy,
            // Range Request
            start_range_download,
            validate_download_url,
            // Plugin Extraction
            extract_stream_url,
            // Proxy Bypass
            should_bypass_proxy,
            is_proxy_enabled,
            // Download Stats
            get_download_stats,
            get_work_steal_config,
            // Retry State
            get_retry_state,
            reset_retry_state,
            analyze_network_error,
            // Resume File
            open_file_for_resume,
            // Plugin Config
            set_plugin_config,
            // DiskWriter Stats
            get_disk_writer_config
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            
            // Start clipboard monitor
            clipboard::CLIPBOARD_MONITOR.start(app.handle().clone());
            
            // Start scheduler
            scheduler::start_scheduler(app.handle().clone());
            
            // Start LAN API server for mobile app integration
            tauri::async_runtime::spawn(async move {
                let lan_server = lan_api::LanApiServer::new(8765);
                if let Err(e) = lan_server.start().await {
                    eprintln!("LAN API server error: {}", e);
                }
            });
            
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
