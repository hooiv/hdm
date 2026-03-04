use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::{Semaphore, OwnedSemaphorePermit};
use url::Url;

/// Manages concurrent connection limits per domain
#[derive(Debug, Clone)]
pub struct ConnectionManager {
    // Map domain -> Semaphore
    // Default limit: 4 connections per domain
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
