// resilience_cmds.rs — Tauri commands for resilience and recovery system
//
// Exposes resilience monitoring and recovery functionality to the frontend
#![allow(dead_code)]

use crate::resilience::ResilienceEngine;
use crate::auto_recovery::AutoRecoveryEngine;
use crate::network_diagnostics::NetworkDiagnostics;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Command: Get health status for a download
#[tauri::command]
pub fn get_download_health(
    download_id: String,
    resilience_state: tauri::State<Arc<ResilienceEngine>>,
) -> Result<HealthStatusResponse, String> {
    match resilience_state.get_health(&download_id) {
        Some(health) => Ok(HealthStatusResponse {
            success: true,
            health: Some(health),
            error: None,
        }),
        None => Ok(HealthStatusResponse {
            success: false,
            health: None,
            error: Some("Download not found".to_string()),
        }),
    }
}

/// Command: Get all downloads at risk
#[tauri::command]
pub fn get_at_risk_downloads(
    resilience_state: tauri::State<Arc<ResilienceEngine>>,
) -> Result<Vec<crate::resilience::DownloadHealth>, String> {
    Ok(resilience_state.get_at_risk_downloads())
}

/// Command: Get error statistics
#[tauri::command]
pub fn get_error_statistics(
    resilience_state: tauri::State<Arc<ResilienceEngine>>,
    minutes: u64,
) -> Result<ErrorStatsResponse, String> {
    let errors = resilience_state.get_recent_errors(minutes);
    let stats = resilience_state.get_error_stats();

    Ok(ErrorStatsResponse {
        recent_errors_count: errors.len(),
        error_categories: stats,
        total_errors: errors.len(),
    })
}

/// Command: Get recovery plan for a download
#[tauri::command]
pub fn get_recovery_plan(
    download_id: String,
    recovery_state: tauri::State<Arc<AutoRecoveryEngine>>,
) -> Result<RecoveryPlanResponse, String> {
    match recovery_state.get_next_action(&download_id) {
        Some(action) => Ok(RecoveryPlanResponse {
            success: true,
            action: Some(action),
            error: None,
        }),
        None => Ok(RecoveryPlanResponse {
            success: false,
            action: None,
            error: Some("No pending recovery actions".to_string()),
        }),
    }
}

/// Command: Execute recovery action
#[tauri::command]
pub fn execute_recovery_action(
    download_id: String,
    action_id: String,
    success: bool,
    recovery_state: tauri::State<Arc<AutoRecoveryEngine>>,
) -> Result<ActionExecutionResponse, String> {
    recovery_state.mark_action_executed(&download_id, &action_id, success);

    Ok(ActionExecutionResponse {
        success: true,
        message: "Action recorded".to_string(),
    })
}

/// Command: Get recovery statistics
#[tauri::command]
pub fn get_recovery_statistics(
    recovery_state: tauri::State<Arc<AutoRecoveryEngine>>,
) -> Result<crate::auto_recovery::RecoveryStats, String> {
    Ok(recovery_state.get_stats())
}

/// Command: Get pending recovery actions
#[tauri::command]
pub fn get_pending_recovery_actions(
    recovery_state: tauri::State<Arc<AutoRecoveryEngine>>,
) -> Result<Vec<crate::auto_recovery::RecoveryAction>, String> {
    Ok(recovery_state.get_pending_actions())
}

/// Command: Get network diagnostics summary
#[tauri::command]
pub fn get_diagnostics_summary(
    diagnostics_state: tauri::State<Arc<NetworkDiagnostics>>,
) -> Result<DiagnosticsSummaryResponse, String> {
    let summary = diagnostics_state.export_diagnostics_summary();
    Ok(DiagnosticsSummaryResponse {
        total_tests: summary.total_tests,
        total_reports: summary.total_reports,
        total_anomalies: summary.total_anomalies,
        current_pattern: format!("{:?}", summary.current_pattern),
        current_health: summary.current_health,
        recent_anomalies: summary.recent_anomalies,
    })
}

/// Command: Get diagnostic reports
#[tauri::command]
pub fn get_diagnostic_reports(
    limit: usize,
    diagnostics_state: tauri::State<Arc<NetworkDiagnostics>>,
) -> Result<Vec<crate::network_diagnostics::DiagnosticReport>, String> {
    Ok(diagnostics_state.get_recent_reports(limit))
}

/// Response types

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthStatusResponse {
    pub success: bool,
    pub health: Option<crate::resilience::DownloadHealth>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorStatsResponse {
    pub recent_errors_count: usize,
    pub error_categories: std::collections::HashMap<String, u32>,
    pub total_errors: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RecoveryPlanResponse {
    pub success: bool,
    pub action: Option<crate::auto_recovery::RecoveryAction>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ActionExecutionResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiagnosticsSummaryResponse {
    pub total_tests: usize,
    pub total_reports: usize,
    pub total_anomalies: usize,
    pub current_pattern: String,
    pub current_health: Option<crate::network_diagnostics::NetworkHealth>,
    pub recent_anomalies: Vec<crate::network_diagnostics::AnomalyDetection>,
}
