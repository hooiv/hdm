//! Bandwidth History — persistent speed tracking for analytics and graphs.
//!
//! Records aggregate download speed samples at configurable intervals and
//! stores them in a ring buffer persisted to disk. The frontend can query
//! the history to render bandwidth-over-time graphs.

use serde::{Deserialize, Serialize};
use std::sync::Mutex;

/// Maximum number of samples to retain (1 sample/5s × 24h ≈ 17,280).
const MAX_SAMPLES: usize = 17_280;

/// How often to record a sample (seconds).
const SAMPLE_INTERVAL_SECS: u64 = 5;

lazy_static::lazy_static! {
    static ref HISTORY: Mutex<BandwidthHistory> = Mutex::new(BandwidthHistory::new());
}

/// A single speed sample.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SpeedSample {
    /// Unix timestamp (seconds since epoch).
    pub ts: u64,
    /// Aggregate download speed in bytes/sec across all active downloads.
    pub speed_bps: u64,
    /// Number of active downloads at this moment.
    pub active_count: u32,
}

/// Ring buffer of speed samples.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthHistory {
    samples: Vec<SpeedSample>,
    /// Aggregate bytes downloaded since tracking started (session total).
    pub total_bytes: u64,
    /// Peak speed observed this session.
    pub peak_speed_bps: u64,
}

impl BandwidthHistory {
    fn new() -> Self {
        Self {
            samples: Vec::with_capacity(MAX_SAMPLES),
            total_bytes: 0,
            peak_speed_bps: 0,
        }
    }

    fn push(&mut self, sample: SpeedSample) {
        if sample.speed_bps > self.peak_speed_bps {
            self.peak_speed_bps = sample.speed_bps;
        }
        self.total_bytes = self.total_bytes.saturating_add(sample.speed_bps * SAMPLE_INTERVAL_SECS);
        if self.samples.len() >= MAX_SAMPLES {
            self.samples.remove(0);
        }
        self.samples.push(sample);
    }
}

/// Record a speed sample. Called by the progress emitter (~every 5 seconds).
pub fn record_sample(speed_bps: u64, active_count: u32) {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let sample = SpeedSample {
        ts,
        speed_bps,
        active_count,
    };

    if let Ok(mut history) = HISTORY.lock() {
        history.push(sample);
    }
}

/// Retrieve samples within a given time window.
/// `since_secs` is the number of seconds to look back (e.g. 3600 for last hour).
pub fn get_samples(since_secs: u64) -> Vec<SpeedSample> {
    let cutoff = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().saturating_sub(since_secs))
        .unwrap_or(0);

    if let Ok(history) = HISTORY.lock() {
        history
            .samples
            .iter()
            .filter(|s| s.ts >= cutoff)
            .cloned()
            .collect()
    } else {
        Vec::new()
    }
}

/// Get aggregate statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthStats {
    pub sample_count: usize,
    pub total_bytes: u64,
    pub peak_speed_bps: u64,
    pub average_speed_bps: u64,
    /// Speed samples for the requested time window.
    pub samples: Vec<SpeedSample>,
}

pub fn get_stats(since_secs: u64) -> BandwidthStats {
    let samples = get_samples(since_secs);
    let avg = if samples.is_empty() {
        0
    } else {
        let total_speed: u64 = samples.iter().map(|s| s.speed_bps).sum();
        total_speed / samples.len() as u64
    };

    let (total_bytes, peak) = if let Ok(history) = HISTORY.lock() {
        (history.total_bytes, history.peak_speed_bps)
    } else {
        (0, 0)
    };

    BandwidthStats {
        sample_count: samples.len(),
        total_bytes,
        peak_speed_bps: peak,
        average_speed_bps: avg,
        samples,
    }
}

/// Start the background sampler that periodically records aggregate speed.
pub fn start_bandwidth_sampler(app: tauri::AppHandle) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(SAMPLE_INTERVAL_SECS));
        loop {
            interval.tick().await;

            let (speed, count) = {
                if let Some(state) = tauri::Manager::try_state::<crate::core_state::AppState>(&app) {
                    let downloads = state.downloads.lock().unwrap_or_else(|e| e.into_inner());
                    let count = downloads.len() as u32;
                    let speed: u64 = downloads.values().map(|session| {
                        session.manager.lock()
                            .map(|m| m.total_speed())
                            .unwrap_or(0)
                    }).sum();
                    (speed, count)
                } else {
                    (0, 0)
                }
            };

            record_sample(speed, count);
        }
    });
}

/// Save history to disk (called on app exit).
pub fn persist_history() {
    let path = get_history_path();
    if let Ok(history) = HISTORY.lock() {
        let json = serde_json::to_string(&*history).unwrap_or_default();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, json);
    }
}

/// Restore history from disk (called on app startup).
pub fn restore_history() {
    let path = get_history_path();
    if let Ok(data) = std::fs::read_to_string(&path) {
        if let Ok(history) = serde_json::from_str::<BandwidthHistory>(&data) {
            if let Ok(mut h) = HISTORY.lock() {
                *h = history;
            }
        }
    }
}

fn get_history_path() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
        .join("hyperstream")
        .join("bandwidth_history.json")
}
