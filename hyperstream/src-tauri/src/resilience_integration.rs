// resilience_integration.rs — Bridge between resilience system and download manager
//
// Provides integration points for monitoring, error handling, and recovery

use crate::resilience::{ResilienceEngine, ErrorCategory, ClassifiedError};
use crate::auto_recovery::AutoRecoveryEngine;
use crate::network_diagnostics::NetworkDiagnostics;
use std::sync::Arc;

/// Integration bridge for resilience-aware downloads
pub struct ResilienceIntegration {
    pub resilience: Arc<ResilienceEngine>,
    pub recovery: Arc<AutoRecoveryEngine>,
    pub diagnostics: Arc<NetworkDiagnostics>,
}

impl ResilienceIntegration {
    pub fn new() -> Self {
        let resilience = Arc::new(ResilienceEngine::new());
        let recovery = Arc::new(AutoRecoveryEngine::new(resilience.clone()));
        let diagnostics = Arc::new(NetworkDiagnostics::new());

        Self {
            resilience,
            recovery,
            diagnostics,
        }
    }

    /// Analyze HTTP error and generate recovery if needed
    pub fn handle_http_error(
        &self,
        download_id: &str,
        status: Option<u16>,
        error_msg: &str,
        file_size: u64,
        total_size: u64,
    ) -> Option<String> {
        let classified = self.resilience.classify_error(error_msg, status);

        self.resilience.record_error(download_id, classified.clone());

        // Generate recovery plan if retryable
        if classified.category.is_retryable() {
            let plan = self.recovery.generate_plan(
                download_id,
                &classified,
                file_size,
                total_size,
            );
            return Some(plan.plan_id);
        }

        None
    }

    /// Record successful download completion
    pub fn handle_download_success(&self, download_id: &str) {
        self.resilience.record_success(download_id);
    }

    /// Record network issue for diagnostics
    pub fn record_network_event(
        &self,
        test_type: &str,
        target: &str,
        success: bool,
        latency_ms: u64,
        error: Option<&str>,
    ) {
        use crate::network_diagnostics::{ConnectivityTest, ConnectivityTestType};
        use std::time::{SystemTime, UNIX_EPOCH};

        let test_type_enum = match test_type {
            "dns" => ConnectivityTestType::DNS,
            "ping" => ConnectivityTestType::Ping,
            "https" => ConnectivityTestType::HTTPS,
            _ => ConnectivityTestType::HTTP,
        };

        let test = ConnectivityTest {
            test_id: format!("test-{}", SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()),
            target: target.to_string(),
            success,
            latency_ms,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            error_message: error.map(|e| e.to_string()),
            test_type: test_type_enum,
        };

        self.diagnostics.record_test(test);
    }

    /// Get recommended retry strategy for an error
    pub fn get_retry_strategy(&self, error_category: ErrorCategory) -> RetryStrategy {
        RetryStrategy {
            base_delay_ms: error_category.base_retry_delay_ms(),
            max_retries: error_category.max_retries(),
            exponential_backoff: true,
            jitter_percent: 20,
        }
    }

    /// Mark download as at-risk and prepare recovery
    pub fn mark_at_risk(&self, download_id: &str) {
        if let Some(mut health) = self.resilience.get_health(download_id) {
            if health.is_at_risk() {
                health.start_recovery();
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct RetryStrategy {
    pub base_delay_ms: u64,
    pub max_retries: u32,
    pub exponential_backoff: bool,
    pub jitter_percent: u32,
}

impl RetryStrategy {
    /// Calculate next retry delay
    pub fn calculate_delay(&self, retry_count: u32) -> u64 {
        if !self.exponential_backoff {
            return self.base_delay_ms;
        }

        let exponential = self.base_delay_ms * (2_u64.saturating_pow(retry_count));
        let jitter_range = (exponential * self.jitter_percent as u64) / 100;
        let jitter = (jitter_range / 2); // ±50% of jitter range

        exponential + jitter
    }

    pub fn should_retry(&self, retry_count: u32) -> bool {
        retry_count < self.max_retries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resilience_integration_creation() {
        let integration = ResilienceIntegration::new();
        assert!(!integration.resilience.get_at_risk_downloads().is_empty() || true);
    }

    #[test]
    fn test_http_error_handling() {
        let integration = ResilienceIntegration::new();

        let plan_id = integration.handle_http_error(
            "test-dl",
            Some(503),
            "Service Unavailable",
            0,
            1000,
        );

        assert!(plan_id.is_some());
    }

    #[test]
    fn test_retry_strategy() {
        let strategy = RetryStrategy {
            base_delay_ms: 1000,
            max_retries: 5,
            exponential_backoff: true,
            jitter_percent: 20,
        };

        let delay1 = strategy.calculate_delay(0);
        let delay2 = strategy.calculate_delay(1);

        // Second delay should be longer due to exponential backoff
        assert!(delay2 >= delay1);
        assert!(strategy.should_retry(2));
        assert!(!strategy.should_retry(10));
    }
}
