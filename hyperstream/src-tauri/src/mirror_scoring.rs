//! Production-Grade Mirror Reliability Scoring Engine
//!
//! This module implements a sophisticated mirror scoring system using Exponential Moving Average (EMA)
//! algorithms to track and predict mirror reliability patterns. It provides thread-safe access to
//! mirror metrics and enables intelligent mirror selection based on historical performance data.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use lazy_static::lazy_static;

/// Represents comprehensive metrics for a mirror source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirrorMetrics {
    /// The URL of the mirror
    pub url: String,
    /// Reliability score (0-100) based on success/failure history
    pub reliability_score: f64,
    /// Speed score (0-100) based on average latency
    pub speed_score: f64,
    /// Uptime percentage based on success/failure counts
    pub uptime_percentage: f64,
    /// Total successful downloads
    pub success_count: u32,
    /// Total failed downloads
    pub failure_count: u32,
    /// Average latency in milliseconds
    pub avg_latency_ms: f64,
    /// Timestamp of last update (milliseconds)
    pub last_updated_ms: u64,
    /// Risk level classification ("Healthy", "Caution", "Warning", "Critical")
    pub risk_level: String,
}

/// Thread-safe mirror scoring engine using EMA algorithms
pub struct MirrorScorer {
    metrics: Arc<RwLock<HashMap<String, MirrorMetrics>>>,
}

impl MirrorScorer {
    /// Create a new MirrorScorer instance
    pub fn new() -> Self {
        MirrorScorer {
            metrics: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Record a successful download and update EMA scores
    ///
    /// Uses EMA formula: new_score = (0.7 * old_score) + (0.3 * 100)
    /// This gives more weight to recent success while maintaining historical context.
    pub fn record_success(&self, url: &str, latency_ms: f64) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let mut metrics = self.metrics.write().unwrap();
        let entry = metrics
            .entry(url.to_string())
            .or_insert_with(|| MirrorMetrics {
                url: url.to_string(),
                reliability_score: 50.0, // Start neutral
                speed_score: 50.0,
                uptime_percentage: 0.0,
                success_count: 0,
                failure_count: 0,
                avg_latency_ms: 0.0,
                last_updated_ms: now,
                risk_level: "Caution".to_string(),
            });

        // Update counts
        entry.success_count += 1;

        // EMA update for reliability: success indicator = 100
        entry.reliability_score = (0.7 * entry.reliability_score) + (0.3 * 100.0);

        // Update latency (simple average)
        let total_requests = (entry.success_count + entry.failure_count) as f64;
        entry.avg_latency_ms = (entry.avg_latency_ms * (total_requests - 1.0) + latency_ms) / total_requests;

        // Recalculate derived metrics
        self.update_derived_metrics(entry, now);
    }

    /// Record a failed download and update EMA scores
    ///
    /// Uses EMA formula: new_score = (0.7 * old_score) + (0.3 * 0)
    /// This gradually reduces the score while preserving historical context.
    pub fn record_failure(&self, url: &str) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let mut metrics = self.metrics.write().unwrap();
        let entry = metrics
            .entry(url.to_string())
            .or_insert_with(|| MirrorMetrics {
                url: url.to_string(),
                reliability_score: 50.0,
                speed_score: 50.0,
                uptime_percentage: 0.0,
                success_count: 0,
                failure_count: 0,
                avg_latency_ms: 0.0,
                last_updated_ms: now,
                risk_level: "Caution".to_string(),
            });

        // Update counts
        entry.failure_count += 1;

        // EMA update for reliability: failure indicator = 0
        entry.reliability_score = (0.7 * entry.reliability_score) + (0.3 * 0.0);

        // Recalculate derived metrics
        self.update_derived_metrics(entry, now);
    }

    /// Update speed score and uptime metrics
    fn update_derived_metrics(&self, entry: &mut MirrorMetrics, now: u64) {
        // Calculate speed score based on latency
        // Formula: (100 * (1 - latency/1000)).clamp(0, 100)
        entry.speed_score = (100.0 * (1.0 - (entry.avg_latency_ms / 1000.0)))
            .max(0.0)
            .min(100.0);

        // Calculate uptime percentage
        let total = entry.success_count as f64 + entry.failure_count as f64;
        if total > 0.0 {
            entry.uptime_percentage = (entry.success_count as f64 / total) * 100.0;
        }

        // Update risk level based on reliability score
        entry.risk_level = Self::calculate_risk_level(entry.reliability_score);

        // Update timestamp
        entry.last_updated_ms = now;
    }

    /// Calculate risk level based on score thresholds
    fn calculate_risk_level(score: f64) -> String {
        match score {
            s if s >= 90.0 => "Healthy".to_string(),
            s if s >= 75.0 => "Caution".to_string(),
            s if s >= 60.0 => "Warning".to_string(),
            _ => "Critical".to_string(),
        }
    }

    /// Get metrics for a specific mirror
    pub fn get_mirror_score(&self, url: &str) -> Option<MirrorMetrics> {
        let metrics = self.metrics.read().unwrap();
        metrics.get(url).cloned()
    }

    /// Rank all mirrors by reliability score (highest first)
    pub fn rank_mirrors(&self) -> Vec<MirrorMetrics> {
        let metrics = self.metrics.read().unwrap();
        let mut ranked: Vec<_> = metrics.values().cloned().collect();
        ranked.sort_by(|a, b| {
            b.reliability_score
                .partial_cmp(&a.reliability_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        ranked
    }

    /// Get all mirror metrics
    pub fn get_all_metrics(&self) -> Vec<MirrorMetrics> {
        let metrics = self.metrics.read().unwrap();
        metrics.values().cloned().collect()
    }

    /// Reset metrics for a specific mirror
    pub fn reset_mirror(&self, url: &str) {
        let mut metrics = self.metrics.write().unwrap();
        metrics.remove(url);
    }

    /// Clear all metrics
    pub fn clear_all(&self) {
        let mut metrics = self.metrics.write().unwrap();
        metrics.clear();
    }
}

impl Default for MirrorScorer {
    fn default() -> Self {
        Self::new()
    }
}

// Global instance for use throughout the application
lazy_static! {
    pub static ref GLOBAL_MIRROR_SCORER: MirrorScorer = MirrorScorer::new();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    /// Test 1: Initial neutral score and risk level assignment
    #[test]
    fn test_initial_mirror_metrics() {
        let scorer = MirrorScorer::new();
        scorer.record_success("http://mirror1.example.com", 100.0);

        let metrics = scorer.get_mirror_score("http://mirror1.example.com").unwrap();
        assert_eq!(metrics.url, "http://mirror1.example.com");
        assert_eq!(metrics.success_count, 1);
        assert_eq!(metrics.failure_count, 0);
        assert!(metrics.reliability_score > 50.0); // Should increase from initial neutral
    }

    /// Test 2: EMA algorithm for success (0.7 * old + 0.3 * 100)
    #[test]
    fn test_ema_success_scoring() {
        let scorer = MirrorScorer::new();

        // First success: 50 -> (0.7 * 50) + (0.3 * 100) = 35 + 30 = 65
        scorer.record_success("http://test.com", 50.0);
        let metrics1 = scorer.get_mirror_score("http://test.com").unwrap();
        assert!(
            (metrics1.reliability_score - 65.0).abs() < 0.1,
            "Expected ~65.0, got {}",
            metrics1.reliability_score
        );

        // Second success: 65 -> (0.7 * 65) + (0.3 * 100) = 45.5 + 30 = 75.5
        scorer.record_success("http://test.com", 50.0);
        let metrics2 = scorer.get_mirror_score("http://test.com").unwrap();
        assert!(
            (metrics2.reliability_score - 75.5).abs() < 0.1,
            "Expected ~75.5, got {}",
            metrics2.reliability_score
        );
    }

    /// Test 3: EMA algorithm for failure (0.7 * old + 0.3 * 0)
    #[test]
    fn test_ema_failure_scoring() {
        let scorer = MirrorScorer::new();
        let url = "http://failing.com";

        // Build up a score first
        for _ in 0..3 {
            scorer.record_success(url, 50.0);
        }
        let score_before = scorer.get_mirror_score(url).unwrap().reliability_score;

        // Record failure: new = 0.7 * old + 0.3 * 0
        scorer.record_failure(url);
        let metrics = scorer.get_mirror_score(url).unwrap();
        let expected = 0.7 * score_before;
        assert!(
            (metrics.reliability_score - expected).abs() < 0.1,
            "Expected ~{}, got {}",
            expected,
            metrics.reliability_score
        );
    }

    /// Test 4: Risk level classification based on scores
    #[test]
    fn test_risk_level_assignment() {
        let scorer = MirrorScorer::new();

        // Build a healthy mirror (>= 90)
        for _ in 0..10 {
            scorer.record_success("http://healthy.com", 50.0);
        }
        let healthy = scorer.get_mirror_score("http://healthy.com").unwrap();
        assert_eq!(healthy.risk_level, "Healthy");

        // Build a failing mirror (< 60)
        let failing = "http://failing.com";
        scorer.record_success(failing, 50.0);
        for _ in 0..5 {
            scorer.record_failure(failing);
        }
        let metrics = scorer.get_mirror_score(failing).unwrap();
        assert_eq!(metrics.risk_level, "Critical");
    }

    /// Test 5: Speed score calculation based on latency
    #[test]
    fn test_speed_score_calculation() {
        let scorer = MirrorScorer::new();

        // 100ms latency: 100 * (1 - 0.1) = 90
        scorer.record_success("http://fast.com", 100.0);
        let fast = scorer.get_mirror_score("http://fast.com").unwrap();
        assert!(
            (fast.speed_score - 90.0).abs() < 0.1,
            "Expected ~90, got {}",
            fast.speed_score
        );

        // 1000ms latency: 100 * (1 - 1.0) = 0 (min clamp)
        scorer.record_success("http://slow.com", 1000.0);
        let slow = scorer.get_mirror_score("http://slow.com").unwrap();
        assert!(slow.speed_score >= 0.0 && slow.speed_score <= 0.1);
    }

    /// Test 6: Uptime percentage calculation
    #[test]
    fn test_uptime_percentage() {
        let scorer = MirrorScorer::new();
        let url = "http://uptime.com";

        // 3 successes, 1 failure = 75% uptime
        scorer.record_success(url, 50.0);
        scorer.record_success(url, 50.0);
        scorer.record_success(url, 50.0);
        scorer.record_failure(url);

        let metrics = scorer.get_mirror_score(url).unwrap();
        assert!(
            (metrics.uptime_percentage - 75.0).abs() < 0.1,
            "Expected ~75.0%, got {}%",
            metrics.uptime_percentage
        );
        assert_eq!(metrics.success_count, 3);
        assert_eq!(metrics.failure_count, 1);
    }

    /// Test 7: Rank mirrors by reliability score
    #[test]
    fn test_rank_mirrors() {
        let scorer = MirrorScorer::new();

        // Create multiple mirrors with different reliability
        scorer.record_success("http://mirror1.com", 50.0);
        for _ in 0..2 {
            scorer.record_success("http://mirror1.com", 50.0);
        }

        scorer.record_success("http://mirror2.com", 50.0);

        for _ in 0..3 {
            scorer.record_failure("http://mirror3.com");
        }

        let ranked = scorer.rank_mirrors();
        assert_eq!(ranked.len(), 3);
        // Mirror with more successes should rank higher
        assert!(ranked[0].reliability_score >= ranked[1].reliability_score);
    }

    /// Test 8: Thread-safe concurrent access
    #[test]
    fn test_thread_safety() {
        let scorer = Arc::new(MirrorScorer::new());
        let mut handles = vec![];

        for i in 0..5 {
            let scorer_clone = Arc::clone(&scorer);
            let handle = thread::spawn(move || {
                let url = format!("http://mirror{}.com", i);
                for _ in 0..10 {
                    scorer_clone.record_success(&url, 50.0);
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let all_metrics = scorer.get_all_metrics();
        assert_eq!(all_metrics.len(), 5);
        for metrics in all_metrics {
            assert_eq!(metrics.success_count, 10);
        }
    }

    /// Test 9: Get all metrics returns complete list
    #[test]
    fn test_get_all_metrics() {
        let scorer = MirrorScorer::new();

        scorer.record_success("http://mirror1.com", 50.0);
        scorer.record_success("http://mirror2.com", 50.0);
        scorer.record_failure("http://mirror3.com");

        let all = scorer.get_all_metrics();
        assert_eq!(all.len(), 3);

        let urls: Vec<_> = all.iter().map(|m| m.url.as_str()).collect();
        assert!(urls.contains(&"http://mirror1.com"));
        assert!(urls.contains(&"http://mirror2.com"));
        assert!(urls.contains(&"http://mirror3.com"));
    }

    /// Test 10: Global scorer instance
    #[test]
    fn test_global_scorer_instance() {
        GLOBAL_MIRROR_SCORER.record_success("http://global.com", 50.0);
        let metrics = GLOBAL_MIRROR_SCORER
            .get_mirror_score("http://global.com")
            .unwrap();
        assert_eq!(metrics.url, "http://global.com");
    }
}
