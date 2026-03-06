use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::{Semaphore, OwnedSemaphorePermit};
use url::Url;

/// Manages concurrent connection limits per domain
#[derive(Debug, Clone)]
pub struct ConnectionManager {
    // Map domain -> Semaphore
    semaphores: Arc<DashMap<String, Arc<Semaphore>>>,
    default_limit: usize,
}

impl ConnectionManager {
    pub fn new(default_limit: usize) -> Self {
        Self {
            semaphores: Arc::new(DashMap::new()),
            default_limit,
        }
    }

    /// Set a custom connection limit for a specific domain.
    /// Must be called BEFORE any `acquire()` for that domain to take effect,
    /// since the semaphore is created lazily on first access.
    pub fn set_domain_limit(&self, domain: &str, limit: usize) {
        let limit = limit.max(1); // At least 1 connection
        self.semaphores.insert(domain.to_string(), Arc::new(Semaphore::new(limit)));
    }

    /// Configure connection limit for a URL using site rules.
    /// Extracts the domain and applies matching site rules if any.
    pub fn configure_for_url(&self, url_str: &str) {
        let domain = self.extract_domain(url_str);
        let effective = crate::site_rules::resolve_config(url_str);
        if let Some(max_conn) = effective.max_connections {
            self.set_domain_limit(&domain, max_conn as usize);
        }
    }

    /// Acquire a permit for the given URL's domain.
    /// The permit is held until the returned `OwnedSemaphorePermit` is dropped.
    pub async fn acquire(&self, url_str: &str) -> Result<OwnedSemaphorePermit, String> {
        let domain = self.extract_domain(url_str);
        
        // Get or create semaphore for this domain
        let semaphore = self.semaphores
            .entry(domain)
            .or_insert_with(|| Arc::new(Semaphore::new(self.default_limit)))
            .clone();

        // Acquire permit (waits if limit reached)
        semaphore.acquire_owned().await
            .map_err(|_| "Semaphore closed unexpectedly".to_string())
    }

    fn extract_domain(&self, url_str: &str) -> String {
        Url::parse(url_str)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_string()))
            .unwrap_or_else(|| "unknown".to_string())
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new(4) // Conservative default
    }
}
