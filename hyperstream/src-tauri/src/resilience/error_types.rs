//! Structured Error Classification for Download Operations
//!
//! Replaces string-based error handling with typed errors that encode recovery strategy:
//! - Should we failover to another mirror?
//! - How long should we wait before retrying?
//! - Is this a transient or permanent failure?
//!
//! This enables intelligent retry logic without guessing from error messages.

use std::time::Duration;
use serde::{Deserialize, Serialize};

/// Structured error types for downloads, distinguishing failure modes and recovery strategies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DownloadError {
    /// Mirror unreachable / timeout - safe to failover immediately
    /// Example: DNS timeout, TCP RST, connection refused
    MirrorUnreachable {
        mirror: String,
        duration_ms: u64,
    },

    /// Rate limited by mirror - backoff required before retry
    /// Example: 429 Too Many Requests, X-RateLimit-Reset header present
    RateLimited {
        mirror: String,
        retry_after_secs: Option<u64>,
    },

    /// Network error that could be transient or temporary
    /// Example: Broken pipe, temporary network outage, slow connection
    NetworkError {
        reason: String,
        is_transient: bool,
    },

    /// Disk I/O error - can't write file, stop immediately
    /// Example: No space on device, permission denied, file locked
    DiskError {
        path: String,
        reason: String,
    },

    /// Range request not supported by mirror
    /// Example: Server doesn't support HTTP Range header - must use single-segment
    RangeNotSupported {
        mirror: String,
    },

    /// Integrity check failed - data received doesn't match expected
    /// Example: Checksum mismatch, CRC error, corrupted download
    IntegrityViolation {
        expected: String,
        actual: String,
    },

    /// Configuration or setup error - won't be fixed by retry
    /// Example: Invalid URL, missing authentication, bad proxy configuration
    ConfigError {
        reason: String,
    },

    /// Unable to connect to mirror at all
    /// Example: Firewall blocked, DNS resolution failed
    ConnectionFailed {
        mirror: String,
        reason: String,
    },

    /// Too many retries exhausted - give up
    /// Example: Mirror failed 10 times in a row
    RetryExhausted {
        mirror: String,
        attempts: u32,
    },
}

impl DownloadError {
    /// Should we automatically failover to another mirror?
    /// Returns true for transient mirror-specific failures that might succeed elsewhere
    pub fn should_failover(&self) -> bool {
        matches!(
            self,
            Self::MirrorUnreachable { .. }
                | Self::RangeNotSupported { .. }
                | Self::RateLimited { .. }
                | Self::ConnectionFailed { .. }
        )
    }

    /// Should we retry the same mirror after a delay?
    /// Returns true for transient failures that might succeed on retry
    pub fn should_retry(&self) -> bool {
        matches!(
            self,
            Self::NetworkError {
                is_transient: true,
                ..
            } | Self::RateLimited { .. }
        )
    }

    /// Get recommended backoff duration before retrying
    pub fn backoff_duration(&self) -> Duration {
        match self {
            // Rate limit: use server's retry-after, or default 30s
            Self::RateLimited {
                retry_after_secs: Some(secs),
                ..
            } => Duration::from_secs(*secs),
            Self::RateLimited { .. } => Duration::from_secs(30),

            // Transient network error: short backoff (exponential in session layer)
            Self::NetworkError {
                is_transient: true,
                ..
            } => Duration::from_millis(100),

            // No backoff for immediate failover scenarios
            _ => Duration::from_secs(0),
        }
    }

    /// Is this a permanent error that won't recover?
    pub fn is_permanent(&self) -> bool {
        matches!(
            self,
            Self::DiskError { .. }
                | Self::IntegrityViolation { .. }
                | Self::ConfigError { .. }
                | Self::RetryExhausted { .. }
        )
    }

    /// Human-readable description for UI and logging
    pub fn description(&self) -> String {
        match self {
            Self::MirrorUnreachable { mirror, duration_ms } => {
                format!("Mirror {} unreachable after {}ms", mirror, duration_ms)
            }
            Self::RateLimited {
                mirror,
                retry_after_secs,
            } => {
                if let Some(secs) = retry_after_secs {
                    format!(
                        "Mirror {} rate limited, retry after {} seconds",
                        mirror, secs
                    )
                } else {
                    format!("Mirror {} rate limited, retry in 30 seconds", mirror)
                }
            }
            Self::NetworkError { reason, is_transient } => {
                if *is_transient {
                    format!("Transient network error: {}", reason)
                } else {
                    format!("Network error: {}", reason)
                }
            }
            Self::DiskError { path, reason } => {
                format!("Disk error at {}: {}", path, reason)
            }
            Self::RangeNotSupported { mirror } => {
                format!("Mirror {} doesn't support partial downloads (Range requests)", mirror)
            }
            Self::IntegrityViolation { expected, actual } => {
                format!("Integrity check failed: expected {}, got {}", expected, actual)
            }
            Self::ConfigError { reason } => {
                format!("Configuration error: {}", reason)
            }
            Self::ConnectionFailed { mirror, reason } => {
                format!("Failed to connect to {}: {}", mirror, reason)
            }
            Self::RetryExhausted { mirror, attempts } => {
                format!("Retries exhausted: {} failed {} times", mirror, attempts)
            }
        }
    }
}

/// Convert std::io::Error to DownloadError with context preservation
impl From<std::io::Error> for DownloadError {
    fn from(e: std::io::Error) -> Self {
        match e.kind() {
            std::io::ErrorKind::NotFound => Self::DiskError {
                path: String::new(),
                reason: "File not found".into(),
            },
            std::io::ErrorKind::PermissionDenied => Self::DiskError {
                path: String::new(),
                reason: "Permission denied".into(),
            },
            std::io::ErrorKind::Interrupted => Self::NetworkError {
                reason: "Operation interrupted".into(),
                is_transient: true,
            },
            std::io::ErrorKind::InvalidInput => Self::ConfigError {
                reason: e.to_string(),
            },
            _ => Self::NetworkError {
                reason: e.to_string(),
                is_transient: true,
            },
        }
    }
}

impl From<String> for DownloadError {
    fn from(s: String) -> Self {
        // Heuristic parsing for common error patterns
        let lower = s.to_lowercase();

        if lower.contains("timeout") || lower.contains("unreachable") {
            Self::MirrorUnreachable {
                mirror: "unknown".into(),
                duration_ms: 0,
            }
        } else if lower.contains("rate limit") || lower.contains("429") {
            Self::RateLimited {
                mirror: "unknown".into(),
                retry_after_secs: None,
            }
        } else if lower.contains("disk") || lower.contains("space") {
            Self::DiskError {
                path: String::new(),
                reason: s,
            }
        } else if lower.contains("connection") || lower.contains("refused") {
            Self::ConnectionFailed {
                mirror: "unknown".into(),
                reason: s,
            }
        } else {
            Self::NetworkError {
                reason: s,
                is_transient: true,
            }
        }
    }
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.description())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mirror_unreachable_should_failover() {
        let err = DownloadError::MirrorUnreachable {
            mirror: "mirror1.com".into(),
            duration_ms: 5000,
        };
        assert!(err.should_failover());
        assert!(!err.should_retry());
    }

    #[test]
    fn test_disk_error_should_not_failover() {
        let err = DownloadError::DiskError {
            path: "/path".into(),
            reason: "No space".into(),
        };
        assert!(!err.should_failover());
        assert!(!err.should_retry());
        assert!(err.is_permanent());
    }

    #[test]
    fn test_rate_limit_backoff() {
        let err = DownloadError::RateLimited {
            mirror: "mirror1.com".into(),
            retry_after_secs: Some(60),
        };
        assert_eq!(err.backoff_duration().as_secs(), 60);
        assert!(err.should_failover()); // Can failover or retry
        assert!(err.should_retry());
    }

    #[test]
    fn test_transient_network_error_should_retry() {
        let err = DownloadError::NetworkError {
            reason: "Connection reset".into(),
            is_transient: true,
        };
        assert!(err.should_retry());
        assert!(!err.is_permanent());
    }

    #[test]
    fn test_permanent_network_error_should_not_retry() {
        let err = DownloadError::NetworkError {
            reason: "Invalid proxy configuration".into(),
            is_transient: false,
        };
        assert!(!err.should_retry());
        assert!(err.is_permanent());
    }

    #[test]
    fn test_range_not_supported_should_failover() {
        let err = DownloadError::RangeNotSupported {
            mirror: "basic-mirror.com".into(),
        };
        assert!(err.should_failover());
    }

    #[test]
    fn test_integrity_violation_is_permanent() {
        let err = DownloadError::IntegrityViolation {
            expected: "abc123".into(),
            actual: "def456".into(),
        };
        assert!(err.is_permanent());
        assert!(!err.should_failover());
    }

    #[test]
    fn test_string_conversion_timeout() {
        let err = DownloadError::from("Connection timeout after 30s".to_string());
        assert!(matches!(err, DownloadError::MirrorUnreachable { .. }));
    }

    #[test]
    fn test_string_conversion_rate_limit() {
        let err = DownloadError::from("429 Too Many Requests".to_string());
        assert!(matches!(err, DownloadError::RateLimited { .. }));
    }

    #[test]
    fn test_description_is_human_readable() {
        let err = DownloadError::RateLimited {
            mirror: "api.example.com".into(),
            retry_after_secs: Some(45),
        };
        let desc = err.description();
        assert!(desc.contains("api.example.com"));
        assert!(desc.contains("45"));
    }
}
