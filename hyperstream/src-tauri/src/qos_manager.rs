use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use lazy_static::lazy_static;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Priority {
    Critical,  // No throttling, maximum bandwidth
    High,      // 75% of available bandwidth
    Normal,    // 50% (default)
    Low,       // 25%
    Background // 10%, minimal impact
}

#[derive(Serialize, Clone, Debug)]
pub struct QosEntry {
    pub download_id: String,
    pub priority: Priority,
    pub max_bytes_per_sec: u64,
    pub current_bytes_per_sec: u64,
    pub total_downloaded: u64,
}

#[derive(Serialize, Clone, Debug)]
pub struct QosStats {
    pub total_bandwidth_limit: u64,
    pub total_active: usize,
    pub entries: Vec<QosEntry>,
}

lazy_static! {
    static ref QOS_TABLE: Mutex<HashMap<String, QosEntry>> = Mutex::new(HashMap::new());
    static ref GLOBAL_BANDWIDTH_LIMIT: Mutex<u64> = Mutex::new(0); // 0 = unlimited
}

/// Set the global bandwidth limit (bytes/sec). 0 = unlimited.
pub fn set_global_bandwidth_limit(limit: u64) {
    if let Ok(mut global) = GLOBAL_BANDWIDTH_LIMIT.lock() {
        *global = limit;
    }
}

/// Set priority level for a specific download, which determines bandwidth allocation.
pub fn set_download_priority(download_id: String, priority_str: String) -> Result<String, String> {
    let priority = match priority_str.to_lowercase().as_str() {
        "critical" => Priority::Critical,
        "high" => Priority::High,
        "normal" => Priority::Normal,
        "low" => Priority::Low,
        "background" => Priority::Background,
        _ => return Err(format!("Invalid priority: {}. Use: critical, high, normal, low, background", priority_str)),
    };

    let weight = priority_weight(&priority);
    let global_limit = *GLOBAL_BANDWIDTH_LIMIT.lock().unwrap_or_else(|e| e.into_inner());

    let max_bps = if global_limit > 0 {
        (global_limit as f64 * weight) as u64
    } else {
        0 // unlimited
    };

    let entry = QosEntry {
        download_id: download_id.clone(),
        priority: priority.clone(),
        max_bytes_per_sec: max_bps,
        current_bytes_per_sec: 0,
        total_downloaded: 0,
    };

    {
        let mut table = QOS_TABLE.lock().unwrap_or_else(|e| e.into_inner());
        table.insert(download_id.clone(), entry);
    }

    // Rebalance all active limits so total never exceeds global limit
    rebalance_limits();

    Ok(format!("Priority set to {:?} for download {}", priority, download_id))
}

/// Get the current QoS stats for all tracked downloads.
pub fn get_qos_stats() -> Result<QosStats, String> {
    let table = QOS_TABLE.lock().map_err(|e| format!("Lock error: {}", e))?;
    let global_limit = *GLOBAL_BANDWIDTH_LIMIT.lock().unwrap_or_else(|e| e.into_inner());

    Ok(QosStats {
        total_bandwidth_limit: global_limit,
        total_active: table.len(),
        entries: table.values().cloned().collect(),
    })
}

/// Get the bandwidth limit (bytes/sec) for a specific download. Returns 0 for unlimited.
pub fn get_download_limit(download_id: &str) -> u64 {
    let table = QOS_TABLE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(entry) = table.get(download_id) {
        return entry.max_bytes_per_sec;
    }
    0 // No limit set
}

/// Update the current speed measurement for a download.
pub fn update_download_speed(download_id: &str, bytes_per_sec: u64, total: u64) {
    let mut table = QOS_TABLE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(entry) = table.get_mut(download_id) {
        entry.current_bytes_per_sec = bytes_per_sec;
        entry.total_downloaded = total;
    }
}

/// Remove a download from QoS tracking.
pub fn remove_download(download_id: &str) {
    {
        let mut table = QOS_TABLE.lock().unwrap_or_else(|e| e.into_inner());
        table.remove(download_id);
    }
    // Rebalance remaining downloads to reclaim freed bandwidth
    rebalance_limits();
}

/// Rebalance all active download limits proportionally so total never exceeds global limit.
fn rebalance_limits() {
    let global_limit = *GLOBAL_BANDWIDTH_LIMIT.lock().unwrap_or_else(|e| e.into_inner());
    if global_limit == 0 { return; } // unlimited — nothing to rebalance

    let mut table = match QOS_TABLE.lock() {
        Ok(t) => t,
        Err(e) => e.into_inner(),
    };
    let total_weight: f64 = table.values().map(|e| priority_weight(&e.priority)).sum();
    if total_weight == 0.0 { return; }

    for entry in table.values_mut() {
        let weight = priority_weight(&entry.priority);
        entry.max_bytes_per_sec = ((global_limit as f64 * weight) / total_weight) as u64;
    }
}

fn priority_weight(p: &Priority) -> f64 {
    match p {
        Priority::Critical => 1.0,
        Priority::High => 0.75,
        Priority::Normal => 0.50,
        Priority::Low => 0.25,
        Priority::Background => 0.10,
    }
}
