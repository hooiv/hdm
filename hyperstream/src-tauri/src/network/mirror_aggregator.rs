//! Mirror Aggregator: Autonomous mirror discovery and aggregation
//!
//! Periodic background task for discovering, verifying, and injecting mirrors
//! for active downloads, ensuring bandwidth utilization and failover resilience.

use crate::mirror_hunter::DiscoveryEngine;
use crate::network::mirror_scout::{MirrorScout, ScoutStatus};
use crate::core_state::AppState;
use tauri::{AppHandle, Emitter, Manager};
use std::sync::Arc;
use tokio::time::{Duration, interval};
use std::collections::HashMap;
use tokio::sync::RwLock;

/// Health report for a discovered mirror
#[derive(Debug, Clone, serde::Serialize)]
pub struct MirrorHealthReport {
    pub url: String,
    pub status: ScoutStatus,
    pub latency_ms: u64,
    pub verified_at_ms: u64,
}

pub struct MirrorAggregator {
    discovery_engine: Arc<DiscoveryEngine>,
    scout: Arc<MirrorScout>,
    active_mirrors: Arc<RwLock<HashMap<String, Vec<MirrorHealthReport>>>>,
}

impl MirrorAggregator {
    pub fn new() -> Self {
        Self {
            discovery_engine: Arc::new(DiscoveryEngine::new()),
            scout: Arc::new(MirrorScout::new()),
            active_mirrors: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Starts the background discovery loop.
    pub async fn start_loop(&self, app: AppHandle) {
        let mut interval = interval(Duration::from_secs(60 * 5)); // Check every 5 minutes
        
        loop {
            interval.tick().await;
            self.perform_discovery(&app).await;
        }
    }

    async fn perform_discovery(&self, app: &AppHandle) {
        let state = match app.try_state::<Arc<AppState>>() {
            Some(s) => s,
            None => return,
        };

        let active_downloads: Vec<String> = {
            let downloads = state.downloads.lock().unwrap_or_else(|e| e.into_inner());
            downloads.keys().cloned().collect()
        };

        for download_id in active_downloads {
            self.discover_for_download(&download_id, &state, app).await;
        }
    }

    async fn discover_for_download(&self, download_id: &str, state: &AppState, app: &AppHandle) {
        let (filename, file_size) = {
            let downloads = state.downloads.lock().unwrap_or_else(|e| e.into_inner());
            match downloads.get(download_id) {
                Some(s) => {
                    let path = std::path::Path::new(&s.path);
                    let filename = path.file_name()
                        .and_then(|f| f.to_str())
                        .unwrap_or("download.bin")
                        .to_string();
                    (filename, s.manager.lock().unwrap().file_size)
                },
                None => return,
            }
        };

        // 2. Discover candidates
        let candidates = self.discovery_engine.find_mirrors(&filename, file_size, None, None, None).await;
        
        // 3. Verify candidates
        let mut verified_mirrors = Vec::new();
        for candidate in candidates {
            let scout_res = self.scout.verify_mirror(&candidate.url, file_size, None).await;
            
            if scout_res.status == ScoutStatus::Valid {
                verified_mirrors.push(MirrorHealthReport {
                    url: candidate.url.clone(),
                    status: scout_res.status,
                    latency_ms: scout_res.latency_ms,
                    verified_at_ms: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64,
                });
            }
        }

        // 4. Inject and Emit
        if !verified_mirrors.is_empty() {
            let mut mirrors_map = self.active_mirrors.write().await;
            mirrors_map.insert(download_id.to_string(), verified_mirrors.clone());

            // Inject into active session
            let downloads = state.downloads.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(session) = downloads.get(download_id) {
                let mut dynamic = session.dynamic_mirrors.write().unwrap_or_else(|e| e.into_inner());
                for m in &verified_mirrors {
                    if !dynamic.contains(&m.url) {
                        dynamic.push(m.url.clone());
                    }
                }
            }

            // Emit discovery event
            let _ = app.emit("mirror_discovered", (download_id, verified_mirrors));
        }
    }

    pub async fn get_active_mirrors(&self, download_id: &str) -> Vec<MirrorHealthReport> {
        let mirrors = self.active_mirrors.read().await;
        mirrors.get(download_id).cloned().unwrap_or_default()
    }
}
