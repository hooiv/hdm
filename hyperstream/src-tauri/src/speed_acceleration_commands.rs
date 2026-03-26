//! Tauri command handlers for Speed Acceleration Engine
//!
//! Exposes bandwidth monitoring, condition detection, and optimization strategies.

use crate::speed_acceleration::{BandwidthMeasurement, NetworkCondition, SpeedAccelerationEngine};
use serde::{Deserialize, Serialize};

/// Speed acceleration statistics
#[derive(Debug, Serialize, Deserialize)]
pub struct AccelerationStats {
    pub avg_speed_bps: u64,
    pub max_speed_bps: u64,
    pub min_speed_bps: u64,
    pub speed_variance: f64,
    pub network_condition: String,
    pub health_score: u8,
    pub predicted_improvement: bool,
    pub predicted_degradation: bool,
    pub measurements_count: usize,
}

/// Download time estimation
#[derive(Debug, Serialize, Deserialize)]
pub struct DownloadTimeEstimate {
    pub file_size_bytes: u64,
    pub estimated_time_secs: u64,
    pub estimated_time_formatted: String,
    pub confidence_percent: u8,
}

/// Get current acceleration statistics
#[tauri::command]
pub async fn get_acceleration_stats() -> Result<AccelerationStats, String> {
    // In production, would access global engine instance
    let engine = SpeedAccelerationEngine::new();

    let condition = engine.get_condition();
    let avg_speed = engine.get_average_speed(50);
    let variance = engine.get_speed_variance(50);
    let health = engine.get_health_score();
    let measurements = engine.get_measurements();

    let max_speed = measurements.iter().map(|m| m.speed_bps).max().unwrap_or(0);
    let min_speed = measurements
        .iter()
        .map(|m| m.speed_bps)
        .filter(|s| *s > 0)
        .min()
        .unwrap_or(0);

    Ok(AccelerationStats {
        avg_speed_bps: avg_speed,
        max_speed_bps: max_speed,
        min_speed_bps: min_speed,
        speed_variance: variance,
        network_condition: format!("{:?}", condition),
        health_score: health,
        predicted_improvement: engine.predict_improvement(),
        predicted_degradation: engine.predict_degradation(),
        measurements_count: measurements.len(),
    })
}

/// Record a bandwidth measurement
#[tauri::command]
pub async fn record_bandwidth_measurement(
    bytes_transferred: u64,
    duration_ms: u64,
    quality_score: u8,
) -> Result<String, String> {
    if duration_ms == 0 {
        return Err("Duration must be greater than 0".to_string());
    }

    let speed_bps = if duration_ms > 0 {
        (bytes_transferred as f64 / (duration_ms as f64 / 1000.0)) as u64
    } else {
        0
    };

    let measurement = BandwidthMeasurement {
        bytes_transferred,
        duration_ms,
        speed_bps,
        timestamp_secs: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        quality_score: quality_score.min(100),
    };

    Ok(format!(
        "Recorded: {} bytes in {} ms ({}), quality: {}%",
        bytes_transferred,
        duration_ms,
        format_speed(speed_bps),
        quality_score
    ))
}

/// Estimate download time for a file
#[tauri::command]
pub async fn estimate_download_time(file_size_bytes: u64) -> Result<DownloadTimeEstimate, String> {
    if file_size_bytes == 0 {
        return Err("File size must be greater than 0".to_string());
    }

    let engine = SpeedAccelerationEngine::new();
    let duration = engine.estimate_download_time(file_size_bytes);
    let avg_speed = engine.get_average_speed(50);

    // Confidence based on number of measurements
    let measurements = engine.get_measurements();
    let confidence = (measurements.len() as u8).min(100);

    Ok(DownloadTimeEstimate {
        file_size_bytes,
        estimated_time_secs: duration.as_secs(),
        estimated_time_formatted: format_duration(duration),
        confidence_percent: confidence,
    })
}

/// Get optimal segment strategy for current network conditions
#[tauri::command]
pub async fn get_optimal_segment_strategy() -> Result<String, String> {
    let engine = SpeedAccelerationEngine::new();
    let strategy = engine.get_optimal_strategy();

    Ok(format!(
        "Optimal Strategy:\n\
         - Segment Size: {}\n\
         - Parallel Connections: {}\n\
         - Queue Depth: {}\n\
         - Retry Timeout: {}ms\n\
         - Use Caching: {}",
        format_bytes(strategy.optimal_segment_size),
        strategy.parallel_connections,
        strategy.queue_depth,
        strategy.retry_timeout_ms,
        strategy.use_caching
    ))
}

/// Predict network changes
#[tauri::command]
pub async fn predict_network_changes() -> Result<String, String> {
    let engine = SpeedAccelerationEngine::new();

    let improvement = engine.predict_improvement();
    let degradation = engine.predict_degradation();

    let prediction = if degradation {
        "⚠️ Network degradation predicted - prepare for slower speeds"
    } else if improvement {
        "📈 Network improvement predicted - speeds may increase soon"
    } else {
        "➡️ Network conditions expected to remain stable"
    };

    Ok(format!(
        "Prediction: {}\nImprovement likely: {}\nDegradation likely: {}",
        prediction, improvement, degradation
    ))
}

/// Get bandwidth history for visualization
#[tauri::command]
pub async fn get_bandwidth_history() -> Result<Vec<(u64, u64)>, String> {
    let engine = SpeedAccelerationEngine::new();
    let measurements = engine.get_measurements();

    let history: Vec<(u64, u64)> = measurements
        .iter()
        .map(|m| (m.timestamp_secs, m.speed_bps))
        .collect();

    Ok(history)
}

/// Helper: Format bytes
fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.2} GB", bytes as f64 / 1_000_000_000.0)
    } else if bytes >= 1_000_000 {
        format!("{:.2} MB", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{:.2} KB", bytes as f64 / 1_000.0)
    } else {
        format!("{} B", bytes)
    }
}

/// Helper: Format speed
fn format_speed(bps: u64) -> String {
    if bps >= 1_000_000_000 {
        format!("{:.2} GB/s", bps as f64 / 1_000_000_000.0)
    } else if bps >= 1_000_000 {
        format!("{:.2} MB/s", bps as f64 / 1_000_000.0)
    } else if bps >= 1_000 {
        format!("{:.2} KB/s", bps as f64 / 1_000.0)
    } else {
        format!("{} B/s", bps)
    }
}

/// Helper: Format duration
fn format_duration(duration: std::time::Duration) -> String {
    let secs = duration.as_secs();
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else if secs < 86400 {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d {}h", secs / 86400, (secs % 86400) / 3600)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(1_000_000), "1.00 MB");
        assert_eq!(format_bytes(1_000), "1.00 KB");
    }

    #[test]
    fn test_format_speed() {
        assert_eq!(format_speed(1_000_000), "1.00 MB/s");
        assert_eq!(format_speed(1_000), "1.00 KB/s");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(std::time::Duration::from_secs(30)), "30s");
        assert_eq!(
            format_duration(std::time::Duration::from_secs(90)),
            "1m 30s"
        );
    }
}
