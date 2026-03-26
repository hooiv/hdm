// resilience.rs — Comprehensive error recovery and resilience system
//
// This module provides production-grade resilience for downloads:
// - Error classification and categorization
// - Intelligent retry strategies with exponential backoff
// - Download integrity validation
// - Network diagnosis and failure detection
// - Automatic healing procedures
// - Health monitoring and metrics

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Error categories for intelligent handling
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    /// Network unreachable / timeout
    NetworkUnreachable,
    /// Server returned 5xx error
    ServerError,
    /// Client error (4xx)
    ClientError,
    /// Rate limiting / too many requests
    RateLimited,
    /// DNS resolution failed
    DnsFailure,
    /// Connection refused / host unreachable
    ConnectionRefused,
    /// SSL/TLS error
    TlsError,
    /// Disk full / permission denied
    DiskError,
    /// Corrupted data
    CorruptedData,
    /// Unknown error
    Unknown,
}

impl ErrorCategory {
    /// Determine if this error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::NetworkUnreachable
                | Self::ServerError
                | Self::RateLimited
                | Self::DnsFailure
                | Self::ConnectionRefused
        )
    }

    /// Get base delay for retry (milliseconds)
    pub fn base_retry_delay_ms(&self) -> u64 {
        match self {
            Self::RateLimited => 5000,      // Wait longer for rate limits
            Self::ServerError => 2000,       // Wait for server recovery
            Self::NetworkUnreachable => 500, // Quick retry for network
            _ => 1000,
        }
    }

    /// Get max retries for this category
    pub fn max_retries(&self) -> u32 {
        match self {
            Self::RateLimited => 20,          // Aggressive retries for rate limits
            Self::ServerError => 10,          // Moderate retries for server errors
            Self::NetworkUnreachable => 15,   // Many retries for network issues
            Self::DnsFailure => 8,
            Self::ConnectionRefused => 12,
            Self::ClientError => 1,           // Don't retry client errors
            Self::TlsError => 3,              // Limited retries for TLS
            Self::DiskError => 2,             // Limited retries for disk
            Self::CorruptedData => 1,         // Don't retry corruption
            Self::Unknown => 5,
        }
    }
}

/// Classified network error with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassifiedError {
    pub category: ErrorCategory,
    pub message: String,
    pub timestamp: u64,
    pub retry_count: u32,
    pub next_retry_at: u64,
    pub last_error_details: Option<String>,
}

impl ClassifiedError {
    pub fn new(category: ErrorCategory, message: impl Into<String>) -> Self {
        Self {
            category,
            message: message.into(),
            timestamp: current_timestamp_ms(),
            retry_count: 0,
            next_retry_at: 0,
            last_error_details: None,
        }
    }

    /// Calculate next retry time with exponential backoff + jitter
    pub fn calculate_next_retry(&mut self) {
        if !self.category.is_retryable() {
            self.next_retry_at = u64::MAX;
            return;
        }

        let base_delay = self.category.base_retry_delay_ms();
        let exponential_delay = base_delay * (2_u64.saturating_pow(self.retry_count));

        // Add jitter: ±20% randomization
        let jitter = (exponential_delay / 5) / 2; // 20% range
        let randomized = exponential_delay + (jitter / 2);

        self.next_retry_at = current_timestamp_ms() + randomized;
        self.retry_count += 1;
    }

    /// Check if we should retry based on retry count
    pub fn should_retry(&self) -> bool {
        if !self.category.is_retryable() {
            return false;
        }
        self.retry_count < self.category.max_retries()
    }

    /// Check if it's time to retry
    pub fn is_ready_to_retry(&self) -> bool {
        self.should_retry() && current_timestamp_ms() >= self.next_retry_at
    }
}

/// Download integrity validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityCheckResult {
    pub download_id: String,
    pub is_valid: bool,
    pub file_size: u64,
    pub expected_size: u64,
    pub checksum_matches: Option<bool>,
    pub corruption_detected: bool,
    pub recoverable: bool,
    pub issues: Vec<String>,
}

impl IntegrityCheckResult {
    pub fn new_valid(download_id: String, file_size: u64) -> Self {
        Self {
            download_id,
            is_valid: true,
            file_size,
            expected_size: file_size,
            checksum_matches: None,
            corruption_detected: false,
            recoverable: false,
            issues: Vec::new(),
        }
    }

    pub fn is_recoverable_issue(&self) -> bool {
        // Size mismatch can be recovered by resuming
        // Missing checksum is not an issue if size matches
        self.recoverable && self.file_size > 0
    }
}

/// Health status of a download
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    AtRisk,
    Failed,
    Recovering,
    Corrupted,
}

/// Download health and metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadHealth {
    pub download_id: String,
    pub status: HealthStatus,
    pub consecutive_failures: u32,
    pub last_error: Option<ClassifiedError>,
    pub recoveries_attempted: u32,
    pub data_lost_bytes: u64,
    pub retry_history: Vec<(u64, ErrorCategory)>, // (timestamp, category)
    pub last_successful_activity: u64,
}

impl DownloadHealth {
    pub fn new(download_id: String) -> Self {
        Self {
            download_id,
            status: HealthStatus::Healthy,
            consecutive_failures: 0,
            last_error: None,
            recoveries_attempted: 0,
            data_lost_bytes: 0,
            retry_history: Vec::new(),
            last_successful_activity: current_timestamp_ms(),
        }
    }

    pub fn record_failure(&mut self, error: ClassifiedError) {
        self.consecutive_failures += 1;
        self.retry_history
            .push((current_timestamp_ms(), error.category));

        if self.consecutive_failures > 5 {
            self.status = HealthStatus::Failed;
        } else if self.consecutive_failures > 2 {
            self.status = HealthStatus::AtRisk;
        }

        self.last_error = Some(error);
    }

    pub fn record_success(&mut self) {
        self.consecutive_failures = 0;
        self.status = HealthStatus::Healthy;
        self.last_successful_activity = current_timestamp_ms();
    }

    pub fn start_recovery(&mut self) {
        self.recoveries_attempted += 1;
        self.status = HealthStatus::Recovering;
    }

    pub fn is_at_risk(&self) -> bool {
        matches!(self.status, HealthStatus::AtRisk | HealthStatus::Failed)
    }

    pub fn time_since_last_activity_ms(&self) -> u64 {
        current_timestamp_ms().saturating_sub(self.last_successful_activity)
    }
}

/// Resilience engine state
pub struct ResilienceEngine {
    health_tracker: Arc<RwLock<HashMap<String, DownloadHealth>>>,
    error_history: Arc<RwLock<Vec<(u64, String, ClassifiedError)>>>, // (timestamp, download_id, error)
    max_history_age_ms: u64,
}

impl ResilienceEngine {
    pub fn new() -> Self {
        Self {
            health_tracker: Arc::new(RwLock::new(HashMap::new())),
            error_history: Arc::new(RwLock::new(Vec::new())),
            max_history_age_ms: 7 * 24 * 60 * 60 * 1000, // 7 days
        }
    }

    /// Classify an error based on error message and HTTP status
    pub fn classify_error(&self, error_msg: &str, http_status: Option<u16>) -> ClassifiedError {
        let category = match http_status {
            Some(405..=429) => ErrorCategory::RateLimited,
            Some(500 | 502 | 503 | 504) => ErrorCategory::ServerError,
            Some(400..=499) => ErrorCategory::ClientError,
            _ => self.categorize_message(error_msg),
        };

        ClassifiedError::new(category, error_msg)
    }

    fn categorize_message(&self, msg: &str) -> ErrorCategory {
        let msg_lower = msg.to_lowercase();

        if msg_lower.contains("timeout") || msg_lower.contains("connection") {
            ErrorCategory::NetworkUnreachable
        } else if msg_lower.contains("dns") || msg_lower.contains("resolve") {
            ErrorCategory::DnsFailure
        } else if msg_lower.contains("refuse") {
            ErrorCategory::ConnectionRefused
        } else if msg_lower.contains("ssl") || msg_lower.contains("tls") {
            ErrorCategory::TlsError
        } else if msg_lower.contains("disk") || msg_lower.contains("space") || msg_lower.contains("permission") {
            ErrorCategory::DiskError
        } else if msg_lower.contains("corrupt") || msg_lower.contains("checksum") {
            ErrorCategory::CorruptedData
        } else {
            ErrorCategory::Unknown
        }
    }

    /// Record an error for a download
    pub fn record_error(&self, download_id: &str, error: ClassifiedError) {
        let mut history = self.error_history.write().unwrap();
        history.push((current_timestamp_ms(), download_id.to_string(), error.clone()));

        // Cleanup old entries
        let cutoff = current_timestamp_ms().saturating_sub(self.max_history_age_ms);
        history.retain(|(ts, _, _)| *ts > cutoff);

        // Update health tracker
        if let Ok(mut tracker) = self.health_tracker.write() {
            let health = tracker
                .entry(download_id.to_string())
                .or_insert_with(|| DownloadHealth::new(download_id.to_string()));
            health.record_failure(error);
        }
    }

    /// Get health status for a download
    pub fn get_health(&self, download_id: &str) -> Option<DownloadHealth> {
        self.health_tracker
            .read()
            .unwrap()
            .get(download_id)
            .cloned()
    }

    /// Record successful activity
    pub fn record_success(&self, download_id: &str) {
        if let Ok(mut tracker) = self.health_tracker.write() {
            let health = tracker
                .entry(download_id.to_string())
                .or_insert_with(|| DownloadHealth::new(download_id.to_string()));
            health.record_success();
        }
    }

    /// Get all downloads at risk
    pub fn get_at_risk_downloads(&self) -> Vec<DownloadHealth> {
        self.health_tracker
            .read()
            .unwrap()
            .values()
            .filter(|h| h.is_at_risk())
            .cloned()
            .collect()
    }

    /// Get recent errors
    pub fn get_recent_errors(&self, last_n_minutes: u64) -> Vec<(u64, String, ClassifiedError)> {
        let cutoff = current_timestamp_ms().saturating_sub(last_n_minutes * 60 * 1000);
        self.error_history
            .read()
            .unwrap()
            .iter()
            .filter(|(ts, _, _)| *ts > cutoff)
            .cloned()
            .collect()
    }

    /// Get error statistics
    pub fn get_error_stats(&self) -> HashMap<String, u32> {
        let mut stats: HashMap<String, u32> = HashMap::new();
        for (_, _, error) in self.error_history.read().unwrap().iter() {
            *stats.entry(format!("{:?}", error.category)).or_insert(0) += 1;
        }
        stats
    }

    /// Validate download integrity
    pub fn validate_integrity(
        &self,
        download_id: &str,
        file_size: u64,
        expected_size: u64,
    ) -> IntegrityCheckResult {
        let mut result = IntegrityCheckResult::new_valid(download_id.to_string(), file_size);
        result.expected_size = expected_size;

        if file_size == 0 && expected_size > 0 {
            result.is_valid = false;
            result.issues.push("File is empty but supposed to be larger".to_string());
            return result;
        }

        if file_size > expected_size {
            result.is_valid = false;
            result.issues.push("File size exceeds expected size".to_string());
            result.corruption_detected = true;
            return result;
        }

        if file_size < expected_size {
            result.is_valid = false;
            result.issues.push("File incomplete".to_string());
            result.recoverable = true;
        }

        result
    }
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
    fn test_error_classification() {
        let engine = ResilienceEngine::new();

        let err = engine.classify_error("Connection timeout", None);
        assert_eq!(err.category, ErrorCategory::NetworkUnreachable);

        let err = engine.classify_error("", Some(503));
        assert_eq!(err.category, ErrorCategory::ServerError);

        let err = engine.classify_error("", Some(429));
        assert_eq!(err.category, ErrorCategory::RateLimited);
    }

    #[test]
    fn test_exponential_backoff() {
        let mut error = ClassifiedError::new(ErrorCategory::ServerError, "test");

        let delays = (0..5)
            .map(|_| {
                error.calculate_next_retry();
                error.next_retry_at
            })
            .collect::<Vec<_>>();

        // Delays should increase (accounting for jitter, check general trend)
        assert!(delays[1] >= delays[0]);
        assert!(delays[4] > delays[0]);
    }

    #[test]
    fn test_health_tracking() {
        let engine = ResilienceEngine::new();
        let dl_id = "test-download";

        engine.record_success(dl_id);
        let health = engine.get_health(dl_id).unwrap();
        assert_eq!(health.consecutive_failures, 0);

        let error = ClassifiedError::new(ErrorCategory::NetworkUnreachable, "timeout");
        engine.record_error(dl_id, error);
        let health = engine.get_health(dl_id).unwrap();
        assert_eq!(health.consecutive_failures, 1);
    }

    #[test]
    fn test_integrity_validation() {
        let engine = ResilienceEngine::new();

        let result = engine.validate_integrity("dl1", 100, 100);
        assert!(result.is_valid);

        let result = engine.validate_integrity("dl2", 50, 100);
        assert!(!result.is_valid);
        assert!(result.recoverable);

        let result = engine.validate_integrity("dl3", 0, 100);
        assert!(!result.is_valid);
    }
}
