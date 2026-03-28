//! Circuit Breaker Manager
//!
//! Centralized management of circuit breakers for all mirrors.
//! Provides operations for checking mirror health and filtering available mirrors.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use super::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, CircuitBreakerMetrics};
use serde::{Serialize, Deserialize};

/// Manages circuit breakers for all mirrors
#[derive(Clone)]
pub struct CircuitBreakerManager {
    breakers: Arc<Mutex<HashMap<String, CircuitBreaker>>>,
    config: CircuitBreakerConfig,
}

/// Health report for a single mirror
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirrorHealthReport {
    pub url: String,
    pub mirror_host: String, // Added field
    pub health_percent: f64,
    pub state: String,
    pub is_healthy: bool, // Added field
    pub failure_count: u32,
    pub success_count: u32,
    pub success_rate_percent: f64, // Added field
}

impl CircuitBreakerManager {
    /// Create a new circuit breaker manager with default config
    pub fn new() -> Self {
        Self {
            breakers: Arc::new(Mutex::new(HashMap::new())),
            config: CircuitBreakerConfig::default(),
        }
    }

    /// Create with custom configuration
    pub fn with_config(config: CircuitBreakerConfig) -> Self {
        Self {
            breakers: Arc::new(Mutex::new(HashMap::new())),
            config,
        }
    }

    /// Get or create breaker for a mirror URL
    pub fn get_breaker(&self, mirror_url: &str) -> CircuitBreaker {
        let mut breakers = self.breakers.lock().unwrap();
        breakers
            .entry(mirror_url.to_string())
            .or_insert_with(|| CircuitBreaker::new(mirror_url.to_string(), self.config.clone()))
            .clone()
    }

    /// Record success for a mirror
    pub fn record_success(&self, mirror_url: &str) {
        let breaker = self.get_breaker(mirror_url);
        breaker.record_success();
    }

    /// Record failure for a mirror
    pub fn record_failure(&self, mirror_url: &str) {
        let breaker = self.get_breaker(mirror_url);
        breaker.record_failure();
    }

    /// Check if mirror is available for use
    pub fn can_use_mirror(&self, mirror_url: &str) -> bool {
        let breaker = self.get_breaker(mirror_url);
        let (allowed, _) = breaker.allow_request();
        allowed
    }

    /// Filter healthy mirrors from a list
    pub fn filter_healthy_mirrors(&self, mirrors: Vec<String>) -> Vec<String> {
        mirrors
            .into_iter()
            .filter(|m| self.can_use_mirror(m))
            .collect()
    }

    /// Get all mirrors in order of health score (best first)
    pub fn rank_mirrors_by_health(&self, mirrors: Vec<String>) -> Vec<(String, f64)> {
        let mut ranked: Vec<(String, f64)> = mirrors
            .into_iter()
            .map(|m| {
                let breaker = self.get_breaker(&m);
                (m, breaker.health_score())
            })
            .collect();

        ranked.sort_by(|(_, a), (_, b)| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
        ranked
    }

    /// Get health report for a single mirror
    pub fn get_mirror_health(&self, mirror_url: &str) -> Option<MirrorHealthReport> {
        let breaker = self.get_breaker(mirror_url);
        let metrics = breaker.metrics();

        Some(MirrorHealthReport {
            url: mirror_url.to_string(),
            mirror_host: mirror_url.to_string(), // Simplified for now
            health_percent: breaker.health_score(),
            state: breaker.state().to_string(),
            is_healthy: breaker.state().to_string() != "Open",
            failure_count: metrics.failure_count,
            success_count: metrics.success_count,
            success_rate_percent: if (metrics.success_count + metrics.failure_count) > 0 {
                (metrics.success_count as f64 / (metrics.success_count + metrics.failure_count) as f64) * 100.0
            } else {
                100.0
            },
        })
    }

    /// Get health report for all mirrors
    pub fn get_all_mirrors_health(&self) -> Vec<MirrorHealthReport> {
        let breakers = self.breakers.lock().unwrap();
        breakers
            .iter()
            .map(|(url, breaker)| {
                let metrics = breaker.metrics();
                MirrorHealthReport {
                    url: url.clone(),
                    mirror_host: url.clone(),
                    health_percent: breaker.health_score(),
                    state: breaker.state().to_string(),
                    is_healthy: breaker.state().to_string() != "Open",
                    failure_count: metrics.failure_count,
                    success_count: metrics.success_count,
                    success_rate_percent: if (metrics.success_count + metrics.failure_count) > 0 {
                        (metrics.success_count as f64 / (metrics.success_count + metrics.failure_count) as f64) * 100.0
                    } else {
                        100.0
                    },
                }
            })
            .collect()
    }

    /// Get detailed metrics for all mirrors
    pub fn get_detailed_metrics(&self) -> Vec<CircuitBreakerMetrics> {
        let breakers = self.breakers.lock().unwrap();
        breakers.iter().map(|(_, b)| b.metrics()).collect()
    }

    /// Check if any mirrors are currently open (failing)
    pub fn has_failing_mirrors(&self) -> bool {
        let breakers = self.breakers.lock().unwrap();
        breakers.values().any(|b| b.state().to_string() == "Open")
    }

    /// Get count of mirrors in each state
    pub fn get_state_distribution(&self) -> HashMap<String, usize> {
        let mut distribution = HashMap::new();
        distribution.insert("Closed".to_string(), 0);
        distribution.insert("Open".to_string(), 0);
        distribution.insert("HalfOpen".to_string(), 0);

        let breakers = self.breakers.lock().unwrap();
        for breaker in breakers.values() {
            let state = breaker.state().to_string();
            *distribution.entry(state).or_insert(0) += 1;
        }

        distribution
    }

    /// Reset all circuit breakers (for testing/admin)
    pub fn reset_all(&self) {
        let mut breakers = self.breakers.lock().unwrap();
        breakers.clear();
    }

    /// Reset a specific mirror's circuit breaker
    pub fn reset_mirror(&self, mirror_url: &str) {
        let mut breakers = self.breakers.lock().unwrap();
        breakers.remove(mirror_url);
    }
}

impl Default for CircuitBreakerManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manager_filters_healthy_mirrors() {
        let manager = CircuitBreakerManager::new();

        // Trip one mirror
        manager.record_failure("mirror1.com");
        manager.record_failure("mirror1.com");
        manager.record_failure("mirror1.com");
        manager.record_failure("mirror1.com");
        manager.record_failure("mirror1.com");

        // Filter should remove it
        let healthy = manager.filter_healthy_mirrors(vec![
            "mirror1.com".into(),
            "mirror2.com".into(),
        ]);
        assert_eq!(healthy, vec!["mirror2.com"]);
    }

    #[test]
    fn test_manager_ranks_mirrors_by_health() {
        let manager = CircuitBreakerManager::new();

        // Record successes for mirror1
        for _ in 0..3 {
            manager.record_success("mirror1.com");
        }

        // Record failure for mirror2
        manager.record_failure("mirror2.com");

        let ranked = manager.rank_mirrors_by_health(vec![
            "mirror1.com".into(),
            "mirror2.com".into(),
        ]);

        assert_eq!(ranked[0].0, "mirror1.com");
        assert!(ranked[0].1 > ranked[1].1);
    }

    #[test]
    fn test_manager_state_distribution() {
        let manager = CircuitBreakerManager::new();

        // Create some mirrors
        for _ in 0..3 {
            manager.record_success("healthy.com");
        }

        for _ in 0..5 {
            manager.record_failure("failing.com");
        }

        let distribution = manager.get_state_distribution();
        assert_eq!(distribution.get("Closed").unwrap(), &1);
        assert_eq!(distribution.get("Open").unwrap(), &1);
    }

    #[test]
    fn test_manager_reset() {
        let manager = CircuitBreakerManager::new();

        for _ in 0..5 {
            manager.record_failure("mirror.com");
        }

        assert!(!manager.can_use_mirror("mirror.com"));

        manager.reset_mirror("mirror.com");

        // After reset, mirror should be available again
        assert!(manager.can_use_mirror("mirror.com"));
    }
}
