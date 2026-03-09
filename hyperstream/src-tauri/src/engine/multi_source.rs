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

#[derive(Debug, Clone, PartialEq, Eq)]
struct MirrorIdentity {
    total_size: u64,
    etag: Option<String>,
    md5: Option<String>,
}

impl MirrorIdentity {
    fn from_probe(probe: &crate::downloader::initialization::ProbeResult) -> Self {
        Self {
            total_size: probe.total_size,
            etag: probe.etag.clone(),
            md5: probe.md5.clone(),
        }
    }

    fn mismatch_reason(&self, other: &Self) -> Option<String> {
        if self.total_size != other.total_size {
            return Some(format!(
                "content length mismatch (expected {} bytes, got {} bytes)",
                self.total_size, other.total_size
            ));
        }

        if let (Some(expected), Some(actual)) = (self.etag.as_deref(), other.etag.as_deref()) {
            if normalize_etag(expected) != normalize_etag(actual) {
                return Some("ETag mismatch".to_string());
            }
        }

        if let (Some(expected), Some(actual)) = (self.md5.as_deref(), other.md5.as_deref()) {
            if expected.trim() != actual.trim() {
                return Some("Content-MD5 mismatch".to_string());
            }
        }

        None
    }
}

fn normalize_etag(value: &str) -> String {
    let trimmed = value.trim();
    let without_weak_prefix = trimmed
        .strip_prefix("W/")
        .or_else(|| trimmed.strip_prefix("w/"))
        .unwrap_or(trimmed);
    without_weak_prefix.trim_matches('"').to_string()
}

fn parse_content_range_total(header: &str) -> Option<u64> {
    let total = header.split('/').nth(1)?.trim();
    if total == "*" {
        None
    } else {
        total.parse::<u64>().ok().filter(|&size| size > 0)
    }
}

fn response_identity_mismatch(
    expected: &MirrorIdentity,
    response: &rquest::Response,
    requires_range_request: bool,
) -> Option<String> {
    let response_total_size = if response.status() == rquest::StatusCode::PARTIAL_CONTENT {
        response
            .headers()
            .get("content-range")
            .and_then(|value| value.to_str().ok())
            .and_then(parse_content_range_total)
    } else if !requires_range_request {
        response.content_length().or_else(|| {
            response
                .headers()
                .get("content-length")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok())
        })
    } else {
        None
    };

    if let Some(actual_total_size) = response_total_size {
        if actual_total_size != expected.total_size {
            return Some(format!(
                "runtime content length mismatch (expected {} bytes, got {} bytes)",
                expected.total_size, actual_total_size
            ));
        }
    }

    if let (Some(expected_etag), Some(actual_etag)) = (
        expected.etag.as_deref(),
        response
            .headers()
            .get("etag")
            .and_then(|value| value.to_str().ok()),
    ) {
        if normalize_etag(expected_etag) != normalize_etag(actual_etag) {
            return Some("runtime ETag mismatch".to_string());
        }
    }

    if let (Some(expected_md5), Some(actual_md5)) = (
        expected.md5.as_deref(),
        response
            .headers()
            .get("content-md5")
            .and_then(|value| value.to_str().ok()),
    ) {
        if expected_md5.trim() != actual_md5.trim() {
            return Some("runtime Content-MD5 mismatch".to_string());
        }
    }

    None
}

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
    /// If true, the mirror was quarantined because it does not match the canonical file identity.
    pub quarantined: bool,
    /// Human-readable identity validation state.
    pub identity_status: String,
    /// When quarantined, explains why this mirror was excluded.
    pub quarantine_reason: Option<String>,
    /// Marks the mirror that established the canonical file identity for this session.
    pub canonical: bool,
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
            quarantined: false,
            identity_status: "pending".to_string(),
            quarantine_reason: None,
            canonical: false,
        }
    }

    /// Composite score: higher is better. Factors in speed, latency, reliability.
    pub fn score(&self) -> f64 {
        if self.disabled || self.quarantined {
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
        let range_factor = if self.supports_range { 1.0 } else { 0.25 };
        speed_factor * latency_penalty * reliability * range_factor
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
    fn ranked_candidates<'a>(
        mirrors: &'a [MirrorStats],
        require_range: bool,
        exclude_url: Option<&str>,
    ) -> Vec<&'a MirrorStats> {
        let mut ranked: Vec<&MirrorStats> = mirrors
            .iter()
            .filter(|ms| !ms.disabled)
            .filter(|ms| !ms.quarantined)
            .filter(|ms| !require_range || ms.supports_range)
            .filter(|ms| exclude_url.map(|exclude| ms.url != exclude).unwrap_or(true))
            .collect();
        ranked.sort_by(|a, b| b.score().partial_cmp(&a.score()).unwrap_or(std::cmp::Ordering::Equal));
        ranked
    }

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
        self.pick_best_matching(false).unwrap_or_else(|| {
            let m = self.mirrors.lock().unwrap_or_else(|e| e.into_inner());
            m.first().map(|ms| ms.url.clone()).unwrap_or_default()
        })
    }

    /// Pick the best mirror, optionally requiring byte-range support.
    pub fn pick_best_matching(&self, require_range: bool) -> Option<String> {
        let m = self.mirrors.lock().unwrap_or_else(|e| e.into_inner());
        Self::ranked_candidates(&m, require_range, None)
            .first()
            .map(|ms| ms.url.clone())
    }

    /// Pick the best mirror that isn't `exclude_url`. Used for failover.
    pub fn pick_fallback(&self, exclude_url: &str) -> Option<String> {
        self.pick_fallback_matching(exclude_url, false)
    }

    /// Pick the best fallback mirror, optionally requiring byte-range support.
    pub fn pick_fallback_matching(&self, exclude_url: &str, require_range: bool) -> Option<String> {
        let m = self.mirrors.lock().unwrap_or_else(|e| e.into_inner());
        Self::ranked_candidates(&m, require_range, Some(exclude_url))
            .first()
            .map(|ms| ms.url.clone())
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

    /// URLs of mirrors that are still operationally usable and worth validating.
    pub fn usable_urls(&self) -> Vec<String> {
        self.mirrors
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter(|ms| !ms.disabled)
            .map(|ms| ms.url.clone())
            .collect()
    }

    /// Number of usable (non-disabled) mirrors.
    pub fn usable_count(&self) -> usize {
        self.usable_count_matching(false)
    }

    /// Number of usable mirrors, optionally requiring byte-range support.
    pub fn usable_count_matching(&self, require_range: bool) -> usize {
        self.mirrors
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter(|ms| !ms.disabled)
            .filter(|ms| !ms.quarantined)
            .filter(|ms| !require_range || ms.supports_range)
            .count()
    }

    /// Update a mirror's known range support after a deeper probe.
    pub fn set_range_support(&self, url: &str, supports_range: bool) {
        let mut m = self.mirrors.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ms) = m.iter_mut().find(|ms| ms.url == url) {
            ms.probed = true;
            ms.supports_range = supports_range;
        }
    }

    /// Mark a mirror as unsafe for ranged requests and penalize it for future picks.
    pub fn mark_range_unsupported(&self, url: &str) {
        let mut m = self.mirrors.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ms) = m.iter_mut().find(|ms| ms.url == url) {
            ms.probed = true;
            ms.supports_range = false;
            ms.record_error();
        }
    }

    /// Mark a mirror as identity-verified against the canonical file.
    pub fn mark_identity_verified(&self, url: &str, canonical: bool) {
        let mut mirrors = self.mirrors.lock().unwrap_or_else(|e| e.into_inner());
        if canonical {
            for mirror in mirrors.iter_mut() {
                mirror.canonical = mirror.url == url;
            }
        }

        if let Some(ms) = mirrors.iter_mut().find(|ms| ms.url == url) {
            ms.quarantined = false;
            ms.identity_status = "verified".to_string();
            ms.quarantine_reason = None;
            if !canonical {
                ms.canonical = false;
            }
        }
    }

    /// Quarantine a mirror so it cannot be selected for segment mixing.
    pub fn quarantine_mirror(&self, url: &str, reason: impl Into<String>) {
        let reason = reason.into();
        let mut mirrors = self.mirrors.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ms) = mirrors.iter_mut().find(|ms| ms.url == url) {
            ms.quarantined = true;
            ms.canonical = false;
            ms.identity_status = "quarantined".to_string();
            ms.quarantine_reason = Some(reason);
        }
    }

    /// Add a new mirror dynamically (e.g. discovered during download).
    pub fn add_mirror(&self, url: String, source: String) {
        let mut m = self.mirrors.lock().unwrap_or_else(|e| e.into_inner());
        if !m.iter().any(|ms| ms.url == url) {
            m.push(MirrorStats::new(url, source));
        }
    }
}

fn configured_multi_source_segments(raw_segment_count: u32) -> u32 {
    if raw_segment_count == 0 {
        let adaptive = crate::adaptive_threads::recommended_threads();
        if adaptive >= 2 { adaptive } else { 8 }
    } else {
        raw_segment_count
    }
}

fn resolve_effective_segments(
    requested_segments: u32,
    resume_from: u64,
    supports_range: bool,
) -> Result<u32, String> {
    if resume_from > 0 && !supports_range {
        return Err(
            "Cannot safely resume a multi-source download because no selected mirror supports byte ranges"
                .to_string(),
        );
    }

    if !supports_range {
        Ok(1)
    } else {
        Ok(requested_segments.max(1))
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

    // 2. Determine file size from the best compatible mirror
    let saved = {
        let downloads = persistence::load_downloads().unwrap_or_default();
        downloads.into_iter().find(|d| d.id == id)
    };
    let resume_from = saved.as_ref().map(|s| s.downloaded_bytes).unwrap_or(0);
    let requested_segments = configured_multi_source_segments(settings.segments);
    let mut prefer_range_probe = requested_segments > 1 || resume_from > 0;

    let (best_url, probe) = loop {
        let candidate = if prefer_range_probe {
            if let Some(url) = pool.pick_best_matching(true) {
                url
            } else if resume_from > 0 {
                return Err(
                    "Cannot safely resume a multi-source download because no selected mirror supports byte ranges"
                        .to_string(),
                );
            } else {
                prefer_range_probe = false;
                continue;
            }
        } else {
            let url = pool.pick_best();
            if url.is_empty() {
                return Err("No usable mirrors available for multi-source download".to_string());
            }
            url
        };

        let probe = crate::downloader::initialization::determine_total_size(&client, &candidate).await?;
        pool.set_range_support(&candidate, probe.supports_range);

        if prefer_range_probe && !probe.supports_range {
            pool.mark_range_unsupported(&candidate);
            if pool.usable_count_matching(true) > 0 {
                continue;
            }
            if resume_from > 0 {
                return Err(
                    "Cannot safely resume a multi-source download because no selected mirror supports byte ranges"
                        .to_string(),
                );
            }
            prefer_range_probe = false;
            continue;
        }

        break (candidate, probe);
    };
    let canonical_identity = MirrorIdentity::from_probe(&probe);
    pool.mark_identity_verified(&best_url, true);

    for mirror_url in pool.usable_urls() {
        if mirror_url == best_url {
            continue;
        }

        match crate::downloader::initialization::determine_total_size(&client, &mirror_url).await {
            Ok(mirror_probe) => {
                pool.set_range_support(&mirror_url, mirror_probe.supports_range);
                let mirror_identity = MirrorIdentity::from_probe(&mirror_probe);
                if let Some(reason) = canonical_identity.mismatch_reason(&mirror_identity) {
                    println!(
                        "[multi-source] Quarantining mirror {} due to {}",
                        mirror_url, reason
                    );
                    pool.quarantine_mirror(&mirror_url, reason);
                } else {
                    pool.mark_identity_verified(&mirror_url, false);
                }
            }
            Err(error) => {
                println!(
                    "[multi-source] Quarantining mirror {} because identity probe failed: {}",
                    mirror_url, error
                );
                pool.quarantine_mirror(&mirror_url, format!("identity probe failed: {}", error));
            }
        }
    }

    let total_size = canonical_identity.total_size;
    let etag = canonical_identity.etag.clone();
    let md5 = canonical_identity.md5.clone();
    let effective_segments = resolve_effective_segments(
        requested_segments,
        resume_from,
        probe.supports_range,
    )?;

    if total_size == 0 {
        return Err("Cannot determine file size for multi-source download".to_string());
    }

    let _ = app.emit("mirror_stats", serde_json::json!({
        "id": id.clone(),
        "mirrors": pool.get_stats(),
    }));

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
    let path = if resume_from > 0 {
        saved
            .as_ref()
            .map(|download| download.path.clone())
            .filter(|saved_path| !saved_path.is_empty())
            .unwrap_or(final_path)
    } else {
        resolve_filename_collision(&final_path)
    };

    // 4. Setup file
    let file = crate::downloader::initialization::setup_file(&path, resume_from, total_size)?;
    let file_mutex = file;

    // 5. Initialize segment manager
    let manager = crate::downloader::initialization::setup_manager(
        total_size,
        saved.as_ref(),
        resume_from,
        effective_segments,
    );
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
        let canonical_identity_worker = canonical_identity.clone();

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
            let initial_request_requires_range = current_pos > 0 || end < total_size_worker;
            let mut current_mirror = match if initial_request_requires_range {
                pool_worker.pick_best_matching(true)
            } else {
                Some(pool_worker.pick_best())
            } {
                Some(url) if !url.is_empty() => url,
                _ => {
                    let m = manager_clone.lock().unwrap_or_else(|e| e.into_inner());
                    let mut segs = m.segments.write().unwrap_or_else(|e| e.into_inner());
                    if let Some(seg) = segs.iter_mut().find(|s| s.id == seg_id) {
                        seg.downloaded_cursor = current_pos;
                        seg.state = crate::downloader::structures::SegmentState::Error;
                    }
                    crate::media::sounds::play_error();
                    return;
                }
            };
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

                let requires_range_request = current_pos > 0 || end < total_size_worker;
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

                let mut request = client_clone.get(&current_mirror);
                if requires_range_request {
                    request = request.header("Range", &range_header);
                }
                let res_future = request.send();

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
                        if let Some(fallback) = pool_worker
                            .pick_fallback_matching(&current_mirror, requires_range_request)
                        {
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
                    if let Some(fallback) = pool_worker
                        .pick_fallback_matching(&current_mirror, requires_range_request)
                    {
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
                    if let Some(fallback) = pool_worker
                        .pick_fallback_matching(&current_mirror, requires_range_request)
                    {
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

                if requires_range_request && response.status() != rquest::StatusCode::PARTIAL_CONTENT {
                    println!(
                        "[multi-source] Seg {} mirror {} ignored Range (status {}), rejecting mirror",
                        seg_id,
                        current_mirror,
                        response.status().as_u16()
                    );
                    pool_worker.mark_range_unsupported(&current_mirror);
                    if let Some(fallback) = pool_worker.pick_fallback_matching(&current_mirror, true) {
                        current_mirror = fallback;
                        chunk_start_time = std::time::Instant::now();
                        chunk_bytes = 0;
                        retry_count = 0;
                        continue;
                    }

                    let m = manager_clone.lock().unwrap_or_else(|e| e.into_inner());
                    let mut segs = m.segments.write().unwrap_or_else(|e| e.into_inner());
                    if let Some(seg) = segs.iter_mut().find(|s| s.id == seg_id) {
                        seg.downloaded_cursor = current_pos;
                        seg.state = crate::downloader::structures::SegmentState::Error;
                    }
                    crate::media::sounds::play_error();
                    break;
                }

                if !response.status().is_success() {
                    pool_worker.record_error(&current_mirror);
                    if let Some(fallback) = pool_worker
                        .pick_fallback_matching(&current_mirror, requires_range_request)
                    {
                        println!(
                            "[multi-source] Seg {} unexpected HTTP {} on {}, switching → {}",
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

                if let Some(reason) = response_identity_mismatch(
                    &canonical_identity_worker,
                    &response,
                    requires_range_request,
                ) {
                    println!(
                        "[multi-source] Seg {} mirror {} failed identity check: {}",
                        seg_id, current_mirror, reason
                    );
                    pool_worker.quarantine_mirror(&current_mirror, reason);
                    let _ = app_handle_clone.emit("mirror_stats", serde_json::json!({
                        "id": id_worker.clone(),
                        "mirrors": pool_worker.get_stats(),
                    }));
                    if let Some(fallback) = pool_worker
                        .pick_fallback_matching(&current_mirror, requires_range_request)
                    {
                        current_mirror = fallback;
                        chunk_start_time = std::time::Instant::now();
                        chunk_bytes = 0;
                        retry_count = 0;
                        continue;
                    }

                    let m = manager_clone.lock().unwrap_or_else(|e| e.into_inner());
                    let mut segs = m.segments.write().unwrap_or_else(|e| e.into_inner());
                    if let Some(seg) = segs.iter_mut().find(|s| s.id == seg_id) {
                        seg.downloaded_cursor = current_pos;
                        seg.state = crate::downloader::structures::SegmentState::Error;
                    }
                    crate::media::sounds::play_error();
                    break;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn configure_mirror(pool: &MirrorPool, url: &str, avg_speed_bps: u64, supports_range: bool) {
        let mut mirrors = pool.mirrors.lock().unwrap();
        let mirror = mirrors.iter_mut().find(|mirror| mirror.url == url).unwrap();
        mirror.probed = true;
        mirror.avg_speed_bps = avg_speed_bps;
        mirror.latency_ms = 25;
        mirror.supports_range = supports_range;
    }

    #[test]
    fn resolve_effective_segments_falls_back_to_single_segment_without_range() {
        assert_eq!(resolve_effective_segments(8, 0, false).unwrap(), 1);
        assert_eq!(resolve_effective_segments(8, 0, true).unwrap(), 8);
    }

    #[test]
    fn resolve_effective_segments_rejects_resume_without_range() {
        let error = resolve_effective_segments(4, 1024, false).unwrap_err();
        assert!(error.contains("Cannot safely resume"));
    }

    #[test]
    fn pick_best_matching_requires_range_capability() {
        let primary = "https://primary.test/file";
        let mirror = "https://mirror.test/file";
        let pool = MirrorPool::new(primary, &[(mirror.to_string(), "Mirror".to_string())]);

        configure_mirror(&pool, primary, 1_000_000, false);
        configure_mirror(&pool, mirror, 350_000, true);

        assert_eq!(pool.pick_best_matching(true).as_deref(), Some(mirror));
        assert_eq!(pool.usable_count_matching(true), 1);
    }

    #[test]
    fn mark_range_unsupported_excludes_mirror_from_range_fallbacks() {
        let primary = "https://primary.test/file";
        let mirror_a = "https://mirror-a.test/file";
        let mirror_b = "https://mirror-b.test/file";
        let pool = MirrorPool::new(
            primary,
            &[
                (mirror_a.to_string(), "Mirror A".to_string()),
                (mirror_b.to_string(), "Mirror B".to_string()),
            ],
        );

        configure_mirror(&pool, primary, 100_000, false);
        configure_mirror(&pool, mirror_a, 900_000, true);
        configure_mirror(&pool, mirror_b, 500_000, true);

        assert_eq!(pool.pick_best_matching(true).as_deref(), Some(mirror_a));

        pool.mark_range_unsupported(mirror_a);

        assert_eq!(pool.pick_best_matching(true).as_deref(), Some(mirror_b));

        let stats = pool.get_stats();
        let mirror_a_stats = stats.iter().find(|mirror| mirror.url == mirror_a).unwrap();
        assert!(!mirror_a_stats.supports_range);
        assert_eq!(mirror_a_stats.error_count, 1);
    }

    #[test]
    fn mirror_identity_treats_weak_etags_as_equivalent() {
        let canonical = MirrorIdentity {
            total_size: 1024,
            etag: Some("\"abc123\"".to_string()),
            md5: None,
        };
        let equivalent = MirrorIdentity {
            total_size: 1024,
            etag: Some("W/\"abc123\"".to_string()),
            md5: None,
        };

        assert_eq!(canonical.mismatch_reason(&equivalent), None);
    }

    #[test]
    fn mirror_identity_rejects_md5_mismatch() {
        let canonical = MirrorIdentity {
            total_size: 1024,
            etag: None,
            md5: Some("abc".to_string()),
        };
        let mismatch = MirrorIdentity {
            total_size: 1024,
            etag: None,
            md5: Some("xyz".to_string()),
        };

        assert_eq!(
            canonical.mismatch_reason(&mismatch).as_deref(),
            Some("Content-MD5 mismatch")
        );
    }

    #[test]
    fn quarantine_mirror_excludes_it_from_future_selection() {
        let primary = "https://primary.test/file";
        let mirror_a = "https://mirror-a.test/file";
        let mirror_b = "https://mirror-b.test/file";
        let pool = MirrorPool::new(
            primary,
            &[
                (mirror_a.to_string(), "Mirror A".to_string()),
                (mirror_b.to_string(), "Mirror B".to_string()),
            ],
        );

        configure_mirror(&pool, mirror_a, 900_000, true);
        configure_mirror(&pool, mirror_b, 500_000, true);
        pool.mark_identity_verified(mirror_a, true);
        pool.mark_identity_verified(mirror_b, false);

        assert_eq!(pool.pick_best_matching(true).as_deref(), Some(mirror_a));

        pool.quarantine_mirror(mirror_a, "ETag mismatch");

        assert_eq!(pool.pick_best_matching(true).as_deref(), Some(mirror_b));

        let stats = pool.get_stats();
        let mirror_a_stats = stats.iter().find(|mirror| mirror.url == mirror_a).unwrap();
        assert!(mirror_a_stats.quarantined);
        assert_eq!(mirror_a_stats.identity_status, "quarantined");
        assert_eq!(mirror_a_stats.quarantine_reason.as_deref(), Some("ETag mismatch"));
        assert!(!mirror_a_stats.canonical);
    }
}
