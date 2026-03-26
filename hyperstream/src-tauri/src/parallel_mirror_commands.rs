//! Tauri commands for parallel mirror retry functionality
//!
//! Exposes mirror selection and retry strategy configuration to the frontend.
//! Allows users to configure aggressive or conservative parallel retry approaches.

use crate::parallel_mirror_retry::{
    ParallelRetryConfig, ParallelMirrorRetryManager, ParallelRetryResult, MirrorAttemptResult,
};
use serde::Serialize;
use tauri::State;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Get current parallel retry configuration
#[tauri::command]
pub async fn get_parallel_retry_config() -> Result<ParallelRetryConfig, String> {
    // For now, return default config. In production, would load from settings
    Ok(ParallelRetryConfig::default())
}

/// Update parallel retry configuration
#[tauri::command]
pub async fn update_parallel_retry_config(
    new_config: ParallelRetryConfig,
) -> Result<(), String> {
    // Validate configuration
    if new_config.max_concurrent_mirrors < 2 || new_config.max_concurrent_mirrors > 10 {
        return Err("max_concurrent_mirrors must be between 2 and 10".to_string());
    }
    if new_config.attempt_timeout_secs < 5 || new_config.attempt_timeout_secs > 60 {
        return Err("attempt_timeout_secs must be between 5 and 60".to_string());
    }
    if new_config.min_mirror_score_threshold > 100 {
        return Err("min_mirror_score_threshold must be 0-100".to_string());
    }

    // In production, would persist to settings
    eprintln!("[ParallelRetry] Updated config: {:?}", new_config);
    Ok(())
}

/// Select optimal mirrors based on scores and configuration
/// 
/// Returns a list of mirror URLs to use for parallel retry
#[tauri::command]
pub async fn select_optimal_mirrors(
    available_mirrors: Vec<(String, u8)>, // (url, score) pairs
    max_concurrent: u32,
    min_score: u8,
) -> Result<Vec<String>, String> {
    if available_mirrors.is_empty() {
        return Err("No mirrors provided".to_string());
    }

    let manager = ParallelMirrorRetryManager::new();
    let selected = manager.select_mirrors(&available_mirrors, max_concurrent, min_score);

    if selected.is_empty() {
        return Err("No mirrors met the minimum score threshold".to_string());
    }

    Ok(selected)
}

/// Estimate aggregated throughput from multiple mirrors
/// 
/// `conservative` = true: max_speed + (second_max / 2)
/// `conservative` = false: sum of all speeds
#[tauri::command]
pub async fn estimate_aggregated_throughput(
    individual_speeds_bps: Vec<u64>,
    conservative: bool,
) -> Result<u64, String> {
    if individual_speeds_bps.is_empty() {
        return Err("No speeds provided".to_string());
    }

    let estimated = ParallelMirrorRetryManager::estimate_aggregated_speed(
        &individual_speeds_bps,
        conservative,
    );

    Ok(estimated)
}

/// Simulate a parallel mirror retry result (for testing/preview)
/// 
/// This command helps users understand what would happen with their mirror set
#[tauri::command]
pub async fn simulate_parallel_retry(
    mirror_speeds_bps: Vec<u64>, // Individual mirror speeds
    segment_size_bytes: u64,
    num_successful: u32, // How many mirrors succeed
) -> Result<ParallelRetrySimulation, String> {
    if mirror_speeds_bps.is_empty() {
        return Err("No mirror speeds provided".to_string());
    }

    let num_mirrors = mirror_speeds_bps.len() as u32;
    if num_successful > num_mirrors {
        return Err("num_successful cannot exceed number of mirrors".to_string());
    }

    // Calculate individual durations
    let mut mirror_results = Vec::new();
    let mut total_aggregated_speed = 0u64;

    for (idx, speed) in mirror_speeds_bps.iter().enumerate() {
        let succeeded = idx < num_successful as usize;
        let duration_ms = if succeeded && *speed > 0 {
            ((segment_size_bytes as f64 / *speed as f64) * 1000.0) as u64
        } else {
            0
        };

        if succeeded {
            total_aggregated_speed += speed;
        }

        mirror_results.push(MirrorSimulationResult {
            mirror_index: idx as u32,
            speed_bps: *speed,
            duration_ms,
            succeeded,
        });
    }

    // Total time is the maximum (race to first success)
    let total_duration_ms = mirror_results
        .iter()
        .filter(|r| r.succeeded)
        .map(|r| r.duration_ms)
        .max()
        .unwrap_or(0);

    Ok(ParallelRetrySimulation {
        num_mirrors: num_mirrors as u32,
        num_successful,
        segment_size_bytes,
        individual_results: mirror_results,
        aggregated_speed_bps: total_aggregated_speed,
        expected_completion_ms: total_duration_ms,
        improvement_vs_single: if let Some(first_speed) = mirror_speeds_bps.first() {
            if *first_speed > 0 {
                ((total_aggregated_speed as f64 / *first_speed as f64) * 100.0) as u32
            } else {
                0
            }
        } else {
            0
        },
    })
}

/// Simulation result showing what users can expect
#[derive(Debug, Clone, Serialize)]
pub struct ParallelRetrySimulation {
    /// Total number of mirrors available
    pub num_mirrors: u32,
    /// Number of mirrors that succeeded
    pub num_successful: u32,
    /// Size of segment being downloaded
    pub segment_size_bytes: u64,
    /// Individual mirror results
    pub individual_results: Vec<MirrorSimulationResult>,
    /// Combined speed from all successful mirrors
    pub aggregated_speed_bps: u64,
    /// Expected time to complete the segment
    pub expected_completion_ms: u64,
    /// Speedup percentage vs. single fastest mirror
    pub improvement_vs_single: u32,
}

/// Individual mirror result in simulation
#[derive(Debug, Clone, Serialize)]
pub struct MirrorSimulationResult {
    /// Which mirror (0-indexed)
    pub mirror_index: u32,
    /// Speed of this mirror (bytes per second)
    pub speed_bps: u64,
    /// Time for this mirror to complete segment
    pub duration_ms: u64,
    /// Whether this mirror succeeded
    pub succeeded: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_select_mirrors_filters_by_score() {
        let mirrors = vec![
            ("https://mirror1.com".to_string(), 95),
            ("https://mirror2.com".to_string(), 40),
        ];
        let result = select_optimal_mirrors(mirrors, 2, 50).await;
        assert!(result.is_ok());
        let selected = result.unwrap();
        assert_eq!(selected.len(), 1); // Only mirror1 has score >= 50
    }

    #[tokio::test]
    async fn test_estimate_throughput_aggressive() {
        let speeds = vec![1000000, 2000000, 1500000];
        let result = estimate_aggregated_throughput(speeds, false).await;
        assert_eq!(result.unwrap(), 4500000); // Sum
    }

    #[tokio::test]
    async fn test_estimate_throughput_conservative() {
        let speeds = vec![1000000, 2000000, 1500000];
        let result = estimate_aggregated_throughput(speeds, true).await;
        // max(2M) + second_max/2 (1.5M/2 = 0.75M) = 2.75M
        assert_eq!(result.unwrap(), 2750000);
    }

    #[tokio::test]
    async fn test_simulation_calculates_speedup() {
        let speeds = vec![1000000, 1000000]; // Two mirrors at 1MB/s each
        let result = simulate_parallel_retry(speeds, 1024000, 2).await;
        assert!(result.is_ok());
        let sim = result.unwrap();
        // Aggregated: 2MB/s, single: 1MB/s, improvement: 200%
        assert_eq!(sim.aggregated_speed_bps, 2000000);
        assert_eq!(sim.improvement_vs_single, 200);
    }
}
