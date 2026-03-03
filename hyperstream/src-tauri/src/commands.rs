
use tauri::{State, AppHandle, Manager};
use tauri::menu::Menu;
use tauri::tray::TrayIconBuilder;
use std::sync::{Arc, Mutex};
use futures_util::StreamExt;
use std::collections::HashMap;
use tokio::sync::broadcast;
use crate::commands;
use crate::downloader;
use crate::persistence;
use crate::http_server;
use crate::settings;
use crate::speed_limiter;
use crate::clipboard;
use crate::network;
use crate::scheduler;
use crate::media;
use crate::plugin_vm;
use crate::mqtt_client;
use crate::doi_resolver;
use crate::spider;
use crate::zip_preview;
use crate::proxy;
use crate::adaptive_threads;
use crate::import_export;
use crate::lan_api;
use crate::system_monitor;
use crate::feeds;
use crate::search;
use crate::cloud_bridge;
use crate::media_processor;
use crate::ai;
use crate::audio_events;
use crate::webhooks;
use crate::archive_manager;
use crate::metadata_scrubber;
use crate::ephemeral_server;
use crate::wayback;
use crate::docker_pull;
use crate::power_manager;
use crate::cas_manager;
use crate::warc_archiver;
use crate::git_lfs;
use crate::sandbox;
use crate::notarize;
use crate::mirror_hunter;
use crate::usb_flasher;
use crate::api_replay;
use crate::c2pa_validator;
use crate::bandwidth_arb;
use crate::stego_vault;
use crate::tui_dashboard;
use crate::auto_extract;
use crate::ipfs_gateway;
use crate::sql_query;
use crate::dlna_cast;
use crate::qos_manager;
use crate::mod_optimizer;
use crate::rclone_bridge;
use crate::subtitle_gen;
use crate::virtual_drive;
use crate::geofence;

#[tauri::command]
pub async fn add_magnet_link(
    magnet: String,
    state: State<'_, AppState>
) -> Result<usize, String> {
    println!("Adding magnet link: {}", magnet);
    state.torrent_manager.add_magnet(&magnet).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn play_torrent(
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
pub async fn get_torrents(
    state: State<'_, AppState>
) -> Result<Vec<network::bittorrent::manager::TorrentStatus>, String> {
    Ok(state.torrent_manager.get_torrents())
}

#[tauri::command]
pub async fn export_data(path: String) -> Result<(), String> {
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
pub async fn import_data(path: String) -> Result<(), String> {
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
pub async fn start_download(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
    url: String,
    path: String,
    _force: Option<bool>,
    custom_headers: Option<std::collections::HashMap<String, String>>
) -> Result<(), String> {
    start_download_impl(&app, &state, id, url, path, None, custom_headers).await
}

#[tauri::command]
pub async fn pause_download(
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
pub fn get_downloads() -> Result<Vec<SavedDownload>, String> {
    persistence::load_downloads()
}

#[tauri::command]
pub fn remove_download_entry(id: String) -> Result<(), String> {
    persistence::remove_download(&id)
}

#[tauri::command]
pub fn get_settings() -> serde_json::Value {
    let s = settings::load_settings();
    serde_json::to_value(s).unwrap_or(serde_json::json!({}))
}

#[tauri::command]
pub fn save_settings(json: serde_json::Value) -> Result<(), String> {
    let new_settings: settings::Settings = serde_json::from_value(json).map_err(|e| e.to_string())?;
    // Update speed limiter when settings change
    speed_limiter::GLOBAL_LIMITER.set_limit(new_settings.speed_limit_kbps * 1024);
    // Update clipboard monitor
    clipboard::CLIPBOARD_MONITOR.set_enabled(new_settings.clipboard_monitor);
    settings::save_settings(&new_settings)
}

#[tauri::command]
pub fn open_file(path: String) -> Result<(), String> {
    std::process::Command::new("cmd")
        .args(["/c", "start", "", &path])
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn open_folder(path: String) -> Result<(), String> {
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
pub fn schedule_download(id: String, url: String, filename: String, scheduled_time: String) -> Result<(), String> {
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
pub fn cancel_scheduled_download(id: String) -> Result<(), String> {
    scheduler::remove_scheduled_download(&id);
    Ok(())
}

#[tauri::command]
pub async fn crawl_website(
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

#[tauri::command]
pub fn preview_zip_partial(data: Vec<u8>) -> Result<zip_preview::ZipPreview, String> {
    zip_preview::preview_zip_partial(&data)
}

#[tauri::command]
pub fn preview_zip_file(path: String) -> Result<zip_preview::ZipPreview, String> {
    zip_preview::preview_zip(std::path::Path::new(&path))
}

#[tauri::command]
pub fn extract_single_file(zip_path: String, entry_name: String, dest_path: String) -> Result<(), String> {
    zip_preview::extract_file(
        std::path::Path::new(&zip_path),
        &entry_name,
        std::path::Path::new(&dest_path)
    )
}

#[tauri::command]
pub async fn preview_zip_remote(url: String) -> Result<zip_preview::ZipPreview, String> {
    let client = rquest::Client::builder()
        .build()
        .map_err(|e| e.to_string())?;
    zip_preview::preview_zip_remote(url, client).await
}

#[tauri::command]
pub async fn download_zip_entry(url: String, entry_name: String, dest_path: String) -> Result<(), String> {
    let client = rquest::Client::builder()
        .build()
        .map_err(|e| e.to_string())?;
    let bytes = zip_preview::download_entry_remote(url, entry_name, client).await?;
    std::fs::write(dest_path, bytes).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn read_zip_last_bytes(path: String, length: usize) -> Result<Vec<u8>, String> {
    zip_preview::read_last_bytes(std::path::Path::new(&path), length)
}

#[tauri::command]
pub async fn init_tor_network() -> Result<u16, String> {
    network::tor::init_tor().await
}

#[tauri::command]
pub fn get_tor_status() -> Option<u16> {
    network::tor::get_socks_port()
}

#[tauri::command]
pub async fn perform_semantic_search(query: String) -> Result<Vec<ai::SearchResult>, String> {
    ai::semantic_search(&query)
}

#[tauri::command]
pub async fn index_all_downloads() -> Result<usize, String> {
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
pub async fn join_workspace(host_ip: String) -> Result<(), String> {
    network::sync_client::connect_to_workspace(host_ip).await
}

#[tauri::command]
pub async fn parse_hls_stream(url: String) -> Result<media::HlsStream, String> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| e.to_string())?;
    
    let parser = media::HlsParser::new(client);
    parser.parse(&url).await
}

#[tauri::command]
pub fn parse_dash_manifest(content: String, base_url: String) -> Result<media::dash_parser::DashManifest, String> {
    media::dash_parser::parse_mpd(&content, &base_url)
}

#[tauri::command]
pub async fn mux_video_audio(video_path: String, audio_path: String, output_path: String) -> Result<(), String> {
    media::muxer::merge_streams(
        std::path::Path::new(&video_path),
        std::path::Path::new(&audio_path),
        std::path::Path::new(&output_path)
    )
}

#[tauri::command]
pub fn check_ffmpeg_installed() -> bool {
    media::muxer::is_ffmpeg_available()
}

#[tauri::command]
pub fn decrypt_aes_128(input_path: String, output_path: String, key_hex: String, iv_hex: String) -> Result<(), String> {
    let key = media::decrypt::decode_hex(&key_hex)?;
    let iv = media::decrypt::decode_hex(&iv_hex)?;
    
    let encrypted_data = std::fs::read(&input_path).map_err(|e| e.to_string())?;
    let decrypted = media::decrypt::decrypt_aes128(&encrypted_data, &key, &iv)?;
    
    std::fs::write(&output_path, decrypted).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn test_browser_fingerprint() -> Result<String, String> {
    let settings = settings::load_settings();
    let proxy_config = crate::proxy::ProxyConfig::from_settings(&settings);

    // Enable DPI evasion for the test to verify headers
    let client = network::masq::build_impersonator_client(network::masq::BrowserProfile::Chrome, Some(&proxy_config), None)
        .map_err(|e| e.to_string())?;
    
    // Hit a trace URL (using httpbin for now to show headers)
    let resp = client.get("https://httpbin.org/headers")
        .send()
        .await
        .map_err(|e| e.to_string())?;
        
    let text = resp.text().await.map_err(|e| e.to_string())?;
    Ok(text)
}

#[tauri::command]
pub fn get_proxy_config() -> serde_json::Value {
    let settings = settings::load_settings();
    let config = proxy::ProxyConfig::from_settings(&settings);
    serde_json::to_value(config).unwrap_or(serde_json::json!({}))
}

#[tauri::command]
pub async fn test_proxy(config: proxy::ProxyConfig) -> Result<bool, String> {
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

#[tauri::command]
// pub async fn mount_drive(id: String, path: String) -> Result<u16, String> {
//    virtual_drive::DRIVE_MANAGER.mount(id, path).await
// }

#[tauri::command]
// pub async fn unmount_drive(id: String) -> Result<(), String> {
//    virtual_drive::DRIVE_MANAGER.unmount(id)
// }

#[tauri::command]
pub async fn upload_to_cloud(app_handle: tauri::AppHandle, path: String, target_name: Option<String>) -> Result<String, String> {
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

#[tauri::command]
pub async fn process_media(app_handle: tauri::AppHandle, path: String, action: String) -> Result<String, String> {
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

    let final_path = crate::resolve_download_path(&path, &settings.download_dir)?;
    
    let input_str = final_path.to_str().ok_or_else(|| "Invalid path encoding".to_string())?;

    match action.as_str() {
        "preview" => {
            let output_path = final_path.with_extension("webp");
            media_processor::MediaProcessor::generate_preview(input_str, output_path.to_str().ok_or_else(|| "Invalid output path encoding".to_string())?)
        },
        "audio" => {
            let output_path = final_path.with_extension("mp3");
            media_processor::MediaProcessor::extract_audio(input_str, output_path.to_str().ok_or_else(|| "Invalid output path encoding".to_string())?)
        },
        _ => Err("Unknown action".to_string())
    }
}

#[tauri::command]
pub fn export_downloads(path: String) -> Result<(), String> {
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
pub fn import_downloads(path: String) -> Result<Vec<import_export::ExportedDownload>, String> {
    let export = import_export::HyperStreamExport::from_json_file(std::path::Path::new(&path))?;
    Ok(export.downloads)
}

#[tauri::command]
pub fn import_from_idm_file(path: String) -> Result<Vec<import_export::ExportedDownload>, String> {
    import_export::import_from_idm(std::path::Path::new(&path))
}

#[tauri::command]
pub fn export_downloads_csv(path: String) -> Result<(), String> {
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

#[tauri::command]
pub async fn scan_file_for_virus(path: String) -> Result<String, String> {
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
pub fn is_antivirus_available() -> bool {
    virus_scanner::VirusScanner::new().is_available()
}

#[tauri::command]
pub async fn acquire_bandwidth(amount: u32) -> Result<(), String> {
    speed_limiter::GLOBAL_LIMITER.acquire(amount as u64).await;
    Ok(())
}

#[tauri::command]
pub fn set_speed_limit(limit_kbps: u64) {
    speed_limiter::GLOBAL_LIMITER.set_limit(limit_kbps * 1024);
}

#[tauri::command]
pub fn get_speed_limit() -> u64 {
    speed_limiter::GLOBAL_LIMITER.get_limit() / 1024
}

#[tauri::command]
pub fn generate_lan_pairing_code() -> String {
    lan_api::LanApiServer::generate_pairing_code()
}

#[tauri::command]
pub fn get_lan_pairing_qr_data(port: u16, code: String) -> String {
    let server = lan_api::LanApiServer::new(port);
    server.get_pairing_qr_data(&code)
}

#[tauri::command]
pub fn get_local_ip() -> String {
    local_ip_address::local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string())
}

#[tauri::command]
pub fn is_quiet_hours() -> bool {
    scheduler::is_quiet_hours()
}

#[tauri::command]
pub fn get_time_info() -> scheduler::TimeInfo {
    scheduler::get_current_time_info()
}

#[tauri::command]
pub fn get_scheduled_downloads() -> Vec<scheduler::ScheduledDownload> {
    scheduler::get_scheduled_downloads()
}

#[tauri::command]
pub fn remove_scheduled_download(id: String) {
    scheduler::remove_scheduled_download(&id);
}

#[tauri::command]
pub async fn get_plugin_metadata(app_handle: tauri::AppHandle, script: String) -> Result<Option<plugin_vm::lua_host::PluginMetadata>, String> {
    let client = rquest::Client::builder()
        // .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| e.to_string())?;
    
    let host = plugin_vm::lua_host::LuaPluginHost::new(client, app_handle);
    host.init().await.map_err(|e| e.to_string())?;
    host.load_script(&script).await.map_err(|e| e.to_string())?;
    host.get_plugin_metadata().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn analyze_http_status(status_code: u16) -> String {
    use rquest::StatusCode;
    let status = StatusCode::from_u16(status_code).unwrap_or(StatusCode::OK);
    let strategy = downloader::network::analyze_status(status);
    format!("{:?}", strategy)
}

#[tauri::command]
pub fn check_captive_portal(first_bytes: Vec<u8>) -> bool {
    downloader::network::is_captive_portal(&first_bytes)
}

#[tauri::command]
pub fn preallocate_download_file(path: String, size: u64) -> Result<(), String> {
    downloader::disk::preallocate_file(std::path::Path::new(&path), size)
        .map(|_| ()) // Discard the file handle
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn check_file_exists(path: String) -> bool {
    std::path::Path::new(&path).exists()
}

#[tauri::command]
pub fn get_file_size(path: String) -> Result<u64, String> {
    std::fs::metadata(&path)
        .map(|m| m.len())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_adaptive_thread_count() -> u32 {
    THREAD_CONTROLLER.get_threads()
}

#[tauri::command]
pub fn update_thread_count(current_speed: u64, max_speed: u64) -> u32 {
    THREAD_CONTROLLER.update(current_speed, max_speed)
}

#[tauri::command]
pub fn add_bandwidth_sample(bytes: u64) {
    BANDWIDTH_MONITOR.add_sample(bytes);
}

#[tauri::command]
pub fn get_average_bandwidth() -> u64 {
    BANDWIDTH_MONITOR.get_average_speed()
}

#[tauri::command]
pub fn get_next_download_time() -> String {
    scheduler::get_next_download_time().to_rfc3339()
}

#[tauri::command]
pub fn import_from_fdm_file(path: String) -> Result<Vec<import_export::ExportedDownload>, String> {
    import_export::import_from_fdm(std::path::Path::new(&path))
}

#[tauri::command]
pub async fn fetch_feed(url: String) -> Result<Vec<feeds::FeedItem>, String> {
    feeds::fetch_feed(&url).await
}

#[tauri::command]
pub async fn perform_search(query: String) -> Result<Vec<search::SearchResult>, String> {
    let engine = search::SEARCH_ENGINE.lock().await;
    engine.search(query)
}

#[tauri::command]
pub fn get_feeds() -> Vec<feeds::FeedConfig> {
    feeds::FEED_MANAGER.get_feeds()
}

#[tauri::command]
pub fn add_feed(config: feeds::FeedConfig) {
    feeds::FEED_MANAGER.add_feed(config);
}

#[tauri::command]
pub fn remove_feed(id: String) {
    feeds::FEED_MANAGER.remove_feed(&id);
}

#[tauri::command]
pub fn read_file_bytes_at_offset(path: String, offset: u64, length: usize) -> Result<Vec<u8>, String> {
    zip_preview::read_bytes_at_offset(std::path::Path::new(&path), offset, length)
}

#[tauri::command]
pub fn validate_http_response(
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
pub fn parse_retry_after_header(value: String) -> Option<u64> {
    downloader::network::parse_retry_after(&value)
        .map(|d| d.as_secs())
}

#[tauri::command]
pub fn check_error_content_type(content_type: Option<String>, expected_type: Option<String>) -> bool {
    downloader::network::is_error_content_type(
        content_type.as_deref(),
        expected_type.as_deref()
    )
}

#[tauri::command]
pub fn get_chrome_user_agent() -> String {
    downloader::http_client::CHROME_USER_AGENT.to_string()
}

#[tauri::command]
pub fn get_default_http_config() -> serde_json::Value {
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

#[tauri::command]
pub fn calculate_retry_backoff(current_delay_ms: u64) -> u64 {
    let config = downloader::network::RetryConfig::default();
    let current = std::time::Duration::from_millis(current_delay_ms);
    let next = downloader::network::calculate_backoff(current, &config);
    next.as_millis() as u64
}

#[tauri::command]
pub fn get_retry_config() -> serde_json::Value {
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
pub fn analyze_error_strategy(error_type: String) -> String {
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

#[tauri::command]
pub async fn start_range_download(url: String, start: u64, end: u64) -> Result<Vec<u8>, String> {
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
pub async fn validate_download_url(url: String) -> Result<serde_json::Value, String> {
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

#[tauri::command]
pub async fn extract_stream_url(app_handle: tauri::AppHandle, script: String, page_url: String) -> Result<Option<serde_json::Value>, String> {
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

#[tauri::command]
pub fn should_bypass_proxy(url: String) -> bool {
    let config = proxy::ProxyConfig::default();
    config.should_bypass(&url)
}

#[tauri::command]
pub fn is_proxy_enabled() -> bool {
    let config = proxy::ProxyConfig::default();
    config.is_enabled()
}

#[tauri::command]
pub fn get_download_stats(file_size: u64, segment_count: u32) -> serde_json::Value {
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
pub fn get_work_steal_config() -> serde_json::Value {
    let config = downloader::structures::WorkStealConfig::default();
    serde_json::json!({
        "min_split_size": config.min_split_size,
        "steal_ratio": config.steal_ratio,
        "speed_threshold_ratio": config.speed_threshold_ratio
    })
}

#[tauri::command]
pub fn get_retry_state() -> serde_json::Value {
    let state = RETRY_STATE.lock().unwrap();
    serde_json::json!({
        "immediate_attempts": state.immediate_attempts,
        "delayed_attempts": state.delayed_attempts,
        "current_delay_ms": state.current_delay.as_millis() as u64,
        "last_error": state.last_error
    })
}

#[tauri::command]
pub fn reset_retry_state() {
    let mut state = RETRY_STATE.lock().unwrap();
    state.reset();
}

#[tauri::command]
pub fn analyze_network_error(error_type: String) -> String {
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

#[tauri::command]
pub fn open_file_for_resume(path: String) -> Result<u64, String> {
    let file = downloader::disk::open_for_resume(std::path::Path::new(&path))
        .map_err(|e| e.to_string())?;
    let size = file.metadata()
        .map(|m| m.len())
        .map_err(|e| e.to_string())?;
    Ok(size)
}

#[tauri::command]
pub async fn set_plugin_config(app_handle: tauri::AppHandle, script: String, config: std::collections::HashMap<String, String>) -> Result<(), String> {
    let client = rquest::Client::builder()
        // .min_tls_version(rquest::Version::TLS_1_2)
        .build()
        .map_err(|e| e.to_string())?;
    
    let host = plugin_vm::lua_host::LuaPluginHost::new(client, app_handle);
    host.init().await.map_err(|e| e.to_string())?;
    host.load_script(&script).await.map_err(|e| e.to_string())?;
    host.set_config(config).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_disk_writer_config() -> serde_json::Value {
    let config = downloader::disk::DiskWriterConfig::default();
    serde_json::json!({
        "max_pending_writes": config.max_pending_writes,
        "coalesce_threshold": config.coalesce_threshold,
        "use_sparse": config.use_sparse
    })
}

#[tauri::command]
pub async fn refresh_download_url(state: State<'_, AppState>, app_handle: tauri::AppHandle, id: String, new_url: String) -> Result<(), String> {
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
pub async fn install_plugin(app_handle: tauri::AppHandle, url: String, filename: Option<String>) -> Result<String, String> {

    plugin_vm::updater::install_plugin_from_url(&app_handle, url, filename).await
}

#[tauri::command]
pub async fn move_download_item(id: String, direction: String) -> Result<(), String> {
    persistence::move_download(&id, &direction)
}

#[tauri::command]
pub fn set_chaos_config(latency_ms: u64, error_rate: u64, enabled: bool) {
    crate::network::chaos::GLOBAL_CHAOS.update(enabled, latency_ms, error_rate);
}

#[tauri::command]
pub fn get_chaos_config() -> serde_json::Value {
    // Return simple JSON
    serde_json::json!({
        "enabled": crate::network::chaos::GLOBAL_CHAOS.enabled.load(std::sync::atomic::Ordering::Relaxed),
        "latency_ms": crate::network::chaos::GLOBAL_CHAOS.latency_ms.load(std::sync::atomic::Ordering::Relaxed),
        "error_rate": crate::network::chaos::GLOBAL_CHAOS.error_rate_percent.load(std::sync::atomic::Ordering::Relaxed)
    })
}

#[tauri::command]
pub async fn download_as_warc(url: String, save_path: String) -> Result<String, String> {
    crate::warc_archiver::download_as_warc(url, std::path::PathBuf::from(save_path)).await
}

#[tauri::command]
pub fn run_in_sandbox(path: String) -> Result<String, String> {
    crate::sandbox::run_in_sandbox(path)
}

#[tauri::command]
pub async fn notarize_file(path: String) -> Result<serde_json::Value, String> {
    crate::notarize::notarize_file(path).await
}

#[tauri::command]
pub async fn verify_notarization(path: String) -> Result<serde_json::Value, String> {
    crate::notarize::verify_notarization(path).await
}

#[tauri::command]
pub async fn find_mirrors(path: String) -> Result<serde_json::Value, String> {
    crate::mirror_hunter::find_mirrors(path).await
}

#[tauri::command]
pub fn list_usb_drives() -> Result<Vec<crate::usb_flasher::UsbDrive>, String> {
    crate::usb_flasher::list_usb_drives()
}

#[tauri::command]
pub async fn flash_to_usb(iso_path: String, drive_number: u32) -> Result<String, String> {
    crate::usb_flasher::flash_to_usb(iso_path, drive_number).await
}

#[tauri::command]
pub async fn replay_request(
    url: String, method: String,
    headers: Option<std::collections::HashMap<String, String>>,
    body: Option<String>
) -> Result<crate::api_replay::ReplayResult, String> {
    crate::api_replay::replay_request(url, method, headers, body).await
}

#[tauri::command]
pub async fn fuzz_url(url: String) -> Result<serde_json::Value, String> {
    let result = crate::api_replay::fuzz_url(url).await?;
    serde_json::to_value(result).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn validate_c2pa(path: String) -> Result<serde_json::Value, String> {
    crate::c2pa_validator::validate_c2pa(path).await
}

#[tauri::command]
pub async fn arbitrage_download(urls: Vec<String>) -> Result<serde_json::Value, String> {
    let results = crate::bandwidth_arb::arbitrage_probe(urls).await?;
    serde_json::to_value(results).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn stego_hide(image_path: String, secret_data: String) -> Result<serde_json::Value, String> {
    crate::stego_vault::stego_hide(image_path, secret_data).await
}

#[tauri::command]
pub async fn stego_extract(image_path: String) -> Result<serde_json::Value, String> {
    crate::stego_vault::stego_extract(image_path).await
}

#[tauri::command]
pub fn launch_tui_dashboard() -> Result<String, String> {
    crate::tui_dashboard::launch_tui_dashboard()
}

#[tauri::command]
pub async fn auto_extract_archive(path: String, destination: Option<String>) -> Result<serde_json::Value, String> {
    crate::auto_extract::extract_archive(path, destination).await
}

#[tauri::command]
pub async fn download_ipfs(cid: String, save_path: String) -> Result<serde_json::Value, String> {
    crate::ipfs_gateway::download_ipfs(cid, save_path).await
}

#[tauri::command]
pub async fn query_file(path: String, sql: String) -> Result<serde_json::Value, String> {
    crate::sql_query::query_file(path, sql).await
}

#[tauri::command]
pub async fn discover_dlna() -> Result<Vec<crate::dlna_cast::DlnaDevice>, String> {
    crate::dlna_cast::discover_dlna().await
}

#[tauri::command]
pub async fn cast_to_dlna(file_path: String, device_location: String) -> Result<String, String> {
    crate::dlna_cast::cast_to_dlna(file_path, device_location).await
}

#[tauri::command]
pub fn set_download_priority(id: String, level: String) -> Result<String, String> {
    crate::qos_manager::set_download_priority(id, level)
}

#[tauri::command]
pub fn get_qos_stats() -> Result<crate::qos_manager::QosStats, String> {
    crate::qos_manager::get_qos_stats()
}

#[tauri::command]
pub async fn optimize_mods(paths: Vec<String>) -> Result<serde_json::Value, String> {
    let result = crate::mod_optimizer::optimize_mods(paths).await?;
    serde_json::to_value(result).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn rclone_list_remotes() -> Result<Vec<crate::rclone_bridge::RcloneRemote>, String> {
    crate::rclone_bridge::rclone_list_remotes()
}

#[tauri::command]
pub fn rclone_transfer(source: String, destination: String) -> Result<String, String> {
    crate::rclone_bridge::rclone_transfer(source, destination)
}

#[tauri::command]
pub async fn generate_subtitles(video_path: String) -> Result<serde_json::Value, String> {
    crate::subtitle_gen::generate_subtitles(video_path).await
}

#[tauri::command]
pub fn mount_drive(path: String, letter: String) -> Result<String, String> {
    crate::virtual_drive::mount_drive(path, letter)
}

#[tauri::command]
pub fn unmount_drive(letter: String) -> Result<String, String> {
    crate::virtual_drive::unmount_drive(letter)
}

#[tauri::command]
pub fn list_virtual_drives() -> Result<Vec<crate::virtual_drive::MountedDrive>, String> {
    crate::virtual_drive::list_virtual_drives()
}

#[tauri::command]
pub fn set_geofence_rule(url_pattern: String, region: String, proxy_type: String, proxy_address: String) -> Result<String, String> {
    crate::geofence::set_geofence_rule(url_pattern, region, proxy_type, proxy_address)
}

#[tauri::command]
pub fn get_geofence_rules() -> Result<Vec<crate::geofence::GeofenceRule>, String> {
    crate::geofence::get_geofence_rules()
}

#[tauri::command]
pub async fn get_all_plugins(
    pm: State<'_, std::sync::Arc<crate::plugin_vm::manager::PluginManager>>
) -> Result<Vec<crate::plugin_vm::lua_host::PluginMetadata>, String> {
    Ok(pm.get_plugins_list())
}

#[tauri::command]
pub async fn reload_plugins(
    pm: State<'_, std::sync::Arc<crate::plugin_vm::manager::PluginManager>>
) -> Result<(), String> {
    pm.load_plugins().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_plugin_source(filename: String) -> Result<String, String> {
    let mut path = std::env::current_dir().unwrap_or_default().join("plugins");
    path.push(format!("{}.lua", filename)); // Append extension if missing? Assuming filename is without ext?
    // Start with safe check
    if !path.exists() {
        return Err("Plugin file not found".to_string());
    }
    std::fs::read_to_string(path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_plugin_source(filename: String, content: String) -> Result<(), String> {
    let mut path = std::env::current_dir().unwrap_or_default().join("plugins");
    if !path.exists() {
        std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    }
    path.push(format!("{}.lua", filename));
    std::fs::write(path, content).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_plugin(filename: String) -> Result<(), String> {
    let mut path = std::env::current_dir().unwrap_or_default().join("plugins");
    path.push(format!("{}.lua", filename));
    if path.exists() {
        std::fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn get_audio_enabled() -> bool {
    audio_events::AUDIO_PLAYER.is_enabled().await
}

#[tauri::command]
pub async fn set_audio_enabled(enabled: bool) -> Result<(), String> {
    audio_events::AUDIO_PLAYER.set_enabled(enabled).await;
    Ok(())
}

#[tauri::command]
pub async fn get_audio_volume() -> f32 {
    audio_events::AUDIO_PLAYER.get_volume().await
}

#[tauri::command]
pub async fn set_audio_volume(volume: f32) -> Result<(), String> {
    audio_events::AUDIO_PLAYER.set_volume(volume).await;
    Ok(())
}

#[tauri::command]
pub async fn play_test_sound(sound_type: String) -> Result<(), String> {
    let event = match sound_type.as_str() {
        "success" => audio_events::SoundEvent::DownloadComplete,
        "error" => audio_events::SoundEvent::DownloadError,
        "start" => audio_events::SoundEvent::DownloadStart,
        _ => return Err(format!("Unknown sound type: {}", sound_type)),
    };
    
    audio_events::AUDIO_PLAYER.play(event).await;
    Ok(())
}

#[tauri::command]
pub async fn get_webhooks() -> Result<Vec<webhooks::WebhookConfig>, String> {
    let settings = settings::load_settings();
    Ok(settings.webhooks.unwrap_or_default())
}

#[tauri::command]
pub async fn add_webhook(config: webhooks::WebhookConfig) -> Result<(), String> {
    let mut settings = settings::load_settings();
    let mut webhooks = settings.webhooks.unwrap_or_default();
    webhooks.push(config);
    settings.webhooks = Some(webhooks);
    settings::save_settings(&settings)
}

#[tauri::command]
pub async fn update_webhook(id: String, config: webhooks::WebhookConfig) -> Result<(), String> {
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
pub async fn delete_webhook(id: String) -> Result<(), String> {
    let mut settings = settings::load_settings();
    let mut webhooks = settings.webhooks.unwrap_or_default();
    webhooks.retain(|w| w.id != id);
    settings.webhooks = Some(webhooks);
    settings::save_settings(&settings)
}

#[tauri::command]
pub async fn test_webhook(id: String) -> Result<(), String> {
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

#[tauri::command]
pub async fn detect_archive(path: String) -> Option<archive_manager::ArchiveInfo> {
    archive_manager::ArchiveManager::detect_archive(&path)
}

#[tauri::command]
pub async fn extract_archive(archive_path: String, dest_dir: Option<String>) -> Result<String, String> {
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
pub async fn cleanup_archive(archive_path: String) -> Result<(), String> {
    archive_manager::ArchiveManager::cleanup_archive(&archive_path)
}

#[tauri::command]
pub fn check_unrar_available() -> bool {
    archive_manager::ArchiveManager::check_unrar_available()
}

#[tauri::command]
pub async fn create_p2p_share(
    download_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<network::p2p::P2PShareSession, String> {
    let p2p = state.p2p_node.clone();
    p2p.create_share_session(download_id).await
}

#[tauri::command]
pub async fn join_p2p_share(
    code: String,
    peer_addr: String,
    state: tauri::State<'_, AppState>,
) -> Result<network::p2p::P2PShareSession, String> {
    let p2p = state.p2p_node.clone();
    p2p.join_share_session(code, peer_addr).await
}

#[tauri::command]
pub fn list_p2p_sessions(state: tauri::State<'_, AppState>) -> Vec<network::p2p::P2PShareSession> {
    state.p2p_node.list_sessions()
}

#[tauri::command]
pub fn close_p2p_session(session_id: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.p2p_node.close_session(&session_id)
}

#[tauri::command]
pub fn get_p2p_stats(state: tauri::State<'_, AppState>) -> network::p2p::P2PStats {
    state.p2p_node.get_stats()
}

#[tauri::command]
pub fn set_p2p_upload_limit(kbps: u64, state: tauri::State<'_, AppState>) {
    state.p2p_node.set_upload_limit(kbps);
}

#[tauri::command]
pub fn get_p2p_upload_limit(state: tauri::State<'_, AppState>) -> u64 {
    state.p2p_node.get_upload_limit()
}

#[tauri::command]
pub async fn set_custom_sound_path(event_type: String, path: String) -> Result<(), String> {
    let event = match event_type.as_str() {
        "start" => audio_events::SoundEvent::DownloadStart,
        "complete" => audio_events::SoundEvent::DownloadComplete,
        "error" => audio_events::SoundEvent::DownloadError,
        _ => return Err(format!("Unknown sound event: {}", event_type)),
    };
    
    // Validate file exists
    if !std::path::Path::new(&path).exists() {
        return Err("Sound file does not exist".to_string());
    }
    
    audio_events::AUDIO_PLAYER.set_custom_sound(event, std::path::PathBuf::from(&path)).await;
    
    // Save to settings
    let mut settings = settings::load_settings();
    match event_type.as_str() {
        "start" => settings.custom_sound_start = Some(path),
        "complete" => settings.custom_sound_complete = Some(path),
        "error" => settings.custom_sound_error = Some(path),
        _ => {}
    }
    settings::save_settings(&settings)?;
    
    Ok(())
}

#[tauri::command]
pub async fn clear_custom_sound_path(event_type: String) -> Result<(), String> {
    let event = match event_type.as_str() {
        "start" => audio_events::SoundEvent::DownloadStart,
        "complete" => audio_events::SoundEvent::DownloadComplete,
        "error" => audio_events::SoundEvent::DownloadError,
        _ => return Err(format!("Unknown sound event: {}", event_type)),
    };
    
    audio_events::AUDIO_PLAYER.clear_custom_sound(event).await;
    
    // Save to settings
    let mut settings = settings::load_settings();
    match event_type.as_str() {
        "start" => settings.custom_sound_start = None,
        "complete" => settings.custom_sound_complete = None,
        "error" => settings.custom_sound_error = None,
        _ => {}
    }
    settings::save_settings(&settings)?;
    
    Ok(())
}

#[tauri::command]
pub async fn get_custom_sound_paths() -> std::collections::HashMap<String, String> {
    audio_events::AUDIO_PLAYER.get_custom_sounds().await
}

#[tauri::command]
pub fn scrub_metadata(path: String) -> Result<metadata_scrubber::ScrubResult, String> {
    metadata_scrubber::scrub_file(&path)
}

#[tauri::command]
pub fn get_file_metadata(path: String) -> Result<metadata_scrubber::MetadataInfo, String> {
    metadata_scrubber::get_metadata_info(&path)
}

#[tauri::command]
pub async fn start_ephemeral_share(path: String, timeout_mins: Option<u64>) -> Result<ephemeral_server::EphemeralShare, String> {
    let timeout = timeout_mins.unwrap_or(60); // Default 1 hour
    ephemeral_server::EPHEMERAL_MANAGER.start_share(path, timeout).await
}

#[tauri::command]
pub fn stop_ephemeral_share(id: String) -> Result<(), String> {
    ephemeral_server::EPHEMERAL_MANAGER.stop_share(&id)
}

#[tauri::command]
pub fn list_ephemeral_shares() -> Vec<ephemeral_server::EphemeralShare> {
    ephemeral_server::EPHEMERAL_MANAGER.list_shares()
}

#[tauri::command]
pub async fn check_wayback_availability(url: String) -> Result<Option<wayback::WaybackSnapshot>, String> {
    wayback::check_wayback(&url).await
}

#[tauri::command]
pub fn get_wayback_url(wayback_url: String) -> String {
    wayback::get_wayback_download_url(&wayback_url)
}

#[tauri::command]
pub fn upscale_image(path: String) -> Result<crate::ai::upscale::UpscaleResult, String> {
    crate::ai::upscale::upscale_image(&path)
}

#[tauri::command]
pub fn set_app_firewall_rule(exe_path: String, blocked: bool) -> Result<String, String> {
    crate::network::wfp::set_app_firewall_rule(&exe_path, blocked)
}

#[tauri::command]
pub async fn fetch_with_ja3(url: String, browser: String) -> Result<String, String> {
    crate::network::tls_ja3::fetch_with_ja3(&url, &browser).await
}
