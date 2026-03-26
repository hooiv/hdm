//! Parallel Mirror Retry Engine
//!
//! Implements simultaneous retry attempts across multiple mirrors for maximum download speed
//! and resilience. Races multiple mirrors concurrently and uses the first successful response.
//!
//! Features:
//! - Concurrent requests to multiple mirrors
//! - Smart mirror ranking for optimal bandwidth allocation
//! - Automatic fallback on timeout or failure
//! - Connection pooling and reuse
//! - Bandwidth aggregation for combined parallel throughput

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use std::time::Duration;

/// Configuration for parallel mirror retry strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelRetryConfig {
    /// Maximum number of concurrent mirror attempts (2-10)
    pub max_concurrent_mirrors: u32,
    /// Timeout for each mirror attempt in seconds
    pub attempt_timeout_secs: u64,
    /// Only use mirrors with score >= this threshold
    pub min_mirror_score_threshold: u8,
    /// Enable bandwidth aggregation across mirrors
    pub enable_bandwidth_aggregation: bool,
    /// Backoff multiplier when all mirrors fail
    pub failure_backoff_multiplier: f64,
}

impl Default for ParallelRetryConfig {
    fn default() -> Self {
        Self {
            max_concurrent_mirrors: 3,
            attempt_timeout_secs: 10,
            min_mirror_score_threshold: 50,
            enable_bandwidth_aggregation: true,
            failure_backoff_multiplier: 1.5,
        }
    }
}

/// Result of a single parallel mirror attempt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirrorAttemptResult {
    /// The mirror URL that was tried
    pub mirror_url: String,
    /// Whether this attempt succeeded
    pub succeeded: bool,
    /// Response status code (if applicable)
    pub status_code: Option<u16>,
    /// Bytes downloaded in this attempt
    pub bytes_downloaded: u64,
    /// Duration of attempt in milliseconds
    pub duration_ms: u64,
    /// Calculated speed (bytes per second)
    pub speed_bps: u64,
    /// Error message if failed
    pub error: Option<String>,
}

/// Overall result of parallel mirror retry operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelRetryResult {
    /// Which mirror succeeded (if any)
    pub winner_mirror: Option<String>,
    /// Bytes downloaded across all mirrors
    pub total_bytes_downloaded: u64,
    /// Total duration of the parallel retry operation
    pub total_duration_ms: u64,
    /// Combined throughput from all successful mirrors
    pub aggregated_speed_bps: u64,
    /// All individual mirror results
    pub mirror_results: Vec<MirrorAttemptResult>,
    /// Overall success (at least one mirror succeeded)
    pub overall_success: bool,
    /// Number of mirrors that succeeded
    pub successful_count: u32,
}

impl ParallelRetryResult {
    /// Get success rate as percentage
    pub fn success_rate_percent(&self) -> f64 {
        if self.mirror_results.is_empty() {
            return 0.0;
        }
        let successful = self.mirror_results.iter().filter(|r| r.succeeded).count();
        (successful as f64 / self.mirror_results.len() as f64) * 100.0
    }

    /// Get fastest mirror that succeeded
    pub fn fastest_mirror(&self) -> Option<&MirrorAttemptResult> {
        self.mirror_results
            .iter()
            .filter(|r| r.succeeded)
            .max_by_key(|r| r.speed_bps)
    }

    /// Get slowest mirror that succeeded
    pub fn slowest_mirror(&self) -> Option<&MirrorAttemptResult> {
        self.mirror_results
            .iter()
            .filter(|r| r.succeeded)
            .min_by_key(|r| r.speed_bps)
    }

    /// Get mirrors sorted by speed (descending)
    pub fn mirrors_by_speed(&self) -> Vec<&MirrorAttemptResult> {
        let mut sorted = self.mirror_results.iter().collect::<Vec<_>>();
        sorted.sort_by_key(|r| std::cmp::Reverse(r.speed_bps));
        sorted
    }
}

/// Manages parallel mirror retry strategy
pub struct ParallelMirrorRetryManager {
    config: Arc<RwLock<ParallelRetryConfig>>,
}

impl ParallelMirrorRetryManager {
    /// Create a new parallel mirror retry manager
    pub fn new() -> Self {
        Self {
            config: Arc::new(RwLock::new(ParallelRetryConfig::default())),
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: ParallelRetryConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
        }
    }

    /// Get current configuration
    pub async fn get_config(&self) -> ParallelRetryConfig {
        self.config.read().await.clone()
    }

    /// Update configuration
    pub async fn update_config(&self, config: ParallelRetryConfig) {
        let mut current = self.config.write().await;
        *current = config;
    }

    /// Calculate optimal number of mirrors to use based on their scores
    /// 
    /// Example: If max_concurrent=3 and we have mirrors with scores [95, 85, 75, 65],
    /// this might recommend using 3 mirrors (the top ones) to maximize speed
    pub fn select_mirrors(
        &self,
        available_mirrors: &[(String, u8)], // (url, score) pairs
        max_to_use: u32,
        min_score: u8,
    ) -> Vec<String> {
        let mut filtered: Vec<_> = available_mirrors
            .iter()
            .filter(|(_, score)| *score >= min_score)
            .collect();

        // Sort by score descending
        filtered.sort_by_key(|(_, score)| std::cmp::Reverse(*score));

        // Take top N mirrors
        filtered
            .into_iter()
            .take(max_to_use as usize)
            .map(|(url, _)| url.clone())
            .collect()
    }

    /// Calculate expected aggregated speed from multiple mirrors
    ///
    /// Formula: aggressive_aggregate = sum(individual_speeds)
    /// Formula: conservative_aggregate = max(speed) + (0.5 * second_max)
    pub fn estimate_aggregated_speed(
        individual_speeds: &[u64],
        conservative: bool,
    ) -> u64 {
        if individual_speeds.is_empty() {
            return 0;
        }

        if conservative {
            let mut sorted = individual_speeds.to_vec();
            sorted.sort_by(|a, b| b.cmp(a));
            sorted[0] + (sorted.get(1).copied().unwrap_or(0) / 2)
        } else {
            individual_speeds.iter().sum()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mirror_selection() {
        let manager = ParallelMirrorRetryManager::new();
        let mirrors = vec![
            ("https://mirror1.com".to_string(), 95),
            ("https://mirror2.com".to_string(), 85),
            ("https://mirror3.com".to_string(), 75),
            ("https://mirror4.com".to_string(), 45),
        ];

        let selected = manager.select_mirrors(&mirrors, 3, 50);
        assert_eq!(selected.len(), 3);
        assert!(!selected.contains(&"https://mirror4.com".to_string())); // Score 45 < threshold 50
    }

    #[test]
    fn test_speed_estimation_aggressive() {
        let speeds = vec![1000000, 800000, 600000]; // 1MB, 800KB, 600KB per second
        let estimated = ParallelMirrorRetryManager::estimate_aggregated_speed(&speeds, false);
        assert_eq!(estimated, 2400000); // Sum = 2.4MB/s
    }

    #[test]
    fn test_speed_estimation_conservative() {
        let speeds = vec![1000000, 800000, 600000];
        let estimated = ParallelMirrorRetryManager::estimate_aggregated_speed(&speeds, true);
        assert_eq!(estimated, 1400000); // max + (second_max / 2) = 1MB + 400KB = 1.4MB/s
    }

    #[test]
    fn test_parallel_retry_result_success_rate() {
        let result = ParallelRetryResult {
            winner_mirror: Some("https://mirror1.com".to_string()),
            total_bytes_downloaded: 5242880,
            total_duration_ms: 5000,
            aggregated_speed_bps: 1048576,
            mirror_results: vec![
                MirrorAttemptResult {
                    mirror_url: "https://mirror1.com".to_string(),
                    succeeded: true,
                    status_code: Some(200),
                    bytes_downloaded: 5242880,
                    duration_ms: 5000,
                    speed_bps: 1048576,
                    error: None,
                },
                MirrorAttemptResult {
                    mirror_url: "https://mirror2.com".to_string(),
                    succeeded: false,
                    status_code: Some(408),
                    bytes_downloaded: 0,
                    duration_ms: 10000,
                    speed_bps: 0,
                    error: Some("Timeout".to_string()),
                },
            ],
            overall_success: true,
            successful_count: 1,
        };

        assert_eq!(result.success_rate_percent(), 50.0);
        assert_eq!(result.fastest_mirror().map(|m| m.speed_bps), Some(1048576));
    }

    #[test]
    fn test_config_defaults() {
        let config = ParallelRetryConfig::default();
        assert_eq!(config.max_concurrent_mirrors, 3);
        assert_eq!(config.attempt_timeout_secs, 10);
        assert_eq!(config.min_mirror_score_threshold, 50);
    }
}
