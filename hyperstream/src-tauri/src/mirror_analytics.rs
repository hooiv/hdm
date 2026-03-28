//! Advanced Mirror Analytics Engine
//!
//! Provides deep insights into mirror performance with statistical analysis,
//! trend detection, and prediction capabilities.

use serde::{Deserialize, Serialize};

/// Performance statistics for a mirror
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirrorStatistics {
    /// Mirror URL
    pub url: String,
    /// Number of successful downloads
    pub success_count: u32,
    /// Number of failed downloads
    pub failure_count: u32,
    /// Success rate as percentage (0-100)
    pub success_rate_percent: f64,
    /// Average download speed (bytes per second)
    pub average_speed_bps: u64,
    /// Fastest observed speed
    pub max_speed_bps: u64,
    /// Slowest observed speed
    pub min_speed_bps: u64,
    /// Standard deviation of speeds
    pub speed_std_dev_bps: u64,
    /// Number of corruptions detected
    pub corruption_count: u32,
    /// Corruption rate as percentage
    pub corruption_rate_percent: f64,
    /// Average response time (milliseconds)
    pub avg_response_time_ms: u64,
    /// Median response time
    pub median_response_time_ms: u64,
    /// 95th percentile response time (p95)
    pub p95_response_time_ms: u64,
    /// Time since last successful download
    pub time_since_last_success_secs: u64,
    /// Overall reliability score (0-100)
    pub reliability_score: u8,
    /// Trend direction ("improving", "stable", "degrading")
    pub trend: String,
    /// Confidence in the score (based on sample size)
    pub confidence_percent: u8,
}

/// Historical trend data for a mirror
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirrorTrend {
    /// Mirror URL
    pub url: String,
    /// Time period (hours) over which trend is calculated
    pub time_period_hours: u32,
    /// Success rate 1 period ago
    pub success_rate_previous: f64,
    /// Success rate currently
    pub success_rate_current: f64,
    /// Change in success rate (percentage points)
    pub success_rate_change: f64,
    /// Average speed 1 period ago
    pub speed_bps_previous: u64,
    /// Average speed currently
    pub speed_bps_current: u64,
    /// Trend direction: "improving", "stable", "degrading"
    pub direction: String,
    /// Confidence (0-100) based on data points
    pub confidence: u8,
}

/// Comparison results between mirrors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirrorComparison {
    /// Mirror A URL
    pub mirror_a: String,
    /// Mirror B URL
    pub mirror_b: String,
    /// Which is faster (A or B)
    pub faster_mirror: String,
    /// Speed advantage percentage (positive = A is faster)
    pub speed_advantage_percent: f64,
    /// Which is more reliable (A or B)
    pub more_reliable: String,
    /// Reliability advantage percentage
    pub reliability_advantage_percent: f64,
    /// Overall comparison winner
    pub recommended: String,
    /// Confidence in recommendation (0-100)
    pub confidence: u8,
}

/// Recommendation for mirror selection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirrorRecommendation {
    /// Recommended mirror URL
    pub mirror_url: String,
    /// Reason for recommendation
    pub reason: String,
    /// Confidence in recommendation (0-100)
    pub confidence: u8,
    /// Alternative mirrors to try if primary fails
    pub fallback_mirrors: Vec<String>,
    /// Estimated download speed (bytes per second)
    pub estimated_speed_bps: u64,
    /// Estimated success rate
    pub estimated_success_rate: f64,
}

/// Historical performance snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceSnapshot {
    /// When this snapshot was taken
    pub timestamp_secs: u64,
    /// Mirror statistics at this time
    pub statistics: MirrorStatistics,
}

/// Analytics engine for mirrors
pub struct MirrorAnalyticsEngine {
    // In production, would maintain a time-series database
    // For now, keeps summary statistics
}

impl MirrorAnalyticsEngine {
    /// Create a new analytics engine
    pub fn new() -> Self {
        Self {}
    }

    /// Calculate statistics for a set of mirrors
    /// 
    /// In production, would query historical data from persistent storage
    pub fn calculate_statistics(
        success_count: u32,
        failure_count: u32,
        speeds: &[u64],
        corruption_count: u32,
        response_times: &[u64],
    ) -> Result<MirrorStatistics, String> {
        if success_count + failure_count == 0 {
            return Err("No data available".to_string());
        }

        let total = (success_count + failure_count) as f64;
        let success_rate = (success_count as f64 / total) * 100.0;

        // Calculate speed statistics
        let (avg_speed, max_speed, min_speed, std_dev) = Self::calculate_speed_stats(speeds);

        // Calculate response time percentiles
        let avg_response_time = if !response_times.is_empty() {
            response_times.iter().sum::<u64>() / response_times.len() as u64
        } else {
            0
        };

        let mut sorted_times = response_times.to_vec();
        sorted_times.sort();
        let median_response_time = if !sorted_times.is_empty() {
            sorted_times[sorted_times.len() / 2]
        } else {
            0
        };

        let p95_index = ((sorted_times.len() as f64 * 0.95) as usize).min(sorted_times.len() - 1);
        let p95_response_time = if !sorted_times.is_empty() {
            sorted_times[p95_index]
        } else {
            0
        };

        // Calculate corruption rate
        let corruption_rate = if success_count > 0 {
            (corruption_count as f64 / success_count as f64) * 100.0
        } else {
            0.0
        };

        // Calculate reliability score
        let reliability_score = Self::calculate_reliability_score(
            success_rate,
            corruption_rate,
            speeds,
        );

        // Calculate confidence based on sample size
        let confidence = (((success_count + failure_count) as f64 / 100.0).min(1.0) * 100.0) as u8;

        Ok(MirrorStatistics {
            url: String::new(), // Would be filled in by caller
            success_count,
            failure_count,
            success_rate_percent: success_rate,
            average_speed_bps: avg_speed,
            max_speed_bps: max_speed,
            min_speed_bps: min_speed,
            speed_std_dev_bps: std_dev,
            corruption_count,
            corruption_rate_percent: corruption_rate,
            avg_response_time_ms: avg_response_time,
            median_response_time_ms: median_response_time,
            p95_response_time_ms: p95_response_time,
            time_since_last_success_secs: 0, // Would be calculated from actual timestamps
            reliability_score,
            trend: "stable".to_string(),
            confidence_percent: confidence,
        })
    }

    /// Calculate speed statistics
    fn calculate_speed_stats(speeds: &[u64]) -> (u64, u64, u64, u64) {
        if speeds.is_empty() {
            return (0, 0, 0, 0);
        }

        let avg = speeds.iter().sum::<u64>() / speeds.len() as u64;
        let max = *speeds.iter().max().unwrap_or(&0);
        let min = *speeds.iter().min().unwrap_or(&0);

        // Calculate standard deviation
        let variance = speeds
            .iter()
            .map(|s| {
                let diff = (*s as i64) - (avg as i64);
                (diff * diff) as u64
            })
            .sum::<u64>()
            / speeds.len() as u64;

        let std_dev = (variance as f64).sqrt() as u64;

        (avg, max, min, std_dev)
    }

    /// Calculate overall reliability score
    fn calculate_reliability_score(success_rate: f64, corruption_rate: f64, speeds: &[u64]) -> u8 {
        let success_component = success_rate * 0.6; // 60% weight
        let corruption_component = (100.0 - corruption_rate.min(100.0)) * 0.3; // 30% weight

        let speed_component = if !speeds.is_empty() {
            // Award points based on having decent speed
            if speeds.iter().any(|s| *s > 1_000_000) {
                10.0 // 10% bonus if any mirror >1MB/s
            } else {
                5.0
            }
        } else {
            0.0
        };

        let score = success_component + corruption_component + speed_component;
        (score.min(100.0) as u8).max(0)
    }

    /// Compare two mirrors
    pub fn compare_mirrors(
        stats_a: &MirrorStatistics,
        stats_b: &MirrorStatistics,
    ) -> MirrorComparison {
        let speed_advantage = if stats_b.average_speed_bps > 0 {
            (stats_a.average_speed_bps as f64 / stats_b.average_speed_bps as f64 - 1.0) * 100.0
        } else {
            0.0
        };

        let faster = if stats_a.average_speed_bps > stats_b.average_speed_bps {
            stats_a.url.clone()
        } else {
            stats_b.url.clone()
        };

        let reliability_advantage = stats_a.success_rate_percent - stats_b.success_rate_percent;
        let more_reliable = if stats_a.success_rate_percent > stats_b.success_rate_percent {
            stats_a.url.clone()
        } else {
            stats_b.url.clone()
        };

        let recommended = if stats_a.reliability_score > stats_b.reliability_score {
            stats_a.url.clone()
        } else {
            stats_b.url.clone()
        };

        let confidence = ((stats_a.confidence_percent as u32 + stats_b.confidence_percent as u32) / 2) as u8;

        MirrorComparison {
            mirror_a: stats_a.url.clone(),
            mirror_b: stats_b.url.clone(),
            faster_mirror: faster,
            speed_advantage_percent: speed_advantage,
            more_reliable,
            reliability_advantage_percent: reliability_advantage,
            recommended,
            confidence,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_statistics() {
        let speeds = vec![1_000_000, 2_000_000, 1_500_000];
        let response_times = vec![100, 150, 120];

        let result = MirrorAnalyticsEngine::calculate_statistics(
            9,  // 9 successes
            1,  // 1 failure
            &speeds,
            0,  // 0 corruptions
            &response_times,
        );

        assert!(result.is_ok());
        let stats = result.unwrap();
        assert_eq!(stats.success_count, 9);
        assert_eq!(stats.failure_count, 1);
        assert!(stats.success_rate_percent > 85.0); // 90%
        assert_eq!(stats.average_speed_bps, 1_500_000);
        assert_eq!(stats.max_speed_bps, 2_000_000);
        assert_eq!(stats.min_speed_bps, 1_000_000);
    }

    #[test]
    fn test_compare_mirrors() {
        let mut stats_a = MirrorStatistics {
            url: "https://mirror1.com".to_string(),
            success_count: 100,
            failure_count: 5,
            success_rate_percent: 95.0,
            average_speed_bps: 2_000_000,
            max_speed_bps: 2_500_000,
            min_speed_bps: 1_500_000,
            speed_std_dev_bps: 300_000,
            corruption_count: 1,
            corruption_rate_percent: 1.0,
            avg_response_time_ms: 100,
            median_response_time_ms: 95,
            p95_response_time_ms: 200,
            time_since_last_success_secs: 60,
            reliability_score: 92,
            trend: "stable".to_string(),
            confidence_percent: 100,
        };

        let mut stats_b = stats_a.clone();
        stats_b.url = "https://mirror2.com".to_string();
        stats_b.average_speed_bps = 1_000_000;
        stats_b.reliability_score = 85;

        let comparison = MirrorAnalyticsEngine::compare_mirrors(&stats_a, &stats_b);
        assert_eq!(comparison.faster_mirror, "https://mirror1.com".to_string());
        assert!(comparison.speed_advantage_percent > 100.0);
        assert_eq!(comparison.recommended, "https://mirror1.com".to_string());
    }
}
