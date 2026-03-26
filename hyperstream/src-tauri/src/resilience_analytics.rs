// resilience_analytics.rs — Advanced monitoring, analytics, and proactive resilience
//
// Provides machine learning-ready analytics, predictive failure detection,
// and recommendation engine for optimizing download reliability

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Download reliability metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadMetrics {
    pub download_id: String,
    pub total_bytes_downloaded: u64,
    pub total_bytes_lost: u64,
    pub total_retries: u32,
    pub average_speed_bps: u64,
    pub peak_speed_bps: u64,
    pub failure_rate: f32,  // 0.0 to 1.0
    pub recovery_rate: f32, // 0.0 to 1.0
    pub estimated_completion_time_ms: u64,
}

/// Predictive failure prediction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailurePrediction {
    pub download_id: String,
    pub failure_probability: f32, // 0.0 to 1.0
    pub reason: String,
    pub predicted_failure_time_ms: Option<u64>,
    pub confidence: f32,
    pub timestamp: u64,
}

/// Optimization recommendation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationRecommendation {
    pub download_id: String,
    pub recommendation_type: RecommendationType,
    pub description: String,
    pub expected_improvement_percent: f32,
    pub priority: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RecommendationType {
    ReduceSegments,
    IncreaseSegments,
    EnableCompression,
    UseSmallChunks,
    EnableProxy,
    SwitchServer,
    ReduceBandwidth,
    EnableRetry,
    DisablePipelining,
}

/// Performance trend data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceTrend {
    pub metric_name: String,
    pub values: Vec<(u64, f64)>, // (timestamp, value)
    pub trend_direction: TrendDirection,
    pub volatility: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TrendDirection {
    Improving,
    Degrading,
    Stable,
    Unstable,
}

/// Analytics and monitoring engine
pub struct ResilienceAnalytics {
    metrics: Arc<RwLock<HashMap<String, DownloadMetrics>>>,
    #[allow(dead_code)]
    predictions: Arc<RwLock<Vec<FailurePrediction>>>,
    #[allow(dead_code)]
    recommendations: Arc<RwLock<Vec<OptimizationRecommendation>>>,
    #[allow(dead_code)]
    trends: Arc<RwLock<HashMap<String, PerformanceTrend>>>,
    speed_history: Arc<RwLock<HashMap<String, VecDeque<(u64, u64)>>>>, // (timestamp, speed)
    max_samples: usize,
}

impl ResilienceAnalytics {
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(RwLock::new(HashMap::new())),
            predictions: Arc::new(RwLock::new(Vec::new())),
            recommendations: Arc::new(RwLock::new(Vec::new())),
            trends: Arc::new(RwLock::new(HashMap::new())),
            speed_history: Arc::new(RwLock::new(HashMap::new())),
            max_samples: 1000,
        }
    }

    /// Update metrics for a download
    pub fn record_metric(
        &self,
        download_id: &str,
        bytes_downloaded: u64,
        bytes_lost: u64,
        retries: u32,
        current_speed_bps: u64,
    ) {
        let mut metrics = self.metrics.write().unwrap();
        let metric = metrics
            .entry(download_id.to_string())
            .or_insert_with(|| DownloadMetrics {
                download_id: download_id.to_string(),
                total_bytes_downloaded: 0,
                total_bytes_lost: 0,
                total_retries: 0,
                average_speed_bps: 0,
                peak_speed_bps: 0,
                failure_rate: 0.0,
                recovery_rate: 0.0,
                estimated_completion_time_ms: 0,
            });

        metric.total_bytes_downloaded = bytes_downloaded;
        metric.total_bytes_lost = bytes_lost;
        metric.total_retries = retries;
        metric.peak_speed_bps = metric.peak_speed_bps.max(current_speed_bps);

        // Update speed history for trend analysis
        let mut history = self.speed_history.write().unwrap();
        let speeds = history
            .entry(download_id.to_string())
            .or_insert_with(|| VecDeque::new());

        if speeds.len() >= self.max_samples {
            speeds.pop_front();
        }
        speeds.push_back((current_timestamp_ms(), current_speed_bps));

        // Calculate average speed
        if !speeds.is_empty() {
            metric.average_speed_bps = speeds.iter().map(|(_, s)| s).sum::<u64>() / speeds.len() as u64;
        }
    }

    /// Predict failure for a download based on metrics
    pub fn predict_failure(&self, download_id: &str) -> Option<FailurePrediction> {
        let metrics = self.metrics.read().unwrap();
        let metric = metrics.get(download_id)?;

        let mut probability = 0.0;
        let mut reasons = Vec::new();

        // High failure rate
        if metric.failure_rate > 0.3 {
            probability += 0.4 * metric.failure_rate;
            reasons.push("High failure rate observed".to_string());
        }

        // Excessive retries
        if metric.total_retries > 20 {
            probability += 0.2;
            reasons.push("Excessive retries indicate unstable connection".to_string());
        }

        // Very slow speed
        if metric.average_speed_bps > 0 && metric.average_speed_bps < 10_000 { // < 10 KB/s
            probability += 0.1;
            reasons.push("Very slow download speed".to_string());
        }

        // Low recovery rate
        if metric.recovery_rate < 0.5 && metric.total_retries > 5 {
            probability += 0.2;
            reasons.push("Poor recovery rate".to_string());
        }

        if probability > 0.1 {
            Some(FailurePrediction {
                download_id: download_id.to_string(),
                failure_probability: probability.min(1.0),
                reason: reasons.join("; "),
                predicted_failure_time_ms: if metric.estimated_completion_time_ms > 0 {
                    Some(current_timestamp_ms() + (metric.estimated_completion_time_ms / 3))
                } else {
                    None
                },
                confidence: (reasons.len() as f32 / 4.0).min(1.0),
                timestamp: current_timestamp_ms(),
            })
        } else {
            None
        }
    }

    /// Generate optimization recommendations
    pub fn generate_recommendations(&self, download_id: &str) -> Vec<OptimizationRecommendation> {
        let mut recs = Vec::new();
        let metrics = self.metrics.read().unwrap();

        if let Some(metric) = metrics.get(download_id) {
            let mut priority = 10;

            // Check failure rate
            if metric.failure_rate > 0.2 {
                priority -= 1;
                recs.push(OptimizationRecommendation {
                    download_id: download_id.to_string(),
                    recommendation_type: RecommendationType::ReduceSegments,
                    description: "High failure rate - reduce concurrent segments".to_string(),
                    expected_improvement_percent: 30.0,
                    priority,
                });
            }

            // Check if too few retries are configured
            if metric.total_retries == 0 && metric.failure_rate > 0.0 {
                recs.push(OptimizationRecommendation {
                    download_id: download_id.to_string(),
                    recommendation_type: RecommendationType::EnableRetry,
                    description: "Enable automatic retries for better reliability".to_string(),
                    expected_improvement_percent: 20.0,
                    priority: priority + 1,
                });
            }

            // Check speed
            if metric.average_speed_bps > 0 && metric.average_speed_bps < 50_000 {
                recs.push(OptimizationRecommendation {
                    download_id: download_id.to_string(),
                    recommendation_type: RecommendationType::IncreaseSegments,
                    description: "Low speed - try increasing segments for parallelism".to_string(),
                    expected_improvement_percent: 25.0,
                    priority: priority + 2,
                });
            }

            // Check for data loss
            if metric.total_bytes_lost > 0 {
                recs.push(OptimizationRecommendation {
                    download_id: download_id.to_string(),
                    recommendation_type: RecommendationType::UseSmallChunks,
                    description: "Data loss detected - use smaller chunks".to_string(),
                    expected_improvement_percent: 40.0,
                    priority: priority - 1,
                });
            }
        }

        recs
    }

    /// Analyze trends in performance
    pub fn analyze_trends(&self, download_id: &str) -> Vec<PerformanceTrend> {
        let mut trends = Vec::new();

        if let Some(speeds) = self.speed_history.read().unwrap().get(download_id) {
            if speeds.len() > 5 {
                let recent: Vec<u64> = speeds.iter().rev().take(10).map(|(_, s)| *s).collect();

                let avg = recent.iter().sum::<u64>() / recent.len() as u64;
                let variance: f32 = recent
                    .iter()
                    .map(|s| {
                        let diff = *s as i64 - avg as i64;
                        (diff * diff) as f32
                    })
                    .sum::<f32>()
                    / recent.len() as f32;
                let volatility = variance.sqrt() / (avg.max(1) as f32);

                let trend_direction = if recent.len() >= 2 {
                    let first_half: u64 = recent[..recent.len() / 2].iter().sum();
                    let second_half: u64 = recent[recent.len() / 2..].iter().sum();

                    let first_avg = first_half / (recent.len() / 2) as u64;
                    let second_avg = second_half / ((recent.len() + 1) / 2) as u64;

                    if second_avg > first_avg + (first_avg / 5) {
                        TrendDirection::Improving
                    } else if second_avg < first_avg.saturating_sub(first_avg / 5) {
                        TrendDirection::Degrading
                    } else if volatility > 0.5 {
                        TrendDirection::Unstable
                    } else {
                        TrendDirection::Stable
                    }
                } else {
                    TrendDirection::Stable
                };

                trends.push(PerformanceTrend {
                    metric_name: "download_speed".to_string(),
                    values: speeds.iter().map(|(t, s)| (*t, *s as f64)).collect(),
                    trend_direction,
                    volatility,
                });
            }
        }

        trends
    }

    /// Get all current predictions
    pub fn get_all_predictions(&self) -> Vec<FailurePrediction> {
        let metrics = self.metrics.read().unwrap();
        metrics
            .keys()
            .filter_map(|id| self.predict_failure(id))
            .collect()
    }

    /// Get high-risk downloads
    pub fn get_high_risk_downloads(&self, threshold: f32) -> Vec<(String, f32)> {
        self.get_all_predictions()
            .into_iter()
            .filter(|p| p.failure_probability >= threshold)
            .map(|p| (p.download_id, p.failure_probability))
            .collect()
    }

    /// Get analytics report
    pub fn get_report(&self) -> AnalyticsReport {
        let metrics = self.metrics.read().unwrap();
        let predictions = self.get_all_predictions();
        let high_risk = self.get_high_risk_downloads(0.5);

        AnalyticsReport {
            total_downloads_tracked: metrics.len(),
            high_risk_downloads: high_risk.len(),
            average_failure_rate: if metrics.is_empty() {
                0.0
            } else {
                metrics.values().map(|m| m.failure_rate).sum::<f32>() / metrics.len() as f32
            },
            average_recovery_rate: if metrics.is_empty() {
                0.0
            } else {
                metrics.values().map(|m| m.recovery_rate).sum::<f32>() / metrics.len() as f32
            },
            predictions_count: predictions.len(),
            total_bytes_recovered: metrics.values().map(|m| m.total_bytes_downloaded).sum(),
            timestamp: current_timestamp_ms(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsReport {
    pub total_downloads_tracked: usize,
    pub high_risk_downloads: usize,
    pub average_failure_rate: f32,
    pub average_recovery_rate: f32,
    pub predictions_count: usize,
    pub total_bytes_recovered: u64,
    pub timestamp: u64,
}

fn current_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_recording() {
        let analytics = ResilienceAnalytics::new();
        analytics.record_metric("dl1", 1000, 0, 0, 100_000);

        let report = analytics.get_report();
        assert_eq!(report.total_downloads_tracked, 1);
    }

    #[test]
    fn test_failure_prediction() {
        let analytics = ResilienceAnalytics::new();

        // High failure rate
        for i in 0..5 {
            analytics.record_metric("dl1", 100 * i, 50 * i, (10 + i) as u32, 5_000);
        }

        if let Some(pred) = analytics.predict_failure("dl1") {
            assert!(pred.failure_probability > 0.0);
        }
    }

    #[test]
    fn test_recommendations() {
        let analytics = ResilienceAnalytics::new();
        analytics.record_metric("dl1", 1000, 100, 20, 5_000);

        let recs = analytics.generate_recommendations("dl1");
        assert!(!recs.is_empty());
    }
}
