//! Tauri Commands for Download State Management
//!
//! Provides production-grade commands for:
//! 1. Getting download state information
//! 2. Validating resume safety
//! 3. Recording state transitions
//! 4. Generating diagnostics & health reports

use crate::persistence::{SavedDownload, load_downloads};
use crate::session_state::{DownloadState, DownloadStateInfo};
use crate::session_recovery::{ResumeSafetyValidator, ResumeValidityReport, ValidationLevel};
use serde::{Deserialize, Serialize};

/// Get state information for a single download
#[tauri::command]
pub async fn get_download_state(id: String) -> Result<Option<DownloadStateResponse>, String> {
    let downloads = load_downloads()?;
    
    let download = downloads.into_iter().find(|d| d.id == id);
    match download {
        Some(d) => Ok(Some(DownloadStateResponse::from_saved(&d))),
        None => Ok(None),
    }
}

/// Get state information for all downloads
#[tauri::command]
pub async fn get_all_download_states() -> Result<Vec<DownloadStateResponse>, String> {
    let downloads = load_downloads()?;
    Ok(downloads.iter().map(|d| DownloadStateResponse::from_saved(d)).collect())
}

/// Check if a download is safe to resume
#[tauri::command]
pub async fn validate_resume_safety(id: String) -> Result<ResumeValidityReportResponse, String> {
    let downloads = load_downloads()?;
    let download = downloads.into_iter().find(|d| d.id == id)
        .ok_or_else(|| format!("Download {} not found", id))?;
    
    let report = ResumeSafetyValidator::validate_resume_safe(&download).await;
    Ok(ResumeValidityReportResponse::from(report))
}

/// Get diagnostic information about a download's state history
#[tauri::command]
pub async fn get_download_diagnostics(id: String) -> Result<DownloadDiagnosticsResponse, String> {
    let downloads = load_downloads()?;
    let download = downloads.into_iter().find(|d| d.id == id)
        .ok_or_else(|| format!("Download {} not found", id))?;
    
    let mut state_info = DownloadStateInfo::new(id.clone(), download.total_size);
    state_info.paused_at = download.last_active.clone();
    state_info.downloaded_bytes_at_pause = download.downloaded_bytes;
    state_info.total_bytes_at_pause = download.total_size;
    
    // Convert status string to state
    state_info.current_state = match download.status.as_str() {
        "Downloading" => DownloadState::Downloading,
        "Paused" => DownloadState::Paused,
        "Done" | "Complete" | "Completed" => DownloadState::Completed,
        "Error" => DownloadState::Error,
        _ => DownloadState::Pending,
    };
    
    if let Err(validation_err) = state_info.validate_consistency() {
        return Ok(DownloadDiagnosticsResponse {
            download_id: id,
            state: state_info.current_state.to_string(),
            downloaded_bytes: download.downloaded_bytes,
            total_size: download.total_size,
            progress_percent: if download.total_size > 0 {
                ((download.downloaded_bytes as f64 / download.total_size as f64) * 100.0) as u32
            } else {
                0
            },
            recommendation: format!("State validation failed: {}", validation_err),
            is_healthy: false,
            can_resume: false,
            warning_count: 1,
        });
    }
    
    Ok(DownloadDiagnosticsResponse {
        download_id: id,
        state: state_info.current_state.to_string(),
        downloaded_bytes: download.downloaded_bytes,
        total_size: download.total_size,
        progress_percent: if download.total_size > 0 {
            ((download.downloaded_bytes as f64 / download.total_size as f64) * 100.0) as u32
        } else {
            0
        },
        recommendation: "Download is in a valid state".to_string(),
        is_healthy: true,
        can_resume: download.status == "Paused",
        warning_count: 0,
    })
}

/// Get health summary for all downloads
#[tauri::command]
pub async fn get_downloads_health_summary() -> Result<DownloadsHealthSummary, String> {
    let downloads = load_downloads()?;
    
    let mut healthy = 0;
    let mut at_risk = 0;
    let mut failed = 0;
    let mut total_bytes = 0u64;
    let mut downloaded_bytes = 0u64;
    
    for d in &downloads {
        total_bytes += d.total_size;
        downloaded_bytes += d.downloaded_bytes;
        
        match d.status.as_str() {
            "Done" | "Complete" | "Completed" => healthy += 1,
            "Error" => failed += 1,
            "Paused" => {
                if d.error_message.is_some() {
                    failed += 1;
                } else {
                    at_risk += 1;
                }
            },
            _ => healthy += 1,
        }
    }
    
    Ok(DownloadsHealthSummary {
        total_downloads: downloads.len(),
        healthy_count: healthy,
        at_risk_count: at_risk,
        failed_count: failed,
        total_size_bytes: total_bytes,
        downloaded_bytes,
        overall_progress_percent: if total_bytes > 0 {
            ((downloaded_bytes as f64 / total_bytes as f64) * 100.0) as u32
        } else {
            0
        },
    })
}

// ─── Response DTOs ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadStateResponse {
    pub id: String,
    pub url: String,
    pub filename: String,
    pub status: String,
    pub downloaded_bytes: u64,
    pub total_size: u64,
    pub progress_percent: u32,
    pub last_active: Option<String>,
    pub error_message: Option<String>,
}

impl DownloadStateResponse {
    fn from_saved(download: &SavedDownload) -> Self {
        Self {
            id: download.id.clone(),
            url: download.url.clone(),
            filename: download.filename.clone(),
            status: download.status.clone(),
            downloaded_bytes: download.downloaded_bytes,
            total_size: download.total_size,
            progress_percent: if download.total_size > 0 {
                ((download.downloaded_bytes as f64 / download.total_size as f64) * 100.0) as u32
            } else {
                0
            },
            last_active: download.last_active.clone(),
            error_message: download.error_message.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumeValidityReportResponse {
    pub download_id: String,
    pub level: String,
    pub can_resume: bool,
    pub requires_confirmation: bool,
    pub cannot_resume: bool,
    pub checks_passed: Vec<String>,
    pub checks_warning: Vec<String>,
    pub checks_failed: Vec<String>,
    pub recommendation: String,
    pub suggested_retry_delay_secs: Option<u64>,
    pub should_restart_from_scratch: bool,
    pub summary: String,
}

impl From<ResumeValidityReport> for ResumeValidityReportResponse {
    fn from(report: ResumeValidityReport) -> Self {
        let level = match report.level {
            ValidationLevel::Safe => "safe",
            ValidationLevel::Caution => "caution",
            ValidationLevel::Warning => "warning",
            ValidationLevel::Blocked => "blocked",
        }.to_string();

        Self {
            can_resume: report.can_resume(),
            requires_confirmation: report.requires_confirmation(),
            cannot_resume: report.cannot_resume(),
            checks_passed: report.checks_passed,
            checks_warning: report.checks_warning,
            checks_failed: report.checks_failed,
            recommendation: report.recommendation,
            suggested_retry_delay_secs: report.suggested_retry_delay_secs,
            should_restart_from_scratch: report.should_restart_from_scratch,
            summary: report.summary,
            download_id: report.download_id,
            level,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadDiagnosticsResponse {
    pub download_id: String,
    pub state: String,
    pub downloaded_bytes: u64,
    pub total_size: u64,
    pub progress_percent: u32,
    pub recommendation: String,
    pub is_healthy: bool,
    pub can_resume: bool,
    pub warning_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadsHealthSummary {
    pub total_downloads: usize,
    pub healthy_count: usize,
    pub at_risk_count: usize,
    pub failed_count: usize,
    pub total_size_bytes: u64,
    pub downloaded_bytes: u64,
    pub overall_progress_percent: u32,
}
