//! Parallel Mirror Retry Commands
//!
//! Tauri IPC commands for monitoring and controlling the proactive
//! mirror failover and bandwidth arbitrage system.

use tauri::{command, State};
use crate::core_state::AppState;
use crate::parallel_mirror_retry::ParallelRetryConfig;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ParallelRetryStats {
    pub total_proactive_splits: u32,
    pub active_shadow_workers: u32,
    pub bandwidth_saved_ms: u64,
}

/// Get current stats for the parallel retry system
#[command]
pub async fn get_parallel_retry_stats(
    _state: State<'_, AppState>,
) -> Result<ParallelRetryStats, String> {
    // Placeholder for a global registry of retry stats
    Ok(ParallelRetryStats {
        total_proactive_splits: 0,
        active_shadow_workers: 0,
        bandwidth_saved_ms: 0,
    })
}

/// Manually force a parallel retry for a specific segment
#[command]
pub async fn force_parallel_retry(
    download_id: String,
    segment_id: u32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let aggregator = state.mirror_aggregator.clone();
    
    // 1. Get alternative mirror
    let mirrors = aggregator.get_active_mirrors(&download_id).await;
    if mirrors.is_empty() {
        return Err("No alternative mirrors available for retry".to_string());
    }

    // 2. Trigger proactive split
    let downloads = state.downloads.lock().unwrap_or_else(|e| e.into_inner());
    let session = downloads.get(&download_id).ok_or("Download session not found")?;
    
    let _new_seg = session.manager.lock().unwrap().trigger_proactive_split(segment_id)
        .ok_or("Could not split segment (too small or already finished)")?;

    // In a full implementation, we'd signal the DownloadSession to spawn a worker 
    // with the specific mirror. For now, the background monitor will eventually pick it up.
    
    Ok(())
}

/// Update parallel retry configuration on the fly
#[command]
pub async fn update_parallel_retry_config(
    _config: ParallelRetryConfig,
    _state: State<'_, AppState>,
) -> Result<(), String> {
    // Placeholder for dynamic config update
    Ok(())
}
