use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};

use tokio::time::{sleep, Duration};
use rand::Rng;

lazy_static::lazy_static! {
    pub static ref GLOBAL_CHAOS: ChaosConfig = ChaosConfig::default();
}

pub struct ChaosConfig {
    pub enabled: AtomicBool,
    pub latency_ms: AtomicU64,
    pub error_rate_percent: AtomicU64, // 0-100
}

impl Default for ChaosConfig {
    fn default() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            latency_ms: AtomicU64::new(0),
            error_rate_percent: AtomicU64::new(0),
        }
    }
}

impl ChaosConfig {
    pub fn update(&self, enabled: bool, latency_ms: u64, error_rate: u64) {
        self.enabled.store(enabled, Ordering::Relaxed);
        self.latency_ms.store(latency_ms, Ordering::Relaxed);
        self.error_rate_percent.store(error_rate, Ordering::Relaxed);
    }

    pub async fn inject(&self) -> Result<(), String> {
        if !self.enabled.load(Ordering::Relaxed) {
            return Ok(());
        }

        let latency = self.latency_ms.load(Ordering::Relaxed);
        if latency > 0 {
            sleep(Duration::from_millis(latency)).await;
        }

        let error_rate = self.error_rate_percent.load(Ordering::Relaxed);
        if error_rate > 0 {
            let roll = rand::rng().random_range(0..100);
            if roll < error_rate {
                return Err("Simulated Network Failure (Chaos Mode)".to_string());
            }
        }
        
        Ok(())
    }
}

// Helper to check and inject chaos
pub async fn check_chaos() -> Result<(), String> {
    GLOBAL_CHAOS.inject().await
}
