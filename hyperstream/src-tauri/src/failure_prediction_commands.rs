//! Tauri commands for failure prediction engine
//!
//! Exposes the prediction engine to the frontend with proper error handling
//! and integration with the download lifecycle.

use crate::failure_prediction::*;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

/// Request to add metrics for analysis
#[derive(Debug, Deserialize)]
pub struct AddMetricsRequest {
    pub download_id: String,
    pub speed_bps: u64,
    pub idle_time_ms: u64,
    pub active_connections: u32,
    pub recent_errors: u32,
    pub timeout_count: u32,
    pub latency_ms: u64,
    pub jitter_ms: u32,
    pub avg_segment_time_ms: u64,
    pub retried_bytes: u64,
    pub retry_rate_percent: f32,
    pub dns_failures: u32,
    pub rate_limit_hits: u32,
    pub access_denied_hits: u32,
    pub connection_refused: u32,
}

/// Response from prediction analysis
#[derive(Debug, Serialize)]
pub struct PredictionResponse {
    pub success: bool,
    pub prediction: Option<FailurePrediction>,
    pub error: Option<String>,
}

/// Get current acceleration stats
#[tauri::command]
pub async fn record_download_metrics(
    download_id: String,
    speed_bps: u64,
    idle_time_ms: u64,
    active_connections: u32,
    recent_errors: u32,
    timeout_count: u32,
    latency_ms: u64,
    jitter_ms: u32,
    avg_segment_time_ms: u64,
    retried_bytes: u64,
    retry_rate_percent: f32,
    dns_failures: u32,
    rate_limit_hits: u32,
    access_denied_hits: u32,
    connection_refused: u32,
    app: AppHandle,
    state: State<'_, crate::AppState>,
) -> Result<String, String> {
    let engine = state
        .failure_prediction_engine
        .lock()
        .map_err(|e| format!("Failed to acquire lock: {}", e))?;

    let metrics = DownloadMetrics {
        speed_bps,
        idle_time_ms,
        active_connections,
        recent_errors,
        timeout_count,
        latency_ms,
        jitter_ms,
        avg_segment_time_ms,
        retried_bytes,
        retry_rate_percent,
        dns_failures,
        rate_limit_hits,
        access_denied_hits,
        connection_refused,
        timestamp_secs: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };

    engine.add_metrics(&download_id, metrics);

    // Check if we should predict failure
    if let Some(prediction) = engine.predict_failure(&download_id) {
        // Emit event with high-risk predictions
        if prediction.probability_percent > 60 {
        if let Some(main_window) = app.get_webview_window("main") {
            let _ = main_window.emit("failure_prediction", &prediction);
        }
        }
    }

    Ok("Metrics recorded".to_string())
}

#[tauri::command]
pub async fn reset_prediction_history(
    download_id: String,
    state: State<'_, crate::AppState>,
) -> Result<String, String> {
    let engine = state
        .failure_prediction_engine
        .lock()
        .map_err(|e| format!("Failed to acquire lock: {}", e))?;

    engine.reset_history(&download_id);
    Ok(format!("History reset for {}", download_id))
}

#[tauri::command]
pub async fn simulate_prediction_chaos(
    download_id: String,
    scenario: String,
    state: State<'_, crate::AppState>,
    app: AppHandle,
) -> Result<String, String> {
    let engine = state
        .failure_prediction_engine
        .lock()
        .map_err(|e| format!("Failed to acquire lock: {}", e))?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let mut metrics_list = Vec::new();

    match scenario.as_str() {
        "GradualStall" => {
            // Simulate 10MB/s -> zero over 10 samples
            for i in 0..10 {
                let speed = 10_000_000 - (i * 1_000_000);
                metrics_list.push(DownloadMetrics {
                    speed_bps: speed,
                    idle_time_ms: if i == 9 { 5000 } else { 0 },
                    active_connections: 8,
                    recent_errors: 0,
                    timeout_count: 0,
                    latency_ms: (50 + (i * 50)) as u64,
                    jitter_ms: 0,
                    avg_segment_time_ms: 1000,
                    retried_bytes: 0,
                    retry_rate_percent: 0.0,
                    dns_failures: 0,
                    rate_limit_hits: 0,
                    access_denied_hits: 0,
                    connection_refused: 0,
                    timestamp_secs: now + i as u64,
                });
            }
        }
        "HighJitter" => {
            // Constant speed but increasing jitter and latent delays
            for i in 0..10 {
                metrics_list.push(DownloadMetrics {
                    speed_bps: 5_000_000,
                    idle_time_ms: 0,
                    active_connections: 8,
                    recent_errors: 1,
                    timeout_count: i / 2,
                    latency_ms: (100 + (i * 100)) as u64,
                    jitter_ms: i * 50,
                    avg_segment_time_ms: 2000,
                    retried_bytes: i as u64 * 1024,
                    retry_rate_percent: 10.0,
                    dns_failures: 0,
                    rate_limit_hits: 0,
                    access_denied_hits: 0,
                    connection_refused: 0,
                    timestamp_secs: now + i as u64,
                });
            }
        }
        "RateLimited" => {
            // Increasing 429s
            for i in 0..10 {
                metrics_list.push(DownloadMetrics {
                    speed_bps: 2_000_000,
                    idle_time_ms: 0,
                    active_connections: 8,
                    recent_errors: i,
                    timeout_count: 0,
                    latency_ms: 200,
                    jitter_ms: 0,
                    avg_segment_time_ms: 1000,
                    retried_bytes: 0,
                    retry_rate_percent: 20.0,
                    dns_failures: 0,
                    rate_limit_hits: i,
                    access_denied_hits: 0,
                    connection_refused: 0,
                    timestamp_secs: now + i as u64,
                });
            }
        }
        _ => return Err(format!("Unknown scenario: {}", scenario)),
    }

    engine.add_bulk_metrics(&download_id, metrics_list);

    // After bulk update, provide an initial prediction to trigger events
    if let Some(prediction) = engine.predict_failure(&download_id) {
        if let Some(main_window) = app.get_webview_window("main") {
            let _ = main_window.emit("failure_prediction", &prediction);
        }
    }

    Ok(format!("Simulation '{}' applied to {}", scenario, download_id))
}

/// Get failure prediction for current download
#[tauri::command]
pub async fn analyze_failure_risk(
    download_id: String,
    state: State<'_, crate::AppState>,
) -> Result<PredictionResponse, String> {
    let engine = state
        .failure_prediction_engine
        .lock()
        .map_err(|e| format!("Failed to acquire lock: {}", e))?;

    match engine.predict_failure(&download_id) {
        Some(prediction) => Ok(PredictionResponse {
            success: true,
            prediction: Some(prediction),
            error: None,
        }),
        None => Ok(PredictionResponse {
            success: true,
            prediction: None,
            error: None,
        }),
    }
}

/// Report whether a prediction was accurate
#[tauri::command]
pub async fn record_prediction_accuracy(
    prediction_id: String,
    actually_failed: bool,
    state: State<'_, crate::AppState>,
) -> Result<String, String> {
    let engine = state
        .failure_prediction_engine
        .lock()
        .map_err(|e| format!("Failed to acquire lock: {}", e))?;

    engine.record_prediction_result(&prediction_id, actually_failed);
    Ok("Accuracy recorded".to_string())
}

/// Record a failure we didn't predict
#[tauri::command]
pub async fn record_missed_failure(
    download_id: String,
    state: State<'_, crate::AppState>,
) -> Result<String, String> {
    let engine = state
        .failure_prediction_engine
        .lock()
        .map_err(|e| format!("Failed to acquire lock: {}", e))?;

    engine.record_missed_failure(&download_id);
    Ok("Missed failure recorded".to_string())
}

/// Get prediction accuracy statistics
#[tauri::command]
pub async fn get_prediction_accuracy_stats(
    state: State<'_, crate::AppState>,
) -> Result<PredictionAccuracy, String> {
    let engine = state
        .failure_prediction_engine
        .lock()
        .map_err(|e| format!("Failed to acquire lock: {}", e))?;

    Ok(engine.get_accuracy_stats())
}

#[tauri::command]
pub async fn get_current_failure_prediction(
    download_id: String,
    state: State<'_, crate::AppState>,
) -> Result<Option<FailurePrediction>, String> {
    let engine = state
        .failure_prediction_engine
        .lock()
        .map_err(|e| format!("Failed to acquire lock: {}", e))?;

    Ok(engine.get_current_prediction(&download_id))
}

/// Reset prediction engine
#[tauri::command]
pub async fn reset_failure_prediction(state: State<'_, crate::AppState>) -> Result<String, String> {
    let engine = state
        .failure_prediction_engine
        .lock()
        .map_err(|e| format!("Failed to acquire lock: {}", e))?;

    engine.reset();
    Ok("Failure prediction engine reset".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::failure_prediction::{FailurePredictionEngine, PredictionConfig};

    #[test]
    fn test_metrics_recording() {
        let engine = FailurePredictionEngine::new(PredictionConfig::default());
        let metrics = DownloadMetrics {
            speed_bps: 5_000_000,
            idle_time_ms: 100,
            active_connections: 4,
            recent_errors: 0,
            timeout_count: 0,
            latency_ms: 50,
            jitter_ms: 5,
            avg_segment_time_ms: 1000,
            retried_bytes: 0,
            retry_rate_percent: 0.0,
            dns_failures: 0,
            rate_limit_hits: 0,
            access_denied_hits: 0,
            connection_refused: 0,
            timestamp_secs: 1000,
        };

        engine.add_metrics("test", metrics);
        // Verify no panic
        assert!(engine.predict_failure("test").is_none());
    }

    #[test]
    fn test_connection_stalled_detection() {
        let engine = FailurePredictionEngine::new(PredictionConfig::default());

        // Add normal metrics first
        for i in 0..5 {
            let metrics = DownloadMetrics {
                speed_bps: 5_000_000,
                idle_time_ms: 100,
                active_connections: 4,
                recent_errors: 0,
                timeout_count: 0,
                latency_ms: 50,
                jitter_ms: 5,
                avg_segment_time_ms: 1000,
                retried_bytes: 0,
                retry_rate_percent: 0.0,
                dns_failures: 0,
                rate_limit_hits: 0,
                access_denied_hits: 0,
                connection_refused: 0,
                timestamp_secs: 1000 + i,
            };
            engine.add_metrics("test", metrics);
        }

        // Add stalled metrics
        let stalled = DownloadMetrics {
            speed_bps: 0,
            idle_time_ms: 35_000, // > 30s stall
            active_connections: 0,
            recent_errors: 0,
            timeout_count: 0,
            latency_ms: 50,
            jitter_ms: 5,
            avg_segment_time_ms: 5000,
            retried_bytes: 0,
            retry_rate_percent: 0.0,
            dns_failures: 0,
            rate_limit_hits: 0,
            access_denied_hits: 0,
            connection_refused: 0,
            timestamp_secs: 1005,
        };
        engine.add_metrics("test", stalled);

        let prediction = engine.predict_failure("test");
        assert!(prediction.is_some());
        let pred = prediction.unwrap();
        assert!(pred.probability_percent > 20);
        assert_eq!(pred.reason, FailureReason::ConnectionStalled);
    }

    #[test]
    fn test_timeout_pattern_detection() {
        let engine = FailurePredictionEngine::new(PredictionConfig::default());

        let timeout_metrics = DownloadMetrics {
            speed_bps: 2_000_000,
            idle_time_ms: 1000,
            active_connections: 2,
            recent_errors: 7,
            timeout_count: 6, // > 5 threshold
            latency_ms: 200,
            jitter_ms: 50,
            avg_segment_time_ms: 2000,
            retried_bytes: 1_000_000,
            retry_rate_percent: 20.0,
            dns_failures: 0,
            rate_limit_hits: 0,
            access_denied_hits: 0,
            connection_refused: 1,
            timestamp_secs: 1000,
        };

        engine.add_metrics("test", timeout_metrics);

        let prediction = engine.predict_failure("test");
        assert!(prediction.is_some());
        let pred = prediction.unwrap();
        assert!(pred.probability_percent > 20);
    }

    #[test]
    fn test_rate_limiting_detection() {
        let engine = FailurePredictionEngine::new(PredictionConfig::default());

        let rate_limited = DownloadMetrics {
            speed_bps: 3_000_000,
            idle_time_ms: 500,
            active_connections: 4,
            recent_errors: 3,
            timeout_count: 1,
            latency_ms: 75,
            jitter_ms: 10,
            avg_segment_time_ms: 1500,
            retried_bytes: 500_000,
            retry_rate_percent: 10.0,
            dns_failures: 0,
            rate_limit_hits: 2, // Rate limit detected
            access_denied_hits: 0,
            connection_refused: 0,
            timestamp_secs: 1000,
        };

        engine.add_metrics("test", rate_limited);

        let prediction = engine.predict_failure("test");
        assert!(prediction.is_some());
        let pred = prediction.unwrap();
        assert!(pred.probability_percent > 20);
        assert_eq!(pred.reason, FailureReason::RateLimiting);
    }

    #[test]
    fn test_accuracy_tracking() {
        let engine = FailurePredictionEngine::new(PredictionConfig::default());

        let metrics = DownloadMetrics {
            speed_bps: 5_000_000,
            idle_time_ms: 100,
            active_connections: 4,
            recent_errors: 0,
            timeout_count: 0,
            latency_ms: 50,
            jitter_ms: 5,
            avg_segment_time_ms: 1000,
            retried_bytes: 0,
            retry_rate_percent: 0.0,
            dns_failures: 0,
            rate_limit_hits: 0,
            access_denied_hits: 0,
            connection_refused: 0,
            timestamp_secs: 1000,
        };

        engine.add_metrics("test", metrics);
        let pred = engine.predict_failure("test").unwrap_or_default();

        engine.record_prediction_result(&pred.prediction_id, true);

        let stats = engine.get_accuracy_stats();
        assert_eq!(stats.correct_predictions, 1);
    }

    #[test]
    fn test_multiple_failure_reasons() {
        let engine = FailurePredictionEngine::new(PredictionConfig::default());

        let bad_metrics = DownloadMetrics {
            speed_bps: 500_000,   // Very slow
            idle_time_ms: 20_000, // Stalling
            active_connections: 1,
            recent_errors: 8,
            timeout_count: 4,
            latency_ms: 300,
            jitter_ms: 100,
            avg_segment_time_ms: 5000,
            retried_bytes: 5_000_000,
            retry_rate_percent: 40.0,
            dns_failures: 1,
            rate_limit_hits: 1,
            access_denied_hits: 0,
            connection_refused: 2,
            timestamp_secs: 1000,
        };

        engine.add_metrics("test", bad_metrics);

        let prediction = engine.predict_failure("test");
        assert!(prediction.is_some());
        let pred = prediction.unwrap();
        // Multiple issues should result in higher probability
        assert!(pred.probability_percent > 40);
    }

    #[test]
    fn test_healthy_download_no_prediction() {
        let engine = FailurePredictionEngine::new(PredictionConfig::default());

        // Perfect conditions
        let healthy = DownloadMetrics {
            speed_bps: 15_000_000, // Excellent speed
            idle_time_ms: 50,      // Very responsive
            active_connections: 8,
            recent_errors: 0,
            timeout_count: 0,
            latency_ms: 20,
            jitter_ms: 2,
            avg_segment_time_ms: 500,
            retried_bytes: 0,
            retry_rate_percent: 0.0,
            dns_failures: 0,
            rate_limit_hits: 0,
            access_denied_hits: 0,
            connection_refused: 0,
            timestamp_secs: 1000,
        };

        engine.add_metrics("test", healthy);

        let prediction = engine.predict_failure("test");
        // Should not predict failure for perfect conditions
        assert!(prediction.is_none());
    }
}
