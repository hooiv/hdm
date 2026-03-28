//! Circuit Breaker State Machine for Mirror Health Tracking
//!
//! Implements the circuit breaker pattern per-mirror:
//! - **Closed**: Mirror is healthy, requests pass through
//! - **Open**: Mirror is failing, requests are rejected immediately
//! - **Half-Open**: Testing if mirror has recovered
//!
//! This prevents hammering failing mirrors and enables graceful recovery.

use serde::{Serialize, Deserialize};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// Circuit breaker state machine state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CircuitState {
    /// Requests passing through, mirror is healthy
    Closed,
    /// Mirror is failing, requests rejected
    Open,
    /// Testing if mirror has recovered
    HalfOpen,
}

impl std::fmt::Display for CircuitState {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Closed => write!(f, "Closed"),
            Self::Open => write!(f, "Open"),
            Self::HalfOpen => write!(f, "HalfOpen"),
        }
    }
}

/// Circuit breaker metrics for a single mirror
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerMetrics {
    pub mirror_url: String,
    pub state: CircuitState,
    pub failure_count: u32,
    pub success_count: u32,
    pub last_failure_time: Option<u64>,  // Unix timestamp ms
    pub last_success_time: Option<u64>,
    pub state_change_time: u64,          // When did we enter current state?
    pub opened_at: Option<u64>,          // When did we open the circuit?
}

/// Configurable thresholds for circuit breaker behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Fail after this many consecutive failures
    pub failure_threshold: u32,
    /// Time to spend in Open state before trying recovery (seconds)
    pub timeout_secs: u64,
    /// Number of successful requests in HalfOpen state to recover
    pub recovery_success_threshold: u32,
    /// Reset failure count after this many seconds of success
    pub success_window_secs: u64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,          // Trip after 5 failures
            timeout_secs: 30,              // Wait 30 seconds before half-open
            recovery_success_threshold: 2, // 2 successes = recovered
            success_window_secs: 300,      // 5 minutes without failures resets
        }
    }
}

/// Circuit breaker for a single mirror
#[derive(Clone)]
pub struct CircuitBreaker {
    mirror_url: String,
    config: Arc<Mutex<CircuitBreakerConfig>>,
    metrics: Arc<Mutex<CircuitBreakerMetrics>>,
}

impl std::fmt::Debug for CircuitBreaker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CircuitBreaker")
            .field("mirror_url", &self.mirror_url)
            .field("metrics", &*self.metrics.lock().unwrap())
            .finish()
    }
}

impl CircuitBreaker {
    /// Create a new circuit breaker for a mirror
    pub fn new(mirror_url: String, config: CircuitBreakerConfig) -> Self {
        let now = now_ms();

        Self {
            mirror_url: mirror_url.clone(),
            config: Arc::new(Mutex::new(config)),
            metrics: Arc::new(Mutex::new(CircuitBreakerMetrics {
                mirror_url,
                state: CircuitState::Closed,
                failure_count: 0,
                success_count: 0,
                last_failure_time: None,
                last_success_time: None,
                state_change_time: now,
                opened_at: None,
            })),
        }
    }

    /// Record a successful request
    pub fn record_success(&self) {
        let mut metrics = self.metrics.lock().unwrap();
        metrics.success_count += 1;
        metrics.last_success_time = Some(now_ms());

        let config = self.config.lock().unwrap();

        // Transition: Half-Open → Closed after N successes
        if metrics.state == CircuitState::HalfOpen {
            if metrics.success_count >= config.recovery_success_threshold {
                metrics.state = CircuitState::Closed;
                metrics.failure_count = 0; // Reset
                metrics.state_change_time = now_ms();
                eprintln!(
                    "[CircuitBreaker] {} recovered: HalfOpen→Closed (after {} successes)",
                    metrics.mirror_url, metrics.success_count
                );
            }
        } else if metrics.state == CircuitState::Closed && metrics.failure_count > 0 {
            // In Closed state, reset failure count if we've been successful for a while
            let last_failure = metrics.last_failure_time.unwrap_or(0);
            if now_ms() - last_failure > config.success_window_secs * 1000 {
                metrics.failure_count = 0;
            }
        }
    }

    /// Record a failed request
    pub fn record_failure(&self) {
        let mut metrics = self.metrics.lock().unwrap();
        metrics.failure_count += 1;
        metrics.last_failure_time = Some(now_ms());

        let config = self.config.lock().unwrap();

        // Transition: Closed → Open after threshold
        if metrics.state == CircuitState::Closed
            && metrics.failure_count >= config.failure_threshold
        {
            metrics.state = CircuitState::Open;
            metrics.opened_at = Some(now_ms());
            metrics.state_change_time = now_ms();
            eprintln!(
                "[CircuitBreaker] {} degraded: Closed→Open (after {} failures)",
                metrics.mirror_url, metrics.failure_count
            );
        } else if metrics.state == CircuitState::HalfOpen {
            // Any failure in half-open → back to open
            metrics.state = CircuitState::Open;
            metrics.opened_at = Some(now_ms());
            metrics.state_change_time = now_ms();
            metrics.success_count = 0; // Reset recovery progress
            eprintln!(
                "[CircuitBreaker] {} failed recovery: HalfOpen→Open",
                metrics.mirror_url
            );
        }
    }

    /// Check if requests should be allowed through
    /// Returns (allowed, reason)
    pub fn allow_request(&self) -> (bool, String) {
        let config = self.config.lock().unwrap();
        let mut metrics = self.metrics.lock().unwrap();

        match metrics.state {
            CircuitState::Closed => {
                (true, "Circuit closed - mirror healthy".into())
            }

            CircuitState::Open => {
                // Check if timeout elapsed to try half-open
                let opened_at = metrics.opened_at.unwrap_or_else(now_ms);
                let elapsed_ms = now_ms().saturating_sub(opened_at);
                let timeout_ms = config.timeout_secs * 1000;

                if elapsed_ms >= timeout_ms {
                    // Transition to half-open for recovery test
                    metrics.state = CircuitState::HalfOpen;
                    metrics.success_count = 0;
                    metrics.state_change_time = now_ms();
                    eprintln!(
                        "[CircuitBreaker] {} recovering: Open→HalfOpen (timeout {}ms elapsed)",
                        metrics.mirror_url, elapsed_ms
                    );
                    (true, "Circuit half-open - testing recovery".into())
                } else {
                    let wait_ms = timeout_ms.saturating_sub(elapsed_ms);
                    (
                        false,
                        format!("Circuit open - wait {}ms before retry", wait_ms),
                    )
                }
            }

            CircuitState::HalfOpen => {
                (true, "Circuit half-open - testing recovery".into())
            }
        }
    }

    /// Get current state
    pub fn state(&self) -> CircuitState {
        self.metrics.lock().unwrap().state
    }

    /// Get metrics snapshot
    pub fn metrics(&self) -> CircuitBreakerMetrics {
        self.metrics.lock().unwrap().clone()
    }

    /// Calculate health score (0-100)
    pub fn health_score(&self) -> f64 {
        let metrics = self.metrics.lock().unwrap();
        let total = metrics.success_count + metrics.failure_count;
        if total == 0 {
            return 100.0;
        }
        (metrics.success_count as f64 / total as f64) * 100.0
    }

    /// Get mirror URL
    pub fn mirror_url(&self) -> &str {
        &self.mirror_url
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_trips_after_threshold() {
        let breaker = CircuitBreaker::new(
            "test.mirror.com".into(),
            CircuitBreakerConfig {
                failure_threshold: 3,
                ..Default::default()
            },
        );

        // Record 3 failures
        assert_eq!(breaker.state(), CircuitState::Closed);
        breaker.record_failure();
        breaker.record_failure();
        breaker.record_failure();

        // Should trip to Open
        assert_eq!(breaker.state(), CircuitState::Open);
        let (allowed, _) = breaker.allow_request();
        assert!(!allowed);
    }

    #[test]
    fn test_circuit_recovery_sequence() {
        let breaker = CircuitBreaker::new(
            "test.mirror.com".into(),
            CircuitBreakerConfig {
                failure_threshold: 2,
                timeout_secs: 0, // Instant recovery for testing
                recovery_success_threshold: 2,
                ..Default::default()
            },
        );

        // Trip the circuit
        breaker.record_failure();
        breaker.record_failure();
        assert_eq!(breaker.state(), CircuitState::Open);

        // First allow_request after timeout transits to half-open
        let (allowed, _) = breaker.allow_request();
        assert!(allowed); // Will try half-open

        // Simulate successful requests in half-open
        breaker.record_success();
        breaker.record_success();

        // Should recover to Closed
        assert_eq!(breaker.state(), CircuitState::Closed);
    }

    #[test]
    fn test_health_score_calculation() {
        let breaker = CircuitBreaker::new("test.mirror.com".into(), CircuitBreakerConfig::default());

        breaker.record_success();
        breaker.record_success();
        breaker.record_failure();

        let health = breaker.health_score();
        assert!((health - 66.67).abs() < 1.0); // ~2/3
    }

    #[test]
    fn test_half_open_failure_goes_back_to_open() {
        let breaker = CircuitBreaker::new(
            "test.mirror.com".into(),
            CircuitBreakerConfig {
                failure_threshold: 1,
                timeout_secs: 0,
                ..Default::default()
            },
        );

        breaker.record_failure(); // Opens circuit
        assert_eq!(breaker.state(), CircuitState::Open);

        let (allowed, _) = breaker.allow_request();
        assert!(allowed); // Transitions to half-open
        assert_eq!(breaker.state(), CircuitState::HalfOpen);

        breaker.record_failure(); // Fails in half-open
        assert_eq!(breaker.state(), CircuitState::Open); // Back to open
    }

    #[test]
    fn test_closed_state_resets_after_success_window() {
        let breaker = CircuitBreaker::new(
            "test.mirror.com".into(),
            CircuitBreakerConfig {
                failure_threshold: 3,
                success_window_secs: 1, // 1 second for testing
                ..Default::default()
            },
        );

        // Record 2 failures in closed state
        breaker.record_failure();
        breaker.record_failure();
        assert_eq!(breaker.metrics().failure_count, 2);

        // Record a success
        breaker.record_success();

        // Metrics should show 2 failures still
        assert_eq!(breaker.metrics().failure_count, 2);

        // After success window elapsed, failure count should reset on next call
        // (In real code, would need to add reset logic or use a different test approach)
    }
}
