/// Group Metrics Commands — Expose real-time metrics to frontend
///
/// Provides Tauri commands for accessing:
/// - Real-time per-group metrics
/// - Member-level performance tracking
/// - Aggregate system metrics
/// - Trend analysis and predictions

use crate::group_metrics::{GroupMetrics, GroupMetricsTracker, PerformanceSummary};
use std::sync::{Arc, Mutex};
use lazy_static::lazy_static;

lazy_static! {
    /// Global metrics tracker instance
    static ref GLOBAL_GROUP_METRICS: Arc<Mutex<GroupMetricsTracker>> =
        Arc::new(Mutex::new(GroupMetricsTracker::new(5000))); // 5-second sample interval
}

/// Get current metrics for a specific group
#[tauri::command]
pub fn get_group_metrics(group_id: String) -> Result<GroupMetricsResponse, String> {
    let tracker = GLOBAL_GROUP_METRICS.lock().map_err(|e| e.to_string())?;

    match tracker.get_group_metrics(&group_id)? {
        Some(metrics) => Ok(GroupMetricsResponse {
            success: true,
            metrics: Some(metrics),
            error: None,
        }),
        None => Ok(GroupMetricsResponse {
            success: false,
            metrics: None,
            error: Some(format!("Group {} not found", group_id)),
        }),
    }
}

/// Get metrics for all active groups
#[tauri::command]
pub fn get_all_group_metrics() -> Result<AllGroupMetricsResponse, String> {
    let tracker = GLOBAL_GROUP_METRICS.lock().map_err(|e| e.to_string())?;

    let metrics = tracker.get_all_metrics()?;
    let aggregate = tracker.get_aggregate_metrics()?;

    Ok(AllGroupMetricsResponse {
        groups: metrics,
        aggregate,
    })
}

/// Get member-level metrics for a group
#[tauri::command]
pub fn get_group_member_metrics(group_id: String) -> Result<MemberMetricsResponse, String> {
    let tracker = GLOBAL_GROUP_METRICS.lock().map_err(|e| e.to_string())?;

    let members = tracker.get_member_metrics(&group_id)?;
    let member_count = members.len();

    Ok(MemberMetricsResponse {
        group_id,
        members,
        member_count,
    })
}

/// Get historical trend data for a group
#[tauri::command]
pub fn get_group_trends(group_id: String) -> Result<TrendsResponse, String> {
    let tracker = GLOBAL_GROUP_METRICS.lock().map_err(|e| e.to_string())?;

    let trends = tracker.get_group_trends(&group_id)?;
    let point_count = trends.len();

    Ok(TrendsResponse {
        group_id,
        trend_points: trends,
        point_count,
    })
}

/// Get performance summary for a group
#[tauri::command]
pub fn get_group_performance_summary(group_id: String) -> Result<PerformanceSummary, String> {
    let tracker = GLOBAL_GROUP_METRICS.lock().map_err(|e| e.to_string())?;

    tracker.get_group_performance_summary(&group_id)
}

/// Estimate completion time for a group
#[tauri::command]
pub fn estimate_group_completion_time(group_id: String) -> Result<CompletionTimeResponse, String> {
    let tracker = GLOBAL_GROUP_METRICS.lock().map_err(|e| e.to_string())?;

    match tracker.estimate_completion_time(&group_id) {
        Ok(eta_seconds) => Ok(CompletionTimeResponse {
            group_id,
            eta_seconds,
            success: true,
        }),
        Err(e) => Ok(CompletionTimeResponse {
            group_id,
            eta_seconds: 0,
            success: false,
        }),
    }
}

/// Get system-wide download statistics
#[tauri::command]
pub fn get_system_download_stats() -> Result<SystemStatsResponse, String> {
    let tracker = GLOBAL_GROUP_METRICS.lock().map_err(|e| e.to_string())?;

    let aggregate = tracker.get_aggregate_metrics()?;

    Ok(SystemStatsResponse {
        total_groups: aggregate.total_groups,
        active_groups: aggregate.active_groups,
        completed_groups: aggregate.completed_groups,
        failed_groups: aggregate.failed_groups,
        total_transferred: aggregate.total_transferred,
        total_remaining: aggregate.total_remaining,
        system_speed_bps: aggregate.system_speed as u64,
        system_cpu_percent: aggregate.system_cpu_percent,
        system_memory_bytes: aggregate.system_memory_usage,
        global_eta_seconds: aggregate.global_eta_seconds,
    })
}

// ============ Response DTOs ============

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct GroupMetricsResponse {
    pub success: bool,
    pub metrics: Option<GroupMetrics>,
    pub error: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct AllGroupMetricsResponse {
    pub groups: Vec<GroupMetrics>,
    pub aggregate: crate::group_metrics::AggregateMetrics,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct MemberMetricsResponse {
    pub group_id: String,
    pub members: Vec<crate::group_metrics::MemberMetrics>,
    pub member_count: usize,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct TrendsResponse {
    pub group_id: String,
    pub trend_points: Vec<crate::group_metrics::TrendDataPoint>,
    pub point_count: usize,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct CompletionTimeResponse {
    pub group_id: String,
    pub eta_seconds: u64,
    pub success: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct SystemStatsResponse {
    pub total_groups: usize,
    pub active_groups: usize,
    pub completed_groups: usize,
    pub failed_groups: usize,
    pub total_transferred: u64,
    pub total_remaining: u64,
    pub system_speed_bps: u64,
    pub system_cpu_percent: f64,
    pub system_memory_bytes: u64,
    pub global_eta_seconds: u64,
}
