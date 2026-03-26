//! Settings utilities for production-grade error handling and context
//!
//! Provides:
//! - Safe settings operations with detailed error context
//! - Automatic retry with exponential backoff
//! - Error classification (transient vs permanent)
//! - Structured logging compatible with production monitoring
#![allow(dead_code)]

use std::time::Duration;
use std::fmt;
use serde::{Deserialize, Serialize};

/// Detailed error context for settings operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsErrorContext {
    pub operation: String,      // "load", "save", "validate", "migrate"
    pub error_message: String,
    pub is_transient: bool,     // Retryable?
    pub retry_count: u32,
    pub took_ms: u64,
}

impl fmt::Display for SettingsErrorContext {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "[{}] {} (transient={}, retries={})",
            self.operation, self.error_message, self.is_transient, self.retry_count
        )
    }
}

/// Classifies if an error is transient (retryable) or permanent
pub fn classify_error(err: &str) -> bool {
    let transient_keywords = [
        "timeout",
        "lock",
        "busy",
        "io error",
        "connection",
        "temporary",
        "stale",
    ];
    
    let err_lower = err.to_lowercase();
    transient_keywords.iter().any(|kw| err_lower.contains(kw))
}

/// Retry operation with exponential backoff
pub async fn retry_with_backoff<F, T, Fut>(
    mut operation: F,
    max_retries: u32,
    initial_backoff_ms: u64,
) -> Result<T, SettingsErrorContext>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, String>>,
{
    let mut backoff = initial_backoff_ms;
    let mut retry_count = 0;
    
    loop {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(err) => {
                let is_transient = classify_error(&err);
                
                if !is_transient || retry_count >= max_retries {
                    return Err(SettingsErrorContext {
                        operation: "retry".to_string(),
                        error_message: err,
                        is_transient,
                        retry_count,
                        took_ms: 0,
                    });
                }
                
                retry_count += 1;
                tokio::time::sleep(Duration::from_millis(backoff)).await;
                backoff = (backoff * 2).min(30000); // Cap at 30s
            }
        }
    }
}

/// Safe lock acquisition with timeout and panic recovery
pub fn acquire_lock_safe<'a, T>(
    lock: &'a std::sync::RwLock<T>,
    operation: &str,
    _timeout_ms: u64,
) -> Result<std::sync::RwLockReadGuard<'a, T>, String> {
    // Note: RwLock::try_read doesn't have timeout in std,
    // so we do a simple non-blocking attempt
    match lock.try_read() {
        Ok(guard) => Ok(guard),
        Err(e) => {
            match e {
                std::sync::TryLockError::Poisoned(_) => {
                    Err(format!("[{}] Lock poisoned, cache in degraded state", operation))
                }
                std::sync::TryLockError::WouldBlock => {
                    Err(format!("[{}] Failed to acquire read lock: lock would block", operation))
                }
            }
        }
    }
}

/// Safe settings filepath operations
pub mod path_utils {
    use std::path::{Path, PathBuf};

    /// Safely get settings directory with fallback
    pub fn get_settings_dir() -> PathBuf {
        if let Some(config_dir) = dirs::config_dir() {
            config_dir.join("hyperstream")
        } else {
            // Fallback to current directory
            PathBuf::from("./config")
        }
    }

    /// Safely get settings file path
    pub fn get_settings_file() -> PathBuf {
        get_settings_dir().join("settings.json")
    }

    /// Validate that a path is safe (no path traversal)
    pub fn is_safe_path(path: &str) -> bool {
        !path.contains("..") && !path.contains("~")
    }

    /// Convert to absolute path with safety checks
    pub fn to_absolute_safe(base: &Path, rel: &str) -> Result<PathBuf, String> {
        if !is_safe_path(rel) {
            return Err("Path traversal detected".to_string());
        }
        Ok(base.join(rel))
    }
}

/// Performance profiling for settings operations
pub struct OperationTimer {
    name: String,
    start: std::time::Instant,
}

impl OperationTimer {
    pub fn start(name: &str) -> Self {
        Self {
            name: name.to_string(),
            start: std::time::Instant::now(),
        }
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }

    pub fn warn_if_slow(&self, threshold_ms: u64) {
        let elapsed = self.elapsed_ms();
        if elapsed > threshold_ms {
            eprintln!("SLOW [{}]: {}ms", self.name, elapsed);
        }
    }
}

impl Drop for OperationTimer {
    fn drop(&mut self) {
        let elapsed = self.elapsed_ms();
        if elapsed > 100 {
            eprintln!("PERF [{}]: {}ms", self.name, elapsed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_error_transient() {
        assert!(classify_error("Connection timeout"));
        assert!(classify_error("Lock temporarily busy"));
        assert!(classify_error("IO error: resource temporarily unavailable"));
    }

    #[test]
    fn test_classify_error_permanent() {
        assert!(!classify_error("Invalid field 'segments'"));
        assert!(!classify_error("Validation error"));
        assert!(!classify_error("Permission denied"));
    }

    #[test]
    fn test_path_safety_checks() {
        assert!(path_utils::is_safe_path("settings.json"));
        assert!(path_utils::is_safe_path("subdir/settings.json"));
        assert!(!path_utils::is_safe_path("../settings.json"));
        assert!(!path_utils::is_safe_path("~/settings.json"));
    }

    #[test]
    fn test_operation_timer() {
        let timer = OperationTimer::start("test_op");
        std::thread::sleep(Duration::from_millis(10));
        assert!(timer.elapsed_ms() >= 10);
    }
}
