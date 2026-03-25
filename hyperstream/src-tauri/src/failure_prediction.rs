use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Represents a failure pattern for a given URL with historical data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailurePattern {
    pub url: String,
    pub failure_rate: f64,
    pub timeout_count: u32,
    pub corruption_count: u32,
    pub rate_limit_count: u32,
    pub avg_failure_time_sec: f64,
}

impl FailurePattern {
    /// Create a new failure pattern for a URL
    pub fn new(url: String) -> Self {
        Self {
            url,
            failure_rate: 0.0,
            timeout_count: 0,
            corruption_count: 0,
            rate_limit_count: 0,
            avg_failure_time_sec: 0.0,
        }
    }

    /// Initialize a pattern with a given failure rate (for testing/initialization)
    pub fn with_rate(url: String, failure_rate: f64) -> Self {
        let mut pattern = Self::new(url);
        pattern.failure_rate = failure_rate.clamp(0.0, 100.0);
        pattern
    }
}

/// Thread-safe failure predictor that tracks and predicts download failures
pub struct FailurePredictor {
    patterns: Arc<RwLock<HashMap<String, FailurePattern>>>,
}

impl FailurePredictor {
    /// Create a new failure predictor
    pub fn new() -> Self {
        Self {
            patterns: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Record a failure for a given URL
    pub fn record_failure(&self, url: &str, failure_type: FailureType) {
        let mut patterns = self.patterns.write().unwrap();
        let pattern = patterns
            .entry(url.to_string())
            .or_insert_with(|| FailurePattern::new(url.to_string()));

        // Update failure rate based on historical data
        // For simplicity, increment by 10% for each failure recorded
        pattern.failure_rate = (pattern.failure_rate + 10.0).clamp(0.0, 100.0);

        match failure_type {
            FailureType::Timeout => pattern.timeout_count += 1,
            FailureType::Corruption => pattern.corruption_count += 1,
            FailureType::RateLimit => pattern.rate_limit_count += 1,
        }
    }

    /// Predict the failure risk for a segment download
    /// 
    /// Algorithm:
    /// - risk = base_failure_rate
    /// - risk *= (1.0 + segment_size / 10_000_000)  // Size factor (larger = higher risk)
    /// - risk *= 0.8 if is_resume                   // Resume reduces risk by 20%
    /// - risk = clamp(risk, 0, 100)
    pub fn predict_failure_risk(
        &self,
        url: &str,
        segment_size_bytes: u32,
        is_resume: bool,
    ) -> f64 {
        let patterns = self.patterns.read().unwrap();

        // Get base failure rate for URL, default to 30% for new mirrors
        let base_failure_rate = patterns
            .get(url)
            .map(|p| p.failure_rate)
            .unwrap_or(30.0);

        // Start with base rate
        let mut risk = base_failure_rate;

        // Apply size factor: larger segments have higher risk
        let size_factor = 1.0 + (segment_size_bytes as f64) / 10_000_000.0;
        risk *= size_factor;

        // Resume reduces risk by 20%
        if is_resume {
            risk *= 0.8;
        }

        // Clamp to valid percentage range
        risk.clamp(0.0, 100.0)
    }

    /// Get all recorded failure patterns
    pub fn get_patterns(&self) -> Vec<FailurePattern> {
        let patterns = self.patterns.read().unwrap();
        patterns.values().cloned().collect()
    }

    /// Clear all failure patterns (useful for testing)
    pub fn clear(&self) {
        let mut patterns = self.patterns.write().unwrap();
        patterns.clear();
    }

    /// Get the failure pattern for a specific URL
    pub fn get_pattern(&self, url: &str) -> Option<FailurePattern> {
        let patterns = self.patterns.read().unwrap();
        patterns.get(url).cloned()
    }
}

impl Default for FailurePredictor {
    fn default() -> Self {
        Self::new()
    }
}

/// Types of failures that can be recorded
#[derive(Debug, Clone, Copy)]
pub enum FailureType {
    Timeout,
    Corruption,
    RateLimit,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_mirror_has_low_risk() {
        let predictor = FailurePredictor::new();
        let risk = predictor.predict_failure_risk("https://example.com/file.iso", 1_000_000, false);

        // New mirror should have ~30% base risk
        assert!(risk >= 25.0 && risk <= 35.0, "New mirror risk should be ~30%, got {}", risk);
    }

    #[test]
    fn test_failed_mirror_has_high_risk() {
        let predictor = FailurePredictor::new();
        let url = "https://failed-mirror.com/file.zip";

        // Record multiple failures to increase risk
        predictor.record_failure(url, FailureType::Timeout);
        predictor.record_failure(url, FailureType::Corruption);
        predictor.record_failure(url, FailureType::RateLimit);

        let risk = predictor.predict_failure_risk(url, 1_000_000, false);

        // Failed mirror should have >50% risk after 3 failures
        assert!(risk > 50.0, "Failed mirror risk should be >50%, got {}", risk);
    }

    #[test]
    fn test_resume_reduces_risk_by_twenty_percent() {
        let predictor = FailurePredictor::new();
        let url = "https://example.com/large-file.iso";

        // Record some failures
        predictor.record_failure(url, FailureType::Timeout);

        let risk_no_resume = predictor.predict_failure_risk(url, 1_000_000, false);
        let risk_with_resume = predictor.predict_failure_risk(url, 1_000_000, true);

        // Resume should reduce risk by 20%
        let reduction_factor = risk_with_resume / risk_no_resume;
        assert!(
            (reduction_factor - 0.8).abs() < 0.01,
            "Resume should reduce risk by 20% (factor 0.8), got factor {}",
            reduction_factor
        );
    }

    #[test]
    fn test_large_segments_increase_risk() {
        let predictor = FailurePredictor::new();
        let url = "https://example.com/file.bin";

        let small_segment_risk = predictor.predict_failure_risk(url, 1_000_000, false);
        let large_segment_risk = predictor.predict_failure_risk(url, 50_000_000, false);

        // Larger segments should have higher risk
        assert!(
            large_segment_risk > small_segment_risk,
            "Large segments ({}) should have higher risk than small ({}) segments",
            large_segment_risk,
            small_segment_risk
        );
    }

    #[test]
    fn test_record_failure_updates_pattern() {
        let predictor = FailurePredictor::new();
        let url = "https://example.com/test.exe";

        assert_eq!(predictor.get_patterns().len(), 0);

        predictor.record_failure(url, FailureType::Timeout);

        assert_eq!(predictor.get_patterns().len(), 1);
        let pattern = predictor.get_pattern(url).unwrap();
        assert_eq!(pattern.timeout_count, 1);
        assert_eq!(pattern.corruption_count, 0);
        assert_eq!(pattern.rate_limit_count, 0);
    }

    #[test]
    fn test_multiple_failure_types() {
        let predictor = FailurePredictor::new();
        let url = "https://example.com/multi-fail.iso";

        predictor.record_failure(url, FailureType::Timeout);
        predictor.record_failure(url, FailureType::Timeout);
        predictor.record_failure(url, FailureType::Corruption);
        predictor.record_failure(url, FailureType::RateLimit);

        let pattern = predictor.get_pattern(url).unwrap();
        assert_eq!(pattern.timeout_count, 2);
        assert_eq!(pattern.corruption_count, 1);
        assert_eq!(pattern.rate_limit_count, 1);
    }

    #[test]
    fn test_risk_clamping() {
        let predictor = FailurePredictor::new();
        let url = "https://example.com/huge.iso";

        // Record many failures to push risk past 100%
        for _ in 0..20 {
            predictor.record_failure(url, FailureType::Timeout);
        }

        let risk = predictor.predict_failure_risk(url, 100_000_000, false);

        // Risk should be clamped to max 100%
        assert!(
            risk <= 100.0,
            "Risk should be clamped to 100%, got {}",
            risk
        );
    }

    #[test]
    fn test_thread_safety() {
        let predictor = Arc::new(FailurePredictor::new());
        let mut handles = vec![];

        for i in 0..10 {
            let predictor_clone = Arc::clone(&predictor);
            let handle = std::thread::spawn(move || {
                let url = format!("https://example.com/file-{}.bin", i);
                predictor_clone.record_failure(&url, FailureType::Timeout);
                predictor_clone.predict_failure_risk(&url, 1_000_000, false);
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // Should have 10 patterns recorded from 10 threads
        assert_eq!(predictor.get_patterns().len(), 10);
    }
}
