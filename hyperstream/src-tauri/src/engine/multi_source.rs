//! Multi-Source / Mirror Download Engine
//!
//! Downloads a single file from **multiple mirrored URLs** simultaneously.
//! Each worker thread is assigned the fastest available mirror; if a mirror
//! fails, the worker transparently retries on the next-best mirror.
//!
//! # Architecture
//!
//! ```text
//!  ┌──────────┐   probe   ┌──────────────┐
//!  │  Mirror  │ ────────► │  MirrorPool   │◄─── speed / error stats
//!  │  URLs    │           │  (ranked)     │
//!  └──────────┘           └──────┬───────┘
//!                                │  pick_best()
//!                    ┌───────────┼───────────┐
//!                    ▼           ▼           ▼
//!              ┌─────────┐ ┌─────────┐ ┌─────────┐
//!              │ Worker 0│ │ Worker 1│ │ Worker N│   ← each has fallback mirrors
//!              │ Seg 0-1 │ │ Seg 2-3 │ │ Seg N-1 │
//!              └─────────┘ └─────────┘ └─────────┘
//!                    │           │           │
//!                    └───────────┼───────────┘
//!                                ▼
//!                          ┌──────────┐
//!                          │DiskWriter│
//!                          └──────────┘
//! ```
//!
//! Integration: called from `start_download_impl` when mirrors are provided,
//! or from the `start_multi_source_download` Tauri command.

use std::sync::{Arc, Mutex as StdMutex};
use std::sync::atomic::{AtomicU64, Ordering};
use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use tauri::{Emitter, Manager};
use tokio::sync::broadcast;

// ────────────────────────────────────────────────────────────────────────────
// Mirror statistics & pool
// ────────────────────────────────────────────────────────────────────────────

/// Performance and reliability stats for a single mirror.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirrorStats {
    pub url: String,
    /// Human-readable source label (e.g. "Primary", "Internet Archive").
    pub source: String,
    /// Average bytes-per-second measured over the last successful request.
    pub avg_speed_bps: u64,
    /// Cumulative successful bytes from this mirror.
    pub total_bytes: u64,
    /// Number of requests that completed successfully.
    pub success_count: u32,
    /// Number of requests that failed (timeout, HTTP error, etc.).
    pub error_count: u32,
    /// Whether the mirror supports HTTP range requests.
    pub supports_range: bool,
    /// Whether we've probed this mirror yet.
    pub probed: bool,
    /// Milliseconds for initial HEAD/GET response.
    pub latency_ms: u64,
    /// If true, the mirror is temporarily disabled (too many errors).
    pub disabled: bool,
}

impl MirrorStats {
    pub fn new(url: String, source: String) -> Self {
        Self {
            url,
            source,
            avg_speed_bps: 0,
            total_bytes: 0,
            success_count: 0,
            error_count: 0,
            supports_range: false,
            probed: false,
            latency_ms: u64::MAX,
            disabled: false,
        }
    }

    /// Composite score: higher is better. Factors in speed, latency, reliability.
    pub fn score(&self) -> f64 {
        if self.disabled {
            return 0.0;
        }
        if !self.probed {
            return 0.1; // Unknown mirrors get a low but non-zero score
        }
        let speed_factor = self.avg_speed_bps as f64;
        let latency_penalty = 1.0 / (1.0 + (self.latency_ms as f64 / 1000.0));
        let reliability = if self.success_count + self.error_count > 0 {
            self.success_count as f64 / (self.success_count + self.error_count) as f64
        } else {
            0.5
        };
        speed_factor * latency_penalty * reliability
    }

    /// Record a successful chunk transfer.
    pub fn record_success(&mut self, bytes: u64, duration_ms: u64) {
        self.success_count += 1;
        self.total_bytes += bytes;
        if duration_ms > 0 {
            let speed = (bytes as f64 / duration_ms as f64 * 1000.0) as u64;
            // Exponential moving average (α = 0.3)
            if self.avg_speed_bps == 0 {
                self.avg_speed_bps = speed;
            } else {
                self.avg_speed_bps = ((self.avg_speed_bps as f64 * 0.7) + (speed as f64 * 0.3)) as u64;
            }
        }
    }

    /// Record a failed request.
    pub fn record_error(&mut self) {
        self.error_count += 1;
        // Disable after 5 consecutive errors without any success in between
        if self.error_count >= 5 && self.success_count == 0 {
            self.disabled = true;
        }
        // Re-disable if error ratio exceeds 80% after enough attempts
        if self.error_count + self.success_count > 10
            && (self.error_count as f64 / (self.error_count + self.success_count) as f64) > 0.8
        {
            self.disabled = true;
        }
    }
}

/// Thread-safe mirror pool that ranks mirrors and provides the best one.
#[derive(Clone)]
pub struct MirrorPool {
    mirrors: Arc<StdMutex<Vec<MirrorStats>>>,
}

impl MirrorPool {
    /// Create a pool from a primary URL and optional mirror URLs.
    pub fn new(primary_url: &str, mirror_urls: &[(String, String)]) -> Self {
        let mut mirrors = vec![MirrorStats::new(primary_url.to_string(), "Primary".to_string())];
        for (url, source) in mirror_urls {
            if url != primary_url {
                mirrors.push(MirrorStats::new(url.clone(), source.clone()));
            }
        }
        Self {
            mirrors: Arc::new(StdMutex::new(mirrors)),
        }
    }

    /// Probe all mirrors concurrently: send a small range request and measure
    /// latency + range support.  Non-responding mirrors are marked but not removed.
    pub async fn probe_all(&self, client: &rquest::Client) {
        let urls: Vec<String> = {
            let m = self.mirrors.lock().unwrap_or_else(|e| e.into_inner());
            m.iter().map(|ms| ms.url.clone()).collect()
        };

        let mut tasks = Vec::new();
        for url in urls {
            let client = client.clone();
            let pool = self.clone();
            tasks.push(tokio::spawn(async move {
                let start = std::time::Instant::now();
                // Request first 1KB to test range support + measure latency
                let result = tokio::time::timeout(
                    std::time::Duration::from_secs(10),
                    client
                        .get(&url)
                        .header("Range", "bytes=0-1023")
                        .send(),
                ).await;
                let elapsed = start.elapsed().as_millis() as u64;

                match result {
                    Ok(Ok(resp)) => {
                        let supports_range = resp.status() == rquest::StatusCode::PARTIAL_CONTENT;
                        let speed_estimate = if elapsed > 0 { 1024 * 1000 / elapsed } else { 0 };
                        let mut m = pool.mirrors.lock().unwrap_or_else(|e| e.into_inner());
                        if let Some(ms) = m.iter_mut().find(|ms| ms.url == url) {
                            ms.probed = true;
                            ms.latency_ms = elapsed;
                            ms.supports_range = supports_range;
                            ms.avg_speed_bps = speed_estimate;
                        }
                    }
                    Ok(Err(_)) | Err(_) => {
                        let mut m = pool.mirrors.lock().unwrap_or_else(|e| e.into_inner());
                        if let Some(ms) = m.iter_mut().find(|ms| ms.url == url) {
                            ms.probed = true;
                            ms.latency_ms = u64::MAX;
                            ms.record_error();
                        }
                    }
                }
            }));
        }
        // Wait for all probes
        for t in tasks {
            let _ = t.await;
        }
    }

    /// Get the best mirror URL for a worker. Falls back to primary if all else fails.
    /// Each call may return a different mirror for load distribution.
    pub fn pick_best(&self) -> String {
        let m = self.mirrors.lock().unwrap_or_else(|e| e.into_inner());
        let mut ranked: Vec<&MirrorStats> = m.iter().filter(|ms| !ms.disabled).collect();
        ranked.sort_by(|a, b| b.score().partial_cmp(&a.score()).unwrap_or(std::cmp::Ordering::Equal));
        ranked.first().map(|ms| ms.url.clone()).unwrap_or_else(|| {
            // Absolute fallback: return first mirror regardless of disabled state
            m.first().map(|ms| ms.url.clone()).unwrap_or_default()
        })
    }

    /// Pick the best mirror that isn't `exclude_url`. Used for failover.
    pub fn pick_fallback(&self, exclude_url: &str) -> Option<String> {
        let m = self.mirrors.lock().unwrap_or_else(|e| e.into_inner());
        let mut ranked: Vec<&MirrorStats> = m.iter()
            .filter(|ms| !ms.disabled && ms.url != exclude_url)
            .collect();
        ranked.sort_by(|a, b| b.score().partial_cmp(&a.score()).unwrap_or(std::cmp::Ordering::Equal));
        ranked.first().map(|ms| ms.url.clone())
    }

    /// Record success for a mirror.
    pub fn record_success(&self, url: &str, bytes: u64, duration_ms: u64) {
        let mut m = self.mirrors.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ms) = m.iter_mut().find(|ms| ms.url == url) {
            ms.record_success(bytes, duration_ms);
        }
    }

    /// Record error for a mirror.
    pub fn record_error(&self, url: &str) {
        let mut m = self.mirrors.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ms) = m.iter_mut().find(|ms| ms.url == url) {
            ms.record_error();
        }
    }

    /// Get a snapshot of all mirror stats (for frontend display).
    pub fn get_stats(&self) -> Vec<MirrorStats> {
        self.mirrors.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Number of usable (non-disabled) mirrors.
    pub fn usable_count(&self) -> usize {
        self.mirrors.lock().unwrap_or_else(|e| e.into_inner()).iter().filter(|ms| !ms.disabled).count()
    }

    /// Add a new mirror dynamically (e.g. discovered during download).
    pub fn add_mirror(&self, url: String, source: String) {
        let mut m = self.mirrors.lock().unwrap_or_else(|e| e.into_inner());
        if !m.iter().any(|ms| ms.url == url) {
            m.push(MirrorStats::new(url, source));
        }
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Multi-source download implementation
// ────────────────────────────────────────────────────────────────────────────

/// Start a multi-source download.  This is the main entry point, designed to
/// be called from `lib.rs` as a Tauri command.
///
/// `mirrors` is a list of `(url, source_label)` pairs *in addition* to the
/// primary URL.  Workers will probe all mirrors and distribute segments
/// across the fastest ones.
pub async fn start_multi_source_download(
    app: &tauri::AppHandle,
    state: &crate::core_state::AppState,
    id: String,
    primary_url: String,
    mirrors: Vec<(String, String)>,
    path: String,
    custom_headers: Option<HashMap<String, String>>,
) -> Result<(), String> {
    use crate::core_state::*;
    use crate::downloader::disk::*;
    use crate::engine::session::{extract_filename, resolve_filename_collision};
    use crate::{persistence, settings};
    use futures::StreamExt;
    use std::sync::mpsc;
    use std::thread;
    use tauri::Emitter;

    println!("[multi-source] Starting download {} with {} mirrors", id, mirrors.len() + 1);
    crate::media::sounds::play_startup();

    let settings = settings::load_settings();
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

    // 1. Build mirror pool and probe all mirrors
    let pool = MirrorPool::new(&primary_url, &mirrors);
    pool.probe_all(&client).await;
    println!(
        "[multi-source] Probe complete: {}/{} mirrors usable",
        pool.usable_count(),
        mirrors.len() + 1
    );

    // 2. Determine file size from best mirror
    let best_url = pool.pick_best();
    let probe =
        crate::downloader::initialization::determine_total_size(&client, &best_url).await?;
    let total_size = probe.total_size;
    let etag = probe.etag;
    let md5 = probe.md5;

    if total_size == 0 {
        return Err("Cannot determine file size for multi-source download".to_string());
    }

    // 3. File path with collision avoidance + category logic
    let final_path = if settings.use_category_folders {
        let path_obj = std::path::Path::new(&path);
        let filename = path_obj
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let mut new_path_buf = path_obj.to_path_buf();

        for rule in &settings.category_rules {
            if let Ok(re) = regex::Regex::new(&rule.pattern) {
                if re.is_match(&filename) {
                    let category_path = if std::path::Path::new(&rule.path).is_absolute() {
                        std::path::PathBuf::from(&rule.path)
                    } else {
                        std::path::PathBuf::from(&settings.download_dir).join(&rule.path)
                    };
                    std::fs::create_dir_all(&category_path).ok();
                    if let (Ok(canon_dl), Ok(canon_cat)) = (
                        dunce::canonicalize(&settings.download_dir),
                        dunce::canonicalize(&category_path),
                    ) {
                        if !canon_cat.starts_with(&canon_dl) {
                            continue; // Category escapes download dir
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
    let path = resolve_filename_collision(&final_path);

    // 4. Setup file
    let file = crate::downloader::initialization::setup_file(&path, 0, total_size)?;
    let file_mutex = file;

    // 5. Initialize segment manager
    let saved = {
        let downloads = persistence::load_downloads().unwrap_or_default();
        downloads.into_iter().find(|d| d.id == id)
    };
    let manager = crate::downloader::initialization::setup_manager(
        total_size,
        saved.as_ref(),
        saved.as_ref().map(|s| s.downloaded_bytes).unwrap_or(0),
        settings.segments,
    );
    let resume_from = saved.as_ref().map(|s| s.downloaded_bytes).unwrap_or(0);
    let downloaded_total = Arc::new(AtomicU64::new(resume_from));

    // 6. Stop signal
    let (stop_tx, _) = broadcast::channel(1);

    // 7. Store session
    {
        let mut downloads = state.downloads.lock().unwrap_or_else(|e| e.into_inner());
        downloads.insert(
            id.clone(),
            DownloadSession {
                manager: manager.clone(),
                stop_tx: stop_tx.clone(),
                url: primary_url.clone(),
                path: path.clone(),
                file_writer: file_mutex.clone(),
            },
        );
    }

    // 8. Disk writer
    let (tx, rx) = mpsc::channel::<WriteRequest>();
    let file_writer_clone = file_mutex.clone();
    let disk_io_error = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let disk_io_error_writer = disk_io_error.clone();
    thread::spawn(move || {
        let mut writer = DiskWriter::new(file_writer_clone, rx);
        let writer_flag = writer.io_error_flag();
        let error_bridge = disk_io_error_writer.clone();
        let bridge_flag = writer_flag.clone();
        std::thread::spawn(move || {
            while !error_bridge.load(Ordering::Relaxed) {
                if bridge_flag.load(Ordering::Relaxed) {
                    error_bridge.store(true, Ordering::Release);
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        });
        writer.run();
        if writer_flag.load(Ordering::Acquire) {
            disk_io_error_writer.store(true, Ordering::Release);
        }
    });

    // 9. Monitor task (30fps progress emission)
    {
        let manager_monitor = manager.clone();
        let downloaded_monitor = downloaded_total.clone();
        let app_monitor = app.clone();
        let id_monitor = id.clone();
        let url_monitor = primary_url.clone();
        let path_monitor = path.clone();
        let etag_monitor = etag.clone();
        let md5_monitor = md5.clone();
        let mut stop_rx_monitor = stop_tx.subscribe();
        let stop_tx_monitor = stop_tx.clone();
        let disk_io_error_monitor = disk_io_error.clone();
        let pool_monitor = pool.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(33));
            let monitor_start = std::time::Instant::now();
            let monitor_start_iso = chrono::Local::now().to_rfc3339();
            loop {
                tokio::select! {
                    _ = stop_rx_monitor.recv() => break,
                    _ = interval.tick() => {
                        // Check disk I/O errors
                        if disk_io_error_monitor.load(Ordering::Acquire) {
                            eprintln!("[multi-source][{}] Disk I/O error, aborting", id_monitor);
                            let _ = app_monitor.emit("download_error", serde_json::json!({
                                "id": id_monitor,
                                "error": "Disk write error during multi-source download."
                            }));
                            let _ = stop_tx_monitor.send(());
                            {
                                let mut queue = crate::queue_manager::DOWNLOAD_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
                                queue.mark_finished(&id_monitor);
                            }
                            if let Some(app_state) = app_monitor.try_state::<crate::core_state::AppState>() {
                                let mut downloads = app_state.downloads.lock().unwrap_or_else(|e| e.into_inner());
                                downloads.remove(&id_monitor);
                            }
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
                                source_type: Some("multi-source".to_string()),
                            });
                            break;
                        }

                        let d = downloaded_monitor.load(Ordering::Relaxed);
                        let segments = manager_monitor.lock().unwrap_or_else(|e| e.into_inner()).get_segments_snapshot();
                        let slim_segments: Vec<SlimSegment> = segments.iter().map(|s| (
                            s.id, s.start_byte, s.end_byte, s.downloaded_cursor, s.state as u8, s.speed_bps
                        )).collect();

                        let payload = Payload {
                            id: id_monitor.clone(),
                            downloaded: d,
                            total: total_size,
                            segments: slim_segments,
                        };
                        let _ = app_monitor.emit("download_progress", payload.clone());
                        let _ = crate::http_server::get_event_sender().send(
                            serde_json::to_value(&payload).unwrap_or(serde_json::json!(null))
                        );

                        // Also emit mirror stats periodically (every ~1s = 30 ticks)
                        // We use a simple counter trick via the downloaded bytes
                        if d % (1024 * 1024) < 32768 {
                            let stats = pool_monitor.get_stats();
                            let _ = app_monitor.emit("mirror_stats", serde_json::json!({
                                "id": id_monitor,
                                "mirrors": stats,
                            }));
                        }

                        // Completion check
                        if total_size > 0 && d >= total_size {
                            crate::media::sounds::play_complete();
                            crate::cas_manager::register_cas(
                                etag_monitor.as_deref(),
                                md5_monitor.as_deref(),
                                &path_monitor,
                            );

                            // Persist completion
                            let saved = persistence::SavedDownload {
                                id: id_monitor.clone(),
                                url: url_monitor.clone(),
                                path: path_monitor.clone(),
                                filename: extract_filename(&path_monitor).to_string(),
                                total_size,
                                downloaded_bytes: total_size,
                                status: "Complete".to_string(),
                                segments: None,
                                last_active: Some(chrono::Utc::now().to_rfc3339()),
                                error_message: None,
                            };
                            let _ = persistence::upsert_download(saved);
                            // Record in download history
                            let elapsed = monitor_start.elapsed();
                            let avg_speed = if elapsed.as_secs() > 0 { total_size / elapsed.as_secs() } else { 0 };
                            let seg_count = manager_monitor.lock().unwrap_or_else(|e| e.into_inner())
                                .segments.read().unwrap_or_else(|e| e.into_inner()).len() as u32;
                            let mirror_count = pool_monitor.get_stats().len();
                            let _ = crate::download_history::record(crate::download_history::HistoryEntry {
                                id: id_monitor.clone(),
                                url: url_monitor.clone(),
                                path: path_monitor.clone(),
                                filename: extract_filename(&path_monitor).to_string(),
                                total_size,
                                downloaded_bytes: total_size,
                                status: "Complete".to_string(),
                                started_at: monitor_start_iso.clone(),
                                finished_at: chrono::Local::now().to_rfc3339(),
                                avg_speed_bps: avg_speed,
                                duration_secs: elapsed.as_secs(),
                                segments_used: seg_count,
                                error_message: None,
                                source_type: Some(format!("multi-source({})", mirror_count)),
                            });
                            // Auto-categorize completed file
                            {
                                let fname = extract_filename(&path_monitor).to_string();
                                let cat_result = crate::file_categorizer::categorize(&fname);
                                let _ = app_monitor.emit("file_categorized", serde_json::json!({
                                    "id": id_monitor,
                                    "filename": fname,
                                    "category": cat_result.category_name,
                                    "icon": cat_result.icon,
                                    "color": cat_result.color,
                                    "should_move": cat_result.should_move,
                                    "target_dir": cat_result.target_dir,
                                }));
                            }
                            let _ = stop_tx_monitor.send(());
                            {
                                let mut queue = crate::queue_manager::DOWNLOAD_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
                                queue.mark_finished(&id_monitor);
                            }
                            if let Some(app_state) = app_monitor.try_state::<crate::core_state::AppState>() {
                                let mut downloads = app_state.downloads.lock().unwrap_or_else(|e| e.into_inner());
                                downloads.remove(&id_monitor);
                            }
                            break;
                        }
                    }
                }
            }
        });
    }

    // 10. Spawn worker threads — each worker picks the best mirror
    let segments_count = manager
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .segments
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .len();

    for i in 0..segments_count {
        let manager_clone = manager.clone();
        let tx_clone = tx.clone();
        let client_clone = client.clone();
        let downloaded_clone = downloaded_total.clone();
        let cm_clone = state.connection_manager.clone();
        let mut stop_rx = stop_tx.subscribe();
        let stop_tx_clone = stop_tx.clone();
        let id_worker = id.clone();
        let path_worker = path.clone();
        let url_worker = primary_url.clone();
        let app_handle_clone = app.clone();
        let total_size_worker = total_size;
        let disk_io_error_worker = disk_io_error.clone();
        let pool_worker = pool.clone();

        tokio::spawn(async move {
            let (start, end, seg_id) = {
                let m = manager_clone.lock().unwrap_or_else(|e| e.into_inner());
                let mut segs = m.segments.write().unwrap_or_else(|e| e.into_inner());
                let seg = &mut segs[i];
                seg.state = crate::downloader::structures::SegmentState::Downloading;
                (seg.downloaded_cursor, seg.end_byte, seg.id)
            };

            if end == 0 || start >= end {
                return;
            }

            let mut current_pos = start;
            let mut retry_count: u32 = 0;
            const MAX_RETRIES: u32 = 5;
            let mut bytes_since_cursor_update: u64 = 0;
            const CURSOR_UPDATE_THRESHOLD: u64 = 256 * 1024;

            // Pick initial mirror for this worker
            let mut current_mirror = pool_worker.pick_best();
            let mut chunk_start_time = std::time::Instant::now();
            let mut chunk_bytes: u64 = 0;

            loop {
                // Stop signal check
                if stop_rx.try_recv().is_ok() {
                    let m = manager_clone.lock().unwrap_or_else(|e| e.into_inner());
                    let mut segs = m.segments.write().unwrap_or_else(|e| e.into_inner());
                    if let Some(seg) = segs.iter_mut().find(|s| s.id == seg_id) {
                        seg.downloaded_cursor = current_pos;
                        seg.state = crate::downloader::structures::SegmentState::Paused;
                    }
                    break;
                }

                // Disk I/O error check
                if disk_io_error_worker.load(Ordering::Acquire) {
                    let m = manager_clone.lock().unwrap_or_else(|e| e.into_inner());
                    let mut segs = m.segments.write().unwrap_or_else(|e| e.into_inner());
                    if let Some(seg) = segs.iter_mut().find(|s| s.id == seg_id) {
                        seg.downloaded_cursor = current_pos;
                        seg.state = crate::downloader::structures::SegmentState::Error;
                    }
                    break;
                }

                // Segment complete?
                if current_pos >= end {
                    // Report final chunk stats
                    let elapsed = chunk_start_time.elapsed().as_millis() as u64;
                    if chunk_bytes > 0 {
                        pool_worker.record_success(&current_mirror, chunk_bytes, elapsed);
                    }
                    let m = manager_clone.lock().unwrap_or_else(|e| e.into_inner());
                    let mut segs = m.segments.write().unwrap_or_else(|e| e.into_inner());
                    if let Some(seg) = segs.iter_mut().find(|s| s.id == seg_id) {
                        seg.state = crate::downloader::structures::SegmentState::Complete;
                    }
                    break;
                }

                let range_header = format!("bytes={}-{}", current_pos, end - 1);
                let _permit = cm_clone.acquire(&current_mirror).await.ok();

                // Chaos mode
                if let Err(_e) = crate::network::chaos::check_chaos().await {
                    retry_count += 1;
                    if retry_count <= MAX_RETRIES {
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        continue;
                    }
                }

                let res_future = client_clone
                    .get(&current_mirror)
                    .header("Range", &range_header)
                    .send();

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
                        println!(
                            "[multi-source] Worker seg {} error on {}: {}",
                            seg_id, current_mirror, e
                        );
                        pool_worker.record_error(&current_mirror);

                        // Try to failover to another mirror
                        if let Some(fallback) = pool_worker.pick_fallback(&current_mirror) {
                            println!(
                                "[multi-source] Seg {} failing over from {} → {}",
                                seg_id, current_mirror, fallback
                            );
                            current_mirror = fallback;
                            chunk_start_time = std::time::Instant::now();
                            chunk_bytes = 0;
                            retry_count = 0; // Reset retry count for new mirror
                            continue;
                        }

                        // No fallback available, normal retry on same mirror
                        retry_count += 1;
                        if retry_count > MAX_RETRIES {
                            let m = manager_clone.lock().unwrap_or_else(|e| e.into_inner());
                            let mut segs = m.segments.write().unwrap_or_else(|e| e.into_inner());
                            if let Some(seg) = segs.iter_mut().find(|s| s.id == seg_id) {
                                seg.downloaded_cursor = current_pos;
                                seg.state = crate::downloader::structures::SegmentState::Error;
                            }
                            crate::media::sounds::play_error();
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        continue;
                    }
                };

                // Handle 403/410 — try another mirror before giving up
                if response.status() == rquest::StatusCode::FORBIDDEN
                    || response.status() == rquest::StatusCode::GONE
                {
                    pool_worker.record_error(&current_mirror);
                    if let Some(fallback) = pool_worker.pick_fallback(&current_mirror) {
                        println!(
                            "[multi-source] Seg {} HTTP {} on {}, failing over → {}",
                            seg_id,
                            response.status().as_u16(),
                            current_mirror,
                            fallback
                        );
                        current_mirror = fallback;
                        chunk_start_time = std::time::Instant::now();
                        chunk_bytes = 0;
                        retry_count = 0;
                        continue;
                    }
                    // No fallback — stop all workers (link expired everywhere)
                    let _ = stop_tx_clone.send(());
                    let segments = manager_clone
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .get_segments_snapshot();
                    let total_downloaded: u64 = segments
                        .iter()
                        .map(|s| s.downloaded_cursor.saturating_sub(s.start_byte))
                        .sum();
                    let filename_s = std::path::Path::new(&path_worker)
                        .file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_else(|| "download".to_string());
                    let _ = persistence::upsert_download(persistence::SavedDownload {
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
                    });
                    crate::media::sounds::play_error();
                    return;
                }

                // Handle 429/503 rate limiting
                if response.status() == rquest::StatusCode::TOO_MANY_REQUESTS
                    || response.status() == rquest::StatusCode::SERVICE_UNAVAILABLE
                {
                    // Try another mirror first before waiting
                    pool_worker.record_error(&current_mirror);
                    if let Some(fallback) = pool_worker.pick_fallback(&current_mirror) {
                        println!(
                            "[multi-source] Seg {} rate-limited on {}, switching → {}",
                            seg_id, current_mirror, fallback
                        );
                        current_mirror = fallback;
                        chunk_start_time = std::time::Instant::now();
                        chunk_bytes = 0;
                        continue;
                    }
                    // No fallback — obey Retry-After
                    let wait_time = if let Some(h) = response.headers().get("Retry-After") {
                        if let Ok(s) = h.to_str() {
                            crate::downloader::network::parse_retry_after(s)
                                .unwrap_or(std::time::Duration::from_secs(30))
                        } else {
                            std::time::Duration::from_secs(30)
                        }
                    } else {
                        std::time::Duration::from_secs(30)
                    };
                    tokio::time::sleep(wait_time).await;
                    continue;
                }

                // Stream response body
                let mut stream = response.bytes_stream();
                chunk_start_time = std::time::Instant::now();
                chunk_bytes = 0;

                loop {
                    tokio::select! {
                        _ = stop_rx.recv() => {
                            let m = manager_clone.lock().unwrap_or_else(|e| e.into_inner());
                            let mut segs = m.segments.write().unwrap_or_else(|e| e.into_inner());
                            if let Some(seg) = segs.iter_mut().find(|s| s.id == seg_id) {
                                seg.downloaded_cursor = current_pos;
                                seg.state = crate::downloader::structures::SegmentState::Paused;
                            }
                            return;
                        }
                        item = stream.next() => {
                            match item {
                                Some(Ok(chunk)) => {
                                    let remaining = end.saturating_sub(current_pos) as usize;
                                    let safe_chunk = if chunk.len() > remaining {
                                        &chunk[..remaining]
                                    } else {
                                        &chunk[..]
                                    };
                                    let len = safe_chunk.len() as u64;
                                    if len == 0 { break; }

                                    crate::speed_limiter::GLOBAL_LIMITER.acquire(len).await;

                                    if tx_clone.send(WriteRequest {
                                        offset: current_pos,
                                        data: safe_chunk.to_vec(),
                                        segment_id: seg_id,
                                    }).is_err() {
                                        eprintln!("[multi-source] seg {}: disk writer closed", seg_id);
                                        return;
                                    }
                                    current_pos += len;
                                    chunk_bytes += len;

                                    downloaded_clone.fetch_add(len, Ordering::Relaxed);

                                    bytes_since_cursor_update += len;
                                    if bytes_since_cursor_update >= CURSOR_UPDATE_THRESHOLD {
                                        bytes_since_cursor_update = 0;
                                        let m = manager_clone.lock().unwrap_or_else(|e| e.into_inner());
                                        let mut segs = m.segments.write().unwrap_or_else(|e| e.into_inner());
                                        if let Some(seg) = segs.iter_mut().find(|s| s.id == seg_id) {
                                            seg.downloaded_cursor = current_pos;
                                        }
                                    }

                                    // Report stats periodically (every ~1MB)
                                    if chunk_bytes >= 1024 * 1024 {
                                        let elapsed = chunk_start_time.elapsed().as_millis() as u64;
                                        pool_worker.record_success(&current_mirror, chunk_bytes, elapsed);
                                        chunk_start_time = std::time::Instant::now();
                                        chunk_bytes = 0;
                                    }
                                }
                                Some(Err(_)) => {
                                    // Stream error — record and try again (outer loop retry)
                                    pool_worker.record_error(&current_mirror);
                                    break;
                                }
                                None => {
                                    // End of response stream
                                    let elapsed = chunk_start_time.elapsed().as_millis() as u64;
                                    if chunk_bytes > 0 {
                                        pool_worker.record_success(&current_mirror, chunk_bytes, elapsed);
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        });
    }

    // 11. Periodic save loop
    {
        let manager_save = manager.clone();
        let id_save = id.clone();
        let url_save = primary_url.clone();
        let path_save = path.clone();
        let filename_save = std::path::Path::new(&path)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| "download".to_string());
        let mut stop_rx_save = stop_tx.subscribe();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = stop_rx_save.recv() => break,
                    _ = tokio::time::sleep(tokio::time::Duration::from_secs(5)) => {
                        let segments = manager_save.lock().unwrap_or_else(|e| e.into_inner()).get_segments_snapshot();
                        let total_downloaded: u64 = segments.iter().map(|s| s.downloaded_cursor.saturating_sub(s.start_byte)).sum();
                        let _ = persistence::upsert_download(persistence::SavedDownload {
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
                        });
                    }
                }
            }
        });
    }

    Ok(())
}
