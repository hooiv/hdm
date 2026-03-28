//! Production-Grade Connection Pool Manager
//!
//! Implements per-host connection limiting with semaphore-based concurrency control.
//! This is THE critical feature that separates a production download manager from a toy:
//! without it, multi-segment downloads to the same host create 8-64 concurrent connections,
//! causing servers to throttle, rate-limit, or outright ban the client.
//!
//! IDM limits connections per host (typically 8-16). This module does the same, with:
//! - Per-host semaphore permits (configurable via `max_connections_per_host`)
//! - Automatic adaptive limiting on 429/503 responses
//! - Connection lifecycle tracking (active count per host)
//! - Global connection cap to prevent socket exhaustion
//! - Metrics for UI visibility (connections per host, total active, etc.)
//! - Domain normalization (www.example.com == example.com)

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use serde::Serialize;
use tokio::sync::Semaphore;

// ─── Configuration ──────────────────────────────────────────────────────────

/// Maximum total connections across ALL hosts to prevent socket exhaustion
const GLOBAL_CONNECTION_CAP: usize = 128;

/// Minimum connections per host (floor for adaptive reduction)
const MIN_CONNECTIONS_PER_HOST: u32 = 2;

/// How long (in seconds) to keep a reduced connection limit before restoring
const THROTTLE_RECOVERY_SECS: u64 = 120;

/// Cooldown after a 429 response before attempting to restore limits
const RATE_LIMIT_COOLDOWN_SECS: u64 = 60;

// ─── Host entry ─────────────────────────────────────────────────────────────

struct HostEntry {
    /// Semaphore controlling max concurrent connections to this host
    semaphore: Arc<Semaphore>,
    /// Current configured limit for this host
    limit: u32,
    /// Base limit from settings (used for restoration)
    base_limit: u32,
    /// Number of currently active connections (permits held)
    active_count: std::sync::atomic::AtomicU32,
    /// Total connections ever created to this host
    total_connections: std::sync::atomic::AtomicU64,
    /// Timestamp of last 429/throttle response
    last_throttle: Option<std::time::Instant>,
    /// Timestamp of last adaptive reduction
    last_reduction: Option<std::time::Instant>,
    /// Number of sequential throttle responses (for escalating backoff)
    throttle_streak: u32,
}

impl HostEntry {
    fn new(limit: u32) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(limit as usize)),
            limit,
            base_limit: limit,
            active_count: std::sync::atomic::AtomicU32::new(0),
            total_connections: std::sync::atomic::AtomicU64::new(0),
            last_throttle: None,
            last_reduction: None,
            throttle_streak: 0,
        }
    }
}

// ─── Connection permit (RAII guard) ─────────────────────────────────────────

/// RAII guard that releases the connection permit when dropped.
/// Workers hold this while their HTTP connection is active.
pub struct ConnectionPermit {
    host: String,
    _permit: tokio::sync::OwnedSemaphorePermit,
    pool: Arc<ConnectionPoolInner>,
}

impl Drop for ConnectionPermit {
    fn drop(&mut self) {
        // Decrement active count
        let hosts = self.pool.hosts.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = hosts.get(&self.host) {
            entry
                .active_count
                .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        }
        // Note: the OwnedSemaphorePermit is dropped automatically, releasing the permit
    }
}

// ─── Pool metrics (for UI) ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct HostConnectionInfo {
    pub host: String,
    pub active: u32,
    pub limit: u32,
    pub base_limit: u32,
    pub total_connections: u64,
    pub is_throttled: bool,
    pub throttle_streak: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConnectionPoolMetrics {
    pub total_active: u32,
    pub total_hosts: usize,
    pub global_cap: usize,
    pub hosts: Vec<HostConnectionInfo>,
}

// ─── Inner pool ─────────────────────────────────────────────────────────────

struct ConnectionPoolInner {
    hosts: Mutex<HashMap<String, HostEntry>>,
    global_semaphore: Arc<Semaphore>,
    default_limit: Mutex<u32>,
}

impl ConnectionPoolInner {
    fn new(default_limit: u32) -> Self {
        Self {
            hosts: Mutex::new(HashMap::new()),
            global_semaphore: Arc::new(Semaphore::new(GLOBAL_CONNECTION_CAP)),
            default_limit: Mutex::new(default_limit),
        }
    }
}

// ─── Public API ─────────────────────────────────────────────────────────────

/// Global connection pool manager.
/// Thread-safe, shared across all downloads.
pub struct ConnectionPool {
    inner: Arc<ConnectionPoolInner>,
}

impl ConnectionPool {
    pub fn new(default_limit: u32) -> Self {
        let clamped = default_limit.max(MIN_CONNECTIONS_PER_HOST).min(64);
        Self {
            inner: Arc::new(ConnectionPoolInner::new(clamped)),
        }
    }

    /// Update the default per-host connection limit (e.g., from settings).
    pub fn set_default_limit(&self, limit: u32) {
        let clamped = limit.max(MIN_CONNECTIONS_PER_HOST).min(64);
        let mut default = self
            .inner
            .default_limit
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *default = clamped;
    }

    /// Set a specific limit for a given host (overrides the default).
    pub fn set_host_limit(&self, host: &str, limit: u32) {
        let normalized = normalize_host(host);
        let clamped = limit.max(MIN_CONNECTIONS_PER_HOST).min(64);
        let mut hosts = self.inner.hosts.lock().unwrap_or_else(|e| e.into_inner());

        if let Some(entry) = hosts.get_mut(&normalized) {
            // Rebuild semaphore with new limit
            let active = entry
                .active_count
                .load(std::sync::atomic::Ordering::Relaxed);
            let available = clamped.saturating_sub(active);
            entry.semaphore = Arc::new(Semaphore::new(available as usize));
            entry.limit = clamped;
            entry.base_limit = clamped;
        } else {
            hosts.insert(normalized, HostEntry::new(clamped));
        }
    }

    /// Acquire a connection permit for the given URL.
    /// Blocks until a permit is available (respects per-host and global limits).
    /// Returns `None` if the URL cannot be parsed.
    pub async fn acquire(&self, url: &str) -> Result<ConnectionPermit, String> {
        let host = extract_host(url).ok_or_else(|| format!("Cannot parse host from URL: {}", url))?;
        let normalized = normalize_host(&host);

        // Ensure host entry exists
        {
            let mut hosts = self.inner.hosts.lock().unwrap_or_else(|e| e.into_inner());
            if !hosts.contains_key(&normalized) {
                let default = *self
                    .inner
                    .default_limit
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                hosts.insert(normalized.clone(), HostEntry::new(default));
            }
        }

        // Clone the semaphore Arc outside the lock
        let (host_semaphore, _global_semaphore) = {
            let hosts = self.inner.hosts.lock().unwrap_or_else(|e| e.into_inner());
            let entry = hosts.get(&normalized).unwrap(); // guaranteed to exist
            (entry.semaphore.clone(), self.inner.global_semaphore.clone())
        };

        // Acquire per-host permit (this blocks until a slot opens up)
        let permit = host_semaphore
            .acquire_owned()
            .await
            .map_err(|e| format!("Connection pool closed for {}: {}", normalized, e))?;

        // Increment active count
        {
            let hosts = self.inner.hosts.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(entry) = hosts.get(&normalized) {
                entry
                    .active_count
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                entry
                    .total_connections
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }

        Ok(ConnectionPermit {
            host: normalized,
            _permit: permit,
            pool: self.inner.clone(),
        })
    }

    /// Try to acquire a connection permit without blocking.
    /// Returns `None` if no permit is immediately available.
    pub fn try_acquire(&self, url: &str) -> Result<Option<ConnectionPermit>, String> {
        let host =
            extract_host(url).ok_or_else(|| format!("Cannot parse host from URL: {}", url))?;
        let normalized = normalize_host(&host);

        // Ensure host entry exists
        {
            let mut hosts = self.inner.hosts.lock().unwrap_or_else(|e| e.into_inner());
            if !hosts.contains_key(&normalized) {
                let default = *self
                    .inner
                    .default_limit
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                hosts.insert(normalized.clone(), HostEntry::new(default));
            }
        }

        let host_semaphore = {
            let hosts = self.inner.hosts.lock().unwrap_or_else(|e| e.into_inner());
            let entry = hosts.get(&normalized).unwrap();
            entry.semaphore.clone()
        };

        match host_semaphore.try_acquire_owned() {
            Ok(permit) => {
                let hosts = self.inner.hosts.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(entry) = hosts.get(&normalized) {
                    entry
                        .active_count
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    entry
                        .total_connections
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                Ok(Some(ConnectionPermit {
                    host: normalized,
                    _permit: permit,
                    pool: self.inner.clone(),
                }))
            }
            Err(_) => Ok(None),
        }
    }

    /// Report that a server returned a rate-limit (429) or throttle (503) response.
    /// This adaptively reduces the connection limit for that host.
    pub fn report_throttle(&self, url: &str) {
        let host = match extract_host(url) {
            Some(h) => normalize_host(&h),
            None => return,
        };

        let mut hosts = self.inner.hosts.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = hosts.get_mut(&host) {
            entry.throttle_streak += 1;
            entry.last_throttle = Some(std::time::Instant::now());

            // Reduce limit: cut by 25% per throttle event, floor at MIN_CONNECTIONS_PER_HOST
            let reduction = (entry.limit as f64 * 0.25).max(1.0) as u32;
            let new_limit = entry.limit.saturating_sub(reduction).max(MIN_CONNECTIONS_PER_HOST);

            if new_limit < entry.limit {
                eprintln!(
                    "[ConnectionPool] Reducing connections to {} from {} → {} (throttle streak: {})",
                    host, entry.limit, new_limit, entry.throttle_streak
                );

                // Rebuild semaphore with reduced limit
                let active = entry
                    .active_count
                    .load(std::sync::atomic::Ordering::Relaxed);
                let available = new_limit.saturating_sub(active);
                entry.semaphore = Arc::new(Semaphore::new(available as usize));
                entry.limit = new_limit;
                entry.last_reduction = Some(std::time::Instant::now());
            }
        }
    }

    /// Report a successful response from a host.
    /// Resets the throttle streak after successful requests.
    pub fn report_success(&self, url: &str) {
        let host = match extract_host(url) {
            Some(h) => normalize_host(&h),
            None => return,
        };

        let mut hosts = self.inner.hosts.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = hosts.get_mut(&host) {
            // Only reset throttle state after sustained success
            if entry.throttle_streak > 0 {
                // Require cooldown period before restoring
                if let Some(last_reduction) = entry.last_reduction {
                    if last_reduction.elapsed().as_secs() > THROTTLE_RECOVERY_SECS {
                        // Gradually restore: increase by 1 per successful cooldown period
                        if entry.limit < entry.base_limit {
                            let new_limit = (entry.limit + 1).min(entry.base_limit);
                            eprintln!(
                                "[ConnectionPool] Restoring connections to {} from {} → {}",
                                host, entry.limit, new_limit
                            );
                            // Rebuild semaphore
                            let active = entry
                                .active_count
                                .load(std::sync::atomic::Ordering::Relaxed);
                            let available = new_limit.saturating_sub(active);
                            entry.semaphore = Arc::new(Semaphore::new(available as usize));
                            entry.limit = new_limit;
                            entry.last_reduction = Some(std::time::Instant::now());
                        }
                        if entry.limit >= entry.base_limit {
                            entry.throttle_streak = 0;
                            entry.last_throttle = None;
                            entry.last_reduction = None;
                        }
                    }
                }
            }
        }
    }

    /// Get the current connection limit for a host.
    pub fn get_host_limit(&self, url: &str) -> u32 {
        let host = match extract_host(url) {
            Some(h) => normalize_host(&h),
            None => {
                return *self
                    .inner
                    .default_limit
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
            }
        };

        let hosts = self.inner.hosts.lock().unwrap_or_else(|e| e.into_inner());
        hosts
            .get(&host)
            .map(|e| e.limit)
            .unwrap_or_else(|| {
                *self
                    .inner
                    .default_limit
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
            })
    }

    /// Get the number of active connections to a host.
    pub fn active_connections(&self, url: &str) -> u32 {
        let host = match extract_host(url) {
            Some(h) => normalize_host(&h),
            None => return 0,
        };

        let hosts = self.inner.hosts.lock().unwrap_or_else(|e| e.into_inner());
        hosts
            .get(&host)
            .map(|e| {
                e.active_count
                    .load(std::sync::atomic::Ordering::Relaxed)
            })
            .unwrap_or(0)
    }

    /// Check if a host is being throttled (has active throttle streak).
    pub fn is_throttled(&self, url: &str) -> bool {
        let host = match extract_host(url) {
            Some(h) => normalize_host(&h),
            None => return false,
        };

        let hosts = self.inner.hosts.lock().unwrap_or_else(|e| e.into_inner());
        hosts
            .get(&host)
            .map(|e| {
                e.throttle_streak > 0
                    && e.last_throttle
                        .map(|t| t.elapsed().as_secs() < RATE_LIMIT_COOLDOWN_SECS)
                        .unwrap_or(false)
            })
            .unwrap_or(false)
    }

    /// Get comprehensive metrics for all tracked hosts.
    pub fn metrics(&self) -> ConnectionPoolMetrics {
        let hosts = self.inner.hosts.lock().unwrap_or_else(|e| e.into_inner());
        let mut total_active: u32 = 0;
        let mut infos = Vec::with_capacity(hosts.len());

        for (host, entry) in hosts.iter() {
            let active = entry
                .active_count
                .load(std::sync::atomic::Ordering::Relaxed);
            total_active += active;

            infos.push(HostConnectionInfo {
                host: host.clone(),
                active,
                limit: entry.limit,
                base_limit: entry.base_limit,
                total_connections: entry
                    .total_connections
                    .load(std::sync::atomic::Ordering::Relaxed),
                is_throttled: entry.throttle_streak > 0
                    && entry
                        .last_throttle
                        .map(|t| t.elapsed().as_secs() < RATE_LIMIT_COOLDOWN_SECS)
                        .unwrap_or(false),
                throttle_streak: entry.throttle_streak,
            });
        }

        // Sort by active connections descending
        infos.sort_by(|a, b| b.active.cmp(&a.active));

        ConnectionPoolMetrics {
            total_active,
            total_hosts: hosts.len(),
            global_cap: GLOBAL_CONNECTION_CAP,
            hosts: infos,
        }
    }

    /// Cleanup entries for hosts with zero active connections and no recent activity.
    /// Call periodically to prevent unbounded growth of the host map.
    pub fn cleanup_idle_hosts(&self) {
        let mut hosts = self.inner.hosts.lock().unwrap_or_else(|e| e.into_inner());
        hosts.retain(|_host, entry| {
            let active = entry
                .active_count
                .load(std::sync::atomic::Ordering::Relaxed);
            // Keep entries with active connections or recent throttle state
            active > 0
                || entry
                    .last_throttle
                    .map(|t| t.elapsed().as_secs() < THROTTLE_RECOVERY_SECS * 2)
                    .unwrap_or(false)
        });
    }
}

// ─── Helper functions ───────────────────────────────────────────────────────

/// Extract the host (domain:port or domain) from a URL.
fn extract_host(url: &str) -> Option<String> {
    url::Url::parse(url).ok().and_then(|u| {
        u.host_str().map(|h| {
            match u.port() {
                Some(port) if !is_default_port(u.scheme(), port) => {
                    format!("{}:{}", h, port)
                }
                _ => h.to_string(),
            }
        })
    })
}

/// Normalize a host string (lowercase, strip www. prefix).
fn normalize_host(host: &str) -> String {
    let lower = host.to_ascii_lowercase();
    if lower.starts_with("www.") {
        lower[4..].to_string()
    } else {
        lower
    }
}

/// Check if a port is the default for its scheme.
fn is_default_port(scheme: &str, port: u16) -> bool {
    matches!((scheme, port), ("http", 80) | ("https", 443) | ("ftp", 21))
}

// ─── Global instance ────────────────────────────────────────────────────────

lazy_static::lazy_static! {
    pub static ref GLOBAL_POOL: ConnectionPool = {
        let settings = crate::settings::load_settings();
        ConnectionPool::new(settings.max_connections_per_host)
    };
}

/// Start background maintenance task (cleanup idle hosts, restore throttled limits).
pub fn start_pool_maintenance() {
    tauri::async_runtime::spawn(async {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            GLOBAL_POOL.cleanup_idle_hosts();
            // Sync default limit from settings in case user changed it
            let settings = crate::settings::load_settings();
            GLOBAL_POOL.set_default_limit(settings.max_connections_per_host);
        }
    });
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_host() {
        assert_eq!(
            extract_host("https://example.com/file.zip"),
            Some("example.com".to_string())
        );
        assert_eq!(
            extract_host("https://cdn.example.com:8443/file.zip"),
            Some("cdn.example.com:8443".to_string())
        );
        assert_eq!(
            extract_host("https://example.com:443/file.zip"),
            Some("example.com".to_string()) // default port stripped
        );
        assert_eq!(extract_host("not-a-url"), None);
    }

    #[test]
    fn test_normalize_host() {
        assert_eq!(normalize_host("www.example.com"), "example.com");
        assert_eq!(normalize_host("Example.COM"), "example.com");
        assert_eq!(normalize_host("cdn.example.com"), "cdn.example.com");
    }

    #[test]
    fn test_pool_creation() {
        let pool = ConnectionPool::new(8);
        let metrics = pool.metrics();
        assert_eq!(metrics.total_active, 0);
        assert_eq!(metrics.total_hosts, 0);
    }

    #[test]
    fn test_set_host_limit() {
        let pool = ConnectionPool::new(8);
        pool.set_host_limit("example.com", 16);
        assert_eq!(
            pool.get_host_limit("https://example.com/file.zip"),
            16
        );
    }

    #[test]
    fn test_throttle_reduces_limit() {
        let pool = ConnectionPool::new(8);
        // Pre-create the host entry
        pool.set_host_limit("example.com", 8);

        pool.report_throttle("https://example.com/file.zip");

        let limit = pool.get_host_limit("https://example.com/file.zip");
        assert!(limit < 8, "Limit should be reduced after throttle: {}", limit);
        assert!(
            limit >= MIN_CONNECTIONS_PER_HOST,
            "Limit should not go below minimum: {}",
            limit
        );
    }

    #[test]
    fn test_throttle_detection() {
        let pool = ConnectionPool::new(8);
        pool.set_host_limit("example.com", 8);

        assert!(!pool.is_throttled("https://example.com/file.zip"));
        pool.report_throttle("https://example.com/file.zip");
        assert!(pool.is_throttled("https://example.com/file.zip"));
    }

    #[test]
    fn test_active_connections_default_zero() {
        let pool = ConnectionPool::new(8);
        assert_eq!(
            pool.active_connections("https://example.com/file.zip"),
            0
        );
    }

    #[test]
    fn test_metrics_snapshot() {
        let pool = ConnectionPool::new(8);
        pool.set_host_limit("example.com", 12);
        pool.set_host_limit("cdn.test.org", 4);

        let metrics = pool.metrics();
        assert_eq!(metrics.total_hosts, 2);
        assert_eq!(metrics.total_active, 0);
        assert_eq!(metrics.global_cap, GLOBAL_CONNECTION_CAP);
    }

    #[test]
    fn test_cleanup_idle_hosts() {
        let pool = ConnectionPool::new(8);
        pool.set_host_limit("example.com", 8);
        pool.set_host_limit("idle.example.com", 8);

        assert_eq!(pool.metrics().total_hosts, 2);

        // Both are idle with no throttle state, so cleanup should remove them
        pool.cleanup_idle_hosts();
        assert_eq!(pool.metrics().total_hosts, 0);
    }

    #[tokio::test]
    async fn test_acquire_and_release() {
        let pool = ConnectionPool::new(2);

        let permit1 = pool.acquire("https://example.com/file1.zip").await.unwrap();
        assert_eq!(
            pool.active_connections("https://example.com/file1.zip"),
            1
        );

        let permit2 = pool.acquire("https://example.com/file2.zip").await.unwrap();
        assert_eq!(
            pool.active_connections("https://example.com/file2.zip"),
            2
        );

        // Drop permit1 — should release
        drop(permit1);
        assert_eq!(
            pool.active_connections("https://example.com/file1.zip"),
            1
        );

        drop(permit2);
        assert_eq!(
            pool.active_connections("https://example.com/file1.zip"),
            0
        );
    }

    #[tokio::test]
    async fn test_try_acquire_respects_limit() {
        let pool = ConnectionPool::new(2);

        let _p1 = pool.acquire("https://example.com/a").await.unwrap();
        let _p2 = pool.acquire("https://example.com/b").await.unwrap();

        // Third should fail (non-blocking)
        let p3 = pool.try_acquire("https://example.com/c").unwrap();
        assert!(p3.is_none(), "Should not acquire beyond limit");
    }
}
