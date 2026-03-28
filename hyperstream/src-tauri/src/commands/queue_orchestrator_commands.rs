/// Queue Orchestration Commands — Tauri IPC handlers
/// 
/// Exposes queue orchestration features to the frontend:
/// - Real-time bandwidth allocation
/// - Queue analysis and ETC prediction
/// - Bottleneck detection
/// - Performance recommendations

use tauri::command;
use crate::queue_orchestrator::{QueueOrchestrator, QueueOrchestrationState, QueueAnalysis, DownloadMetrics};
use std::sync::OnceLock;

/// Global queue orchestrator instance
pub static QUEUE_ORCHESTRATOR: OnceLock<QueueOrchestrator> = OnceLock::new();

pub fn init_orchestrator() {
    let _ = QUEUE_ORCHESTRATOR.get_or_init(QueueOrchestrator::new);
}

/// Get global orchestrator instance
pub fn get_orchestrator() -> &'static QueueOrchestrator {
    QUEUE_ORCHESTRATOR.get_or_init(QueueOrchestrator::new)
}

/// Get real-time queue orchestration state
#[command]
pub fn get_queue_orchestration_state() -> Result<QueueOrchestrationState, String> {
    let orch = get_orchestrator();
    let metrics = orch.get_metrics(None)?;

    Ok(QueueOrchestrationState {
        total_active_downloads: metrics.iter().filter(|m| !m.is_blocked).count() as u32,
        total_queued_downloads: 0, // Would be fetched from queue manager
        global_bandwidth_available_bps: 0,
        global_bandwidth_used_bps: metrics.iter().map(|m| m.current_speed_bps).sum(),
        estimated_queue_completion_ms: metrics
            .iter()
            .map(|m| m.estimated_remaining_ms)
            .max()
            .unwrap_or(0),
        queue_efficiency: if metrics.is_empty() {
            0.0
        } else {
            metrics
                .iter()
                .map(|m| m.current_speed_bps as f64)
                .sum::<f64>()
                / (metrics.len() as f64 * 10_000_000.0)
                .min(1.0)
        },
        conflict_count: 0,
        downloads: metrics,
    })
}

/// Get detailed queue analysis with recommendations
#[command]
pub fn analyze_queue_health(
    total_queued: u32,
    total_active: u32,
    global_limit: u32,
) -> Result<QueueAnalysis, String> {
    let orch = get_orchestrator();
    orch.analyze_queue(total_queued, total_active, global_limit)
}

/// Get speed trend for a specific download
#[command]
pub fn get_download_speed_trend(id: String) -> Result<String, String> {
    let orch = get_orchestrator();
    orch.get_speed_trend(&id)
}

/// Get metrics for all or specific download
#[command]
pub fn get_download_metrics(id: Option<String>) -> Result<Vec<DownloadMetrics>, String> {
    let orch = get_orchestrator();
    orch.get_metrics(id.as_deref())
}

/// Register a download with the orchestrator
#[command]
pub fn register_orchestrated_download(
    id: String,
    url: String,
    total_bytes: u64,
    priority: u8,
) -> Result<(), String> {
    let orch = get_orchestrator();
    orch.register_download(&id, &url, total_bytes, priority)
}

/// Record progress for a download
#[command]
pub fn record_download_progress(
    id: String,
    bytes_this_sample: u64,
    elapsed_ms: u64,
) -> Result<(), String> {
    let orch = get_orchestrator();
    orch.record_progress(&id, bytes_this_sample, elapsed_ms)
}

/// Unregister a download
#[command]
pub fn unregister_orchestrated_download(id: String) -> Result<(), String> {
    let orch = get_orchestrator();
    orch.unregister_download(&id)
}

/// Mark a download as blocked (waiting for dependency)
#[command]
pub fn set_download_blocked(id: String, blocked: bool) -> Result<(), String> {
    let orch = get_orchestrator();
    orch.set_blocked(&id, blocked)
}

/// Request intelligent bandwidth allocation
///
/// Returns mapping of download_id -> allocated_bytes_per_second
#[command]
pub fn request_bandwidth_allocation(available_bps: u64) -> Result<std::collections::HashMap<String, u64>, String> {
    let orch = get_orchestrator();
    orch.allocate_bandwidth(available_bps)
}

/// Set global bandwidth limit
#[command]
pub fn set_global_bandwidth_limit(bps: u64) -> Result<(), String> {
    let orch = get_orchestrator();
    orch.set_global_bandwidth_limit(bps);
    Ok(())
}

/// Get formatted bytes (e.g., "1.50 MB")
#[command]
pub fn format_bytes_human_readable(bytes: u64) -> Result<String, String> {
    Ok(QueueOrchestrator::format_bytes(bytes))
}
