//! Smart Route Manager Commands
//!
//! Tauri IPC commands for exposing smart route optimization to the frontend.
//! Provides real-time routing metrics, mirror rankings, and decision history.

use tauri::{command, State};
use crate::smart_route_manager::{
    GLOBAL_ROUTE_MANAGER, RouteDecision, MirrorHealthSnapshot, RouteStatus, RouteHistoryEntry, SmartRouteConfig
};
use crate::core_state::AppState;
use crate::network::mirror_aggregator::MirrorHealthReport;
use serde::{Deserialize, Serialize};

/// Get current route status for a download
#[command]
pub fn get_route_status(download_id: String) -> Result<Option<RouteStatus>, String> {
    Ok(GLOBAL_ROUTE_MANAGER.get_route_status(&download_id))
}

/// Optimize route for a download given available mirrors
#[command]
pub fn optimize_download_route(
    download_id: String,
    available_mirrors: Vec<(String, u8)>,
    current_speed_bps: u64,
    remaining_bytes: u64,
) -> Result<RouteDecision, String> {
    Ok(GLOBAL_ROUTE_MANAGER.optimize_route(
        &download_id,
        available_mirrors,
        current_speed_bps,
        remaining_bytes,
    ))
}

/// Get all mirrors ranked by current health score
#[command]
pub fn get_mirror_health_rankings() -> Result<Vec<MirrorHealthSnapshot>, String> {
    Ok(GLOBAL_ROUTE_MANAGER.get_mirror_rankings())
}

/// Get route decision history for a specific download or all downloads
#[command]
pub fn get_route_decision_history(
    download_id: Option<String>,
    limit: u32,
) -> Result<Vec<RouteHistoryEntry>, String> {
    Ok(GLOBAL_ROUTE_MANAGER.get_route_history(
        download_id.as_deref(),
        limit as usize,
    ))
}

/// Get current smart route configuration
#[command]
pub fn get_smart_route_config() -> Result<SmartRouteConfig, String> {
    // In production, would load from settings file
    Ok(SmartRouteConfig::default())
}

/// Update smart route configuration
#[command]
pub fn update_smart_route_config(config: SmartRouteConfig) -> Result<(), String> {
    // In production, would persist to settings file
    // For now, just validate
    if config.max_parallel_mirrors < 1 || config.max_parallel_mirrors > 10 {
        return Err("max_parallel_mirrors must be 1-10".to_string());
    }
    if config.min_mirror_score_threshold > 100 {
        return Err("min_mirror_score_threshold must be 0-100".to_string());
    }
    Ok(())
}

/// Batch request: get all route metrics for a download
/// Useful for dashboard to get everything in one call
#[derive(Debug, Serialize, Deserialize)]
pub struct RouteDashboardSnapshot {
    pub download_id: String,
    pub route_status: Option<RouteStatus>,
    pub mirror_rankings: Vec<MirrorHealthSnapshot>,
    pub recent_decisions: Vec<RouteHistoryEntry>,
}

#[command]
pub fn get_route_dashboard_snapshot(download_id: String, history_limit: u32) -> Result<RouteDashboardSnapshot, String> {
    let route_status = GLOBAL_ROUTE_MANAGER.get_route_status(&download_id);
    let mirror_rankings = GLOBAL_ROUTE_MANAGER.get_mirror_rankings();
    let recent_decisions = GLOBAL_ROUTE_MANAGER.get_route_history(Some(&download_id), history_limit as usize);

    Ok(RouteDashboardSnapshot {
        download_id,
        route_status,
        mirror_rankings,
        recent_decisions,
    })
}

/// Record route decision telemetry for analysis and feedback
#[tauri::command]
pub fn record_route_decision_outcome(
    download_id: String,
    decision_id: String,
    mirror_url: String,
    success: bool,
    duration_ms: f64,
    bytes_transferred: u64,
) -> Result<(), String> {
    crate::smart_route_manager::GLOBAL_ROUTE_MANAGER.record_decision_outcome(
        &download_id,
        &decision_id,
        &mirror_url,
        success,
        duration_ms,
        bytes_transferred,
    );
    Ok(())
}

/// Get mirrors discovered and verified by the autonomous aggregator
#[tauri::command]
pub async fn get_active_mirrors(
    download_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<MirrorHealthReport>, String> {
    Ok(state.mirror_aggregator.get_active_mirrors(&download_id).await)
}

/// Get detailed health vitals for a specific mirror
#[tauri::command]
pub async fn get_mirror_vitals(
    url: String,
    _state: State<'_, AppState>,
) -> Result<MirrorHealthReport, String> {
    // For now, return the latest health report from any download that uses this mirror
    // This is a placeholder for a more advanced global health registry
    Ok(MirrorHealthReport {
        url,
        status: crate::network::mirror_scout::ScoutStatus::Valid,
        latency_ms: 0,
        verified_at_ms: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_optimize_route_command() {
        let mirrors = vec![
            ("https://mirror1.com".to_string(), 85),
            ("https://mirror2.com".to_string(), 70),
        ];

        let result = optimize_download_route(
            "test-download".to_string(),
            mirrors,
            2_000_000,
            1_000_000_000,
        );

        assert!(result.is_ok());
        let decision = result.unwrap();
        assert_eq!(decision.download_id, "test-download");
    }

    #[test]
    fn test_get_route_status_command() {
        let result = get_route_status("test-download".to_string());
        assert!(result.is_ok());
        // Status is optional - download might not exist yet
    }

    #[test]
    fn test_config_validation() {
        let mut config = SmartRouteConfig::default();
        config.max_parallel_mirrors = 15; // Invalid

        let result = update_smart_route_config(config);
        assert!(result.is_err());
    }
}
