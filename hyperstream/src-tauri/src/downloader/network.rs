//! Self-Healing Network Stack
//! 
//! Implements robust retry strategies, exponential backoff, and automatic
//! recovery from network failures.

use std::time::Duration;
use rquest::StatusCode;

/// Retry strategy based on error type
#[derive(Debug, Clone, PartialEq)]
pub enum RetryStrategy {
    /// Retry immediately (e.g., TCP reset)
    Immediate,
    /// Retry after a delay (e.g., server busy)
    Delayed(Duration),
    /// Need to refresh the download link (e.g., 403 Forbidden, expired URL)
    RefreshLink,
    /// Fatal error, give up (e.g., 404 Not Found)
    Fatal(String),
}

/// Configuration for retry behavior
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of immediate retries
    pub max_immediate_retries: u32,
    /// Maximum number of delayed retries
    pub max_delayed_retries: u32,
    /// Initial delay for exponential backoff
    pub initial_delay: Duration,
    /// Maximum delay cap
    pub max_delay: Duration,
    /// Jitter factor (0.0 - 1.0)
    pub jitter_factor: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_immediate_retries: 3,
            max_delayed_retries: 5,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            jitter_factor: 0.3,
        }
    }
}

/// Tracks retry state for a download
#[derive(Debug, Clone)]
pub struct RetryState {
    pub immediate_attempts: u32,
    pub delayed_attempts: u32,
    pub current_delay: Duration,
    pub last_error: Option<String>,
}

impl Default for RetryState {
    fn default() -> Self {
        Self {
            immediate_attempts: 0,
            delayed_attempts: 0,
            current_delay: Duration::from_secs(1),
            last_error: None,
        }
    }
}

impl RetryState {
    pub fn reset(&mut self) {
        self.immediate_attempts = 0;
        self.delayed_attempts = 0;
        self.current_delay = Duration::from_secs(1);
        self.last_error = None;
    }
}

/// Analyze an error and determine the appropriate retry strategy
#[allow(dead_code)]
pub fn analyze_error(error: &rquest::Error) -> RetryStrategy {
    // Connection errors - retry immediately
    if error.is_connect() {
        return RetryStrategy::Immediate;
    }

    // Timeout - retry with delay
    if error.is_timeout() {
        return RetryStrategy::Delayed(Duration::from_secs(5));
    }

    // Check status code
    if let Some(status) = error.status() {
        return analyze_status(status);
    }

    // Generic network error - retry immediately
    if error.is_request() {
        return RetryStrategy::Immediate;
    }

    // Decode/body errors might be transient
    if error.is_decode() || error.is_body() {
        return RetryStrategy::Delayed(Duration::from_secs(2));
    }

    // Unknown error - retry with delay
    RetryStrategy::Delayed(Duration::from_secs(3))
}

/// Analyze HTTP status code and determine retry strategy
pub fn analyze_status(status: StatusCode) -> RetryStrategy {
    match status.as_u16() {
        // Informational - shouldn't happen, retry
        100..=199 => RetryStrategy::Immediate,
        
        // Success - no retry needed
        200..=299 => RetryStrategy::Fatal("Unexpected success status in error handler".to_string()),
        
        // Redirect - shouldn't happen if following redirects
        300..=399 => RetryStrategy::Immediate,
        
        // Client errors
        400 => RetryStrategy::Fatal("Bad Request".to_string()),
        401 => RetryStrategy::RefreshLink, // Need fresh auth
        403 => RetryStrategy::RefreshLink, // Link expired or forbidden
        404 => RetryStrategy::Fatal("File Not Found (404)".to_string()),
        408 => RetryStrategy::Delayed(Duration::from_secs(5)), // Request Timeout
        410 => RetryStrategy::Fatal("File Gone (410)".to_string()),
        416 => RetryStrategy::Fatal("Range Not Satisfiable - file may have changed".to_string()),
        429 => RetryStrategy::Delayed(Duration::from_secs(30)), // Too Many Requests
        
        // Server errors - usually transient
        500 => RetryStrategy::Delayed(Duration::from_secs(10)),
        502 => RetryStrategy::Delayed(Duration::from_secs(5)), // Bad Gateway
        503 => RetryStrategy::Delayed(Duration::from_secs(15)), // Service Unavailable
        504 => RetryStrategy::Delayed(Duration::from_secs(10)), // Gateway Timeout
        
        // Unknown 4xx
        400..=499 => RetryStrategy::Fatal(format!("Client error: {}", status)),
        
        // Unknown 5xx
        500..=599 => RetryStrategy::Delayed(Duration::from_secs(10)),
        
        // Unknown
        _ => RetryStrategy::Delayed(Duration::from_secs(5)),
    }
}

/// Calculate next delay with exponential backoff and jitter
pub fn calculate_backoff(
    current_delay: Duration,
    config: &RetryConfig,
) -> Duration {
    // Double the delay
    let next = current_delay * 2;
    
    // Cap at max
    let capped = if next > config.max_delay {
        config.max_delay
    } else {
        next
    };

    // Add jitter
    let jitter_range = (capped.as_millis() as f64 * config.jitter_factor) as u64;
    let jitter = if jitter_range > 0 {
        rand_jitter(jitter_range)
    } else {
        0
    };

    capped + Duration::from_millis(jitter)
}

/// Simple pseudo-random jitter (no external rand dependency)
fn rand_jitter(max: u64) -> u64 {
    use std::time::SystemTime;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    nanos % max
}

/// Check if content type indicates an error page (captive portal, login page)
pub fn is_error_content_type(content_type: Option<&str>, expected_type: Option<&str>) -> bool {
    let ct = match content_type {
        Some(ct) => ct.to_lowercase(),
        None => return false,
    };

    // HTML when we expected binary = probably error page
    if ct.contains("text/html") {
        if let Some(expected) = expected_type {
            if !expected.contains("html") {
                return true;
            }
        } else {
            // No expected type but got HTML = suspicious
            return true;
        }
    }

    false
}

/// Detect captive portal by checking first bytes
pub fn is_captive_portal(first_bytes: &[u8]) -> bool {
    // Check for HTML doctype or html tag
    let start = String::from_utf8_lossy(&first_bytes[..first_bytes.len().min(100)]);
    let lower = start.to_lowercase();
    
    lower.contains("<!doctype html") || 
    lower.contains("<html") ||
    lower.contains("login") ||
    lower.contains("captive") ||
    lower.contains("redirect")
}

/// Parse Retry-After header to get delay
pub fn parse_retry_after(header_value: &str) -> Option<Duration> {
    // Try parsing as seconds
    if let Ok(seconds) = header_value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }

    // Could also parse HTTP-date format here if needed
    None
}

/// HTTP response validator
#[derive(Debug, Clone)]
pub struct ResponseValidator {
    pub expected_size: Option<u64>,
    pub expected_content_type: Option<String>,
    pub require_range_support: bool,
}

impl ResponseValidator {
    pub fn new() -> Self {
        Self {
            expected_size: None,
            expected_content_type: None,
            require_range_support: true,
        }
    }

    /// Validate a response and return error if invalid
    pub fn validate(&self, 
        status: StatusCode,
        content_length: Option<u64>,
        content_type: Option<&str>,
        accept_ranges: Option<&str>,
    ) -> Result<(), String> {
        // Check status
        if !status.is_success() && status != StatusCode::PARTIAL_CONTENT {
            return Err(format!("Unexpected status: {}", status));
        }

        // For range requests, we expect 206
        if self.require_range_support && status != StatusCode::PARTIAL_CONTENT && status != StatusCode::OK {
            return Err("Server did not return partial content".to_string());
        }

        // Validate content length if expected
        if let (Some(expected), Some(actual)) = (self.expected_size, content_length) {
            // Allow some tolerance for different servers
            if actual != expected && status == StatusCode::OK {
                // Full response when we expected partial - server ignoring Range
                println!("[Validator] Warning: Got full file instead of partial");
            }
        }

        // Check content type for error pages
        if is_error_content_type(content_type, self.expected_content_type.as_deref()) {
            return Err("Received error page instead of file".to_string());
        }

        // Check range support
        if self.require_range_support {
            if let Some(ranges) = accept_ranges {
                if ranges == "none" {
                    return Err("Server does not support range requests".to_string());
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_status() {
        assert!(matches!(analyze_status(StatusCode::NOT_FOUND), RetryStrategy::Fatal(_)));
        assert!(matches!(analyze_status(StatusCode::FORBIDDEN), RetryStrategy::RefreshLink));
        assert!(matches!(analyze_status(StatusCode::SERVICE_UNAVAILABLE), RetryStrategy::Delayed(_)));
    }

    #[test]
    fn test_exponential_backoff() {
        let config = RetryConfig::default();
        let d1 = Duration::from_secs(1);
        let d2 = calculate_backoff(d1, &config);
        assert!(d2 >= Duration::from_secs(2));
        
        // Should cap at max
        let big = Duration::from_secs(100);
        let capped = calculate_backoff(big, &config);
        assert!(capped <= config.max_delay + Duration::from_millis(100));
    }

    #[test]
    fn test_captive_portal_detection() {
        assert!(is_captive_portal(b"<!DOCTYPE html><html>Login required</html>"));
        assert!(!is_captive_portal(b"\x50\x4B\x03\x04")); // ZIP magic bytes
    }
}
