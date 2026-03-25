//! Mirror Scoring Commands - Production-grade mirror reliability and failure prediction
//!
//! Exposes mirror scoring metrics, failure risk prediction, and recording APIs to the frontend.

use crate::mirror_scoring::{MirrorMetrics, GLOBAL_MIRROR_SCORER};
use crate::failure_prediction::{FailurePredictor, FailureType};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// Mirror score response sent to frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirrorScoreResponse {
    pub url: String,
    pub reliability_score: f64,
    pub speed_score: f64,
    pub uptime_percentage: f64,
    pub risk_level: String,
}

/// Failure risk response sent to frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureRiskResponse {
    pub url: String,
    pub failure_risk_percent: f64,
    pub recommendation: String,
}

impl From<MirrorMetrics> for MirrorScoreResponse {
    fn from(metrics: MirrorMetrics) -> Self {
        MirrorScoreResponse {
            url: metrics.url,
            reliability_score: metrics.reliability_score,
            speed_score: metrics.speed_score,
            uptime_percentage: metrics.uptime_percentage,
            risk_level: metrics.risk_level,
        }
    }
}

/// Global failure predictor instance for use throughout the application
fn get_global_failure_predictor() -> &'static FailurePredictor {
    static GLOBAL_FAILURE_PREDICTOR: OnceLock<FailurePredictor> = OnceLock::new();
    GLOBAL_FAILURE_PREDICTOR.get_or_init(|| FailurePredictor::new())
}

/// Get mirror score for a specific URL
///
/// Returns current reliability metrics for a mirror including:
/// - Reliability score (0-100) based on success/failure history
/// - Speed score (0-100) based on average latency
/// - Uptime percentage based on success/failure counts
/// - Risk level classification (Healthy, Caution, Warning, Critical)
#[tauri::command]
pub fn get_mirror_score(url: String) -> Result<MirrorScoreResponse, String> {
    let metrics = GLOBAL_MIRROR_SCORER
        .get_mirror_score(&url)
        .ok_or_else(|| format!("No metrics found for mirror: {}", url))?;
    
    Ok(MirrorScoreResponse::from(metrics))
}

/// Record a successful download from a mirror
///
/// Updates global scoring with success data including bytes transferred and latency.
/// Uses EMA algorithm to gradually increase reliability score.
#[tauri::command]
pub fn record_mirror_success(
    url: String,
    bytes_transferred: u32,
    latency_ms: u32,
) -> Result<(), String> {
    let latency_f64 = latency_ms as f64;
    GLOBAL_MIRROR_SCORER.record_success(&url, latency_f64);
    Ok(())
}

/// Record a failed download from a mirror
///
/// Updates global scoring with failure data including failure reason.
/// Uses EMA algorithm to gradually decrease reliability score.
#[tauri::command]
pub fn record_mirror_failure(url: String, reason: String) -> Result<(), String> {
    GLOBAL_MIRROR_SCORER.record_failure(&url);
    
    // Also record in failure predictor for future risk assessment
    let predictor = get_global_failure_predictor();
    let failure_type = match reason.to_lowercase().as_str() {
        s if s.contains("timeout") => FailureType::Timeout,
        s if s.contains("corrupt") => FailureType::Corruption,
        s if s.contains("rate") => FailureType::RateLimit,
        _ => FailureType::Timeout, // Default to timeout
    };
    predictor.record_failure(&url, failure_type);
    
    Ok(())
}

/// Get all mirrors ranked by reliability score (highest first)
///
/// Returns all tracked mirrors sorted by reliability score in descending order.
/// Useful for intelligent mirror selection and monitoring.
#[tauri::command]
pub fn get_ranked_mirrors() -> Result<Vec<MirrorScoreResponse>, String> {
    let ranked = GLOBAL_MIRROR_SCORER.rank_mirrors();
    let response = ranked
        .into_iter()
        .map(MirrorScoreResponse::from)
        .collect();
    
    Ok(response)
}

/// Predict segment download failure risk for a specific URL
///
/// Analyzes historical failure patterns and segment characteristics to predict
/// the probability of failure for a segment download.
///
/// Returns:
/// - failure_risk_percent: Predicted failure probability (0-100)
/// - recommendation: Action recommendation (CRITICAL, WARNING, OK, etc.)
#[tauri::command]
pub fn predict_segment_failure_risk(
    url: String,
    segment_size_bytes: u32,
    is_resume: bool,
) -> Result<FailureRiskResponse, String> {
    let predictor = get_global_failure_predictor();
    let risk_percent = predictor.predict_failure_risk(&url, segment_size_bytes, is_resume);
    
    let recommendation = if risk_percent >= 80.0 {
        format!("CRITICAL: Failure risk {}% is very high - consider using alternative mirror", risk_percent as u32)
    } else if risk_percent >= 60.0 {
        format!("WARNING: Failure risk {}% is elevated - mirror may be unreliable", risk_percent as u32)
    } else if risk_percent >= 40.0 {
        format!("CAUTION: Failure risk {}% is moderate - standard monitoring recommended", risk_percent as u32)
    } else {
        format!("OK: Failure risk {}% is acceptable", risk_percent as u32)
    };
    
    Ok(FailureRiskResponse {
        url,
        failure_risk_percent: risk_percent,
        recommendation,
    })
}

/// Get all mirror metrics (same as get_ranked_mirrors but unordered)
///
/// Returns all tracked mirrors with current metrics for monitoring and analytics.
#[tauri::command]
pub fn get_all_mirror_metrics() -> Result<Vec<MirrorScoreResponse>, String> {
    let all_metrics = GLOBAL_MIRROR_SCORER.get_all_metrics();
    let response = all_metrics
        .into_iter()
        .map(MirrorScoreResponse::from)
        .collect();
    
    Ok(response)
}
