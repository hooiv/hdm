//! Download Speed Acceleration Engine
//!
//! Intelligently analyzes bandwidth patterns, predicts bottlenecks, and dynamically
//! optimizes segment downloads for maximum throughput.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::time::Duration;

/// Real-time bandwidth measurement
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BandwidthMeasurement {
    /// Bytes transferred in this measurement window
    pub bytes_transferred: u64,
    /// Duration of the measurement window
    pub duration_ms: u64,
    /// Calculated speed in bytes per second
    pub speed_bps: u64,
    /// Timestamp of measurement
    pub timestamp_secs: u64,
    /// Quality indicator (0-100, affected by jitter/variance)
    pub quality_score: u8,
}

/// Network condition state
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NetworkCondition {
    /// Excellent: >10 MB/s, low variance
    Excellent,
    /// Good: 5-10 MB/s, moderate variance
    Good,
    /// Fair: 1-5 MB/s, high variance
    Fair,
    /// Poor: <1 MB/s, very high variance
    Poor,
    /// Degrading: Speeds declining over time
    Degrading,
}

/// Segment optimization strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentStrategy {
    /// Recommended segment size in bytes
    pub optimal_segment_size: u64,
    /// Number of parallel connections to use
    pub parallel_connections: u32,
    /// Maximum segment queue depth
    pub queue_depth: u32,
    /// Prioritization order for segments
    pub priority_weights: Vec<f64>,
    /// Retry timeout in milliseconds
    pub retry_timeout_ms: u64,
    /// Whether to use aggressive caching
    pub use_caching: bool,
}

/// Bandwidth history and analytics
pub struct SpeedAccelerationEngine {
    /// Rolling window of bandwidth measurements
    measurements: VecDeque<BandwidthMeasurement>,
    /// Maximum history to keep (1000 measurements = ~16 mins at 1/sec)
    max_history: usize,
    /// Current network condition
    current_condition: NetworkCondition,
    /// Trend direction (-1=degrading, 0=stable, 1=improving)
    trend: i8,
}

impl SpeedAccelerationEngine {
    /// Create a new acceleration engine
    pub fn new() -> Self {
        Self {
            measurements: VecDeque::new(),
            max_history: 1000,
            current_condition: NetworkCondition::Good,
            trend: 0,
        }
    }

    /// Record a bandwidth measurement
    pub fn record_measurement(&mut self, measurement: BandwidthMeasurement) {
        self.measurements.push_back(measurement);

        // Keep history size under control
        if self.measurements.len() > self.max_history {
            self.measurements.pop_front();
        }

        // Update network condition
        self.update_condition();
    }

    /// Update network condition based on recent measurements
    fn update_condition(&mut self) {
        if self.measurements.is_empty() {
            return;
        }

        let recent = if self.measurements.len() > 100 {
            self.measurements.iter().rev().take(100).collect::<Vec<_>>()
        } else {
            self.measurements.iter().collect::<Vec<_>>()
        };

        let avg_speed: u64 = recent.iter().map(|m| m.speed_bps).sum::<u64>() / recent.len() as u64;
        let avg_quality: u8 = (recent.iter().map(|m| m.quality_score as u64).sum::<u64>()
            / recent.len() as u64) as u8;

        // Determine condition
        self.current_condition = match avg_speed {
            s if s > 10_000_000 && avg_quality > 85 => NetworkCondition::Excellent,
            s if s >= 5_000_000 && avg_quality > 70 => NetworkCondition::Good,
            s if s >= 1_000_000 && avg_quality > 50 => NetworkCondition::Fair,
            _ => NetworkCondition::Poor,
        };

        // Detect trend
        if recent.len() >= 10 {
            let old_avg = recent[0].speed_bps;
            let new_avg = recent[recent.len() - 1].speed_bps;

            if new_avg > old_avg * 105 / 100 {
                self.trend = 1; // Improving
            } else if new_avg < old_avg * 95 / 100 {
                self.trend = -1; // Degrading
            } else {
                self.trend = 0; // Stable
            }
        }
    }

    /// Get the optimal segment strategy based on current conditions
    pub fn get_optimal_strategy(&self) -> SegmentStrategy {
        match self.current_condition {
            NetworkCondition::Excellent => SegmentStrategy {
                optimal_segment_size: 10_000_000, // 10 MB
                parallel_connections: 8,
                queue_depth: 16,
                priority_weights: vec![1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
                retry_timeout_ms: 2000,
                use_caching: true,
            },
            NetworkCondition::Good => SegmentStrategy {
                optimal_segment_size: 5_000_000, // 5 MB
                parallel_connections: 6,
                queue_depth: 12,
                priority_weights: vec![1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
                retry_timeout_ms: 3000,
                use_caching: true,
            },
            NetworkCondition::Fair => SegmentStrategy {
                optimal_segment_size: 2_000_000, // 2 MB
                parallel_connections: 4,
                queue_depth: 8,
                priority_weights: vec![1.0, 1.0, 1.0, 1.0],
                retry_timeout_ms: 5000,
                use_caching: true,
            },
            NetworkCondition::Poor => SegmentStrategy {
                optimal_segment_size: 1_000_000, // 1 MB
                parallel_connections: 2,
                queue_depth: 4,
                priority_weights: vec![1.0, 1.0],
                retry_timeout_ms: 10000,
                use_caching: false,
            },
            NetworkCondition::Degrading => SegmentStrategy {
                optimal_segment_size: 512_000, // 512 KB
                parallel_connections: 1,
                queue_depth: 2,
                priority_weights: vec![1.0],
                retry_timeout_ms: 15000,
                use_caching: false,
            },
        }
    }

    /// Calculate expected download time
    pub fn estimate_download_time(&self, file_size_bytes: u64) -> Duration {
        if self.measurements.is_empty() {
            return Duration::from_secs(0);
        }

        let avg_speed: u64 = self
            .measurements
            .iter()
            .rev()
            .take(50)
            .map(|m| m.speed_bps)
            .sum::<u64>()
            / self.measurements.len().min(50) as u64;

        if avg_speed == 0 {
            return Duration::from_secs(0);
        }

        let seconds = file_size_bytes / avg_speed;
        Duration::from_secs(seconds)
    }

    /// Predict if speed will improve soon
    pub fn predict_improvement(&self) -> bool {
        // If trend is improving and we're not already excellent, improvement likely
        self.trend > 0 && self.current_condition != NetworkCondition::Excellent
    }

    /// Predict if speed will degrade soon
    pub fn predict_degradation(&self) -> bool {
        // If trend is degrading, prepare for worse conditions
        self.trend < 0 && self.current_condition != NetworkCondition::Poor
    }

    /// Get current network condition
    pub fn get_condition(&self) -> NetworkCondition {
        self.current_condition.clone()
    }

    /// Get average speed from last N measurements
    pub fn get_average_speed(&self, samples: usize) -> u64 {
        let effective_samples = samples.min(self.measurements.len());
        if effective_samples == 0 {
            return 0;
        }

        self.measurements
            .iter()
            .rev()
            .take(effective_samples)
            .map(|m| m.speed_bps)
            .sum::<u64>()
            / effective_samples as u64
    }

    /// Get speed variance (higher = less stable)
    pub fn get_speed_variance(&self, samples: usize) -> f64 {
        let effective_samples = samples.min(self.measurements.len());
        if effective_samples < 2 {
            return 0.0;
        }

        let recent: Vec<_> = self
            .measurements
            .iter()
            .rev()
            .take(effective_samples)
            .collect();

        let mean: u64 = recent.iter().map(|m| m.speed_bps).sum::<u64>() / effective_samples as u64;

        let variance: f64 = recent
            .iter()
            .map(|m| {
                let diff = m.speed_bps as f64 - mean as f64;
                diff * diff
            })
            .sum::<f64>()
            / effective_samples as f64;

        variance.sqrt()
    }

    /// Calculate quality score based on speed consistency
    fn calculate_quality_score(avg_speed: u64, variance: f64) -> u8 {
        let stability = if variance == 0.0 {
            100.0
        } else {
            (100.0 / (1.0 + (variance / avg_speed as f64))).min(100.0)
        };

        (stability as u8).max(0).min(100)
    }

    /// Get bandwidth measurements for analysis
    pub fn get_measurements(&self) -> Vec<BandwidthMeasurement> {
        self.measurements.iter().copied().collect()
    }

    /// Clear history
    pub fn clear_history(&mut self) {
        self.measurements.clear();
    }

    /// Get health score (0-100) based on network stability
    pub fn get_health_score(&self) -> u8 {
        let condition_score = match self.current_condition {
            NetworkCondition::Excellent => 100,
            NetworkCondition::Good => 80,
            NetworkCondition::Fair => 50,
            NetworkCondition::Poor => 20,
            NetworkCondition::Degrading => 10,
        };

        let stability_score = if self.trend > 0 {
            10
        } else if self.trend < 0 {
            -20
        } else {
            0
        };

        ((condition_score as i16 + stability_score).max(0) as u8).min(100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_engine() {
        let engine = SpeedAccelerationEngine::new();
        assert_eq!(engine.measurements.len(), 0);
        assert_eq!(engine.current_condition, NetworkCondition::Good);
    }

    #[test]
    fn test_record_and_update() {
        let mut engine = SpeedAccelerationEngine::new();
        let measurement = BandwidthMeasurement {
            bytes_transferred: 10_000_000,
            duration_ms: 1000,
            speed_bps: 10_000_000,
            timestamp_secs: 1000,
            quality_score: 90,
        };

        engine.record_measurement(measurement);
        assert_eq!(engine.measurements.len(), 1);
    }

    #[test]
    fn test_condition_detection() {
        let mut engine = SpeedAccelerationEngine::new();

        // Add high-speed measurements
        for i in 0..10 {
            engine.record_measurement(BandwidthMeasurement {
                bytes_transferred: 20_000_000,
                duration_ms: 1000,
                speed_bps: 20_000_000,
                timestamp_secs: 1000 + i,
                quality_score: 95,
            });
        }

        assert_eq!(engine.current_condition, NetworkCondition::Excellent);
    }

    #[test]
    fn test_strategy_selection() {
        let mut engine = SpeedAccelerationEngine::new();

        // Simulate poor network
        for i in 0..10 {
            engine.record_measurement(BandwidthMeasurement {
                bytes_transferred: 500_000,
                duration_ms: 1000,
                speed_bps: 500_000,
                timestamp_secs: 1000 + i,
                quality_score: 30,
            });
        }

        let strategy = engine.get_optimal_strategy();
        assert_eq!(strategy.parallel_connections, 2);
    }

    #[test]
    fn test_average_speed() {
        let mut engine = SpeedAccelerationEngine::new();

        for i in 0..5 {
            engine.record_measurement(BandwidthMeasurement {
                bytes_transferred: 5_000_000,
                duration_ms: 1000,
                speed_bps: 5_000_000,
                timestamp_secs: 1000 + i,
                quality_score: 80,
            });
        }

        let avg = engine.get_average_speed(5);
        assert_eq!(avg, 5_000_000);
    }

    #[test]
    fn test_health_score() {
        let mut engine = SpeedAccelerationEngine::new();

        // Excellent conditions
        for i in 0..20 {
            engine.record_measurement(BandwidthMeasurement {
                bytes_transferred: 15_000_000,
                duration_ms: 1000,
                speed_bps: 15_000_000,
                timestamp_secs: 1000 + i,
                quality_score: 95,
            });
        }

        let health = engine.get_health_score();
        assert!(health > 80);
    }
}
