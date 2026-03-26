//! Per-download bandwidth allocation with priority-aware fair-share scheduling.
//!
//! The global `GLOBAL_LIMITER` caps total bandwidth across all downloads.
//! This module distributes that total budget among active downloads, giving
//! each one its own token-bucket limiter whose rate is dynamically adjusted.
//!
//! Workers call `ALLOCATOR.acquire(download_id, bytes).await` which respects
//! the per-download allocation.  A background task rebalances every 2 seconds.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};
use tokio::time::{Duration, Instant};

// ─── Per-download token bucket (lightweight copy of SpeedLimiter) ────────────

struct TokenBucket {
    limit_bps: AtomicU64,
    tokens: AtomicU64,
    last_refill: Mutex<Instant>,
}

impl TokenBucket {
    fn new(limit_bps: u64) -> Self {
        Self {
            limit_bps: AtomicU64::new(limit_bps),
            tokens: AtomicU64::new(limit_bps),
            last_refill: Mutex::new(Instant::now()),
        }
    }

    fn set_limit(&self, bps: u64) {
        let old = self.limit_bps.swap(bps, Ordering::SeqCst);
        // If limit increased, grant the difference immediately so downloads ramp up fast
        if bps > old {
            let _ = self.tokens.fetch_add(bps - old, Ordering::SeqCst);
        }
    }

    async fn acquire(&self, requested: u64) -> u64 {
        let limit = self.limit_bps.load(Ordering::Relaxed);
        if limit == 0 {
            return requested; // unlimited
        }

        loop {
            // Refill tokens
            if let Ok(mut last) = self.last_refill.try_lock() {
                let now = Instant::now();
                let elapsed = now.duration_since(*last);
                if elapsed >= Duration::from_millis(50) {
                    let refill = (limit as f64 * elapsed.as_secs_f64()) as u64;
                    if refill > 0 {
                        loop {
                            let cur = self.tokens.load(Ordering::Acquire);
                            let new = (cur + refill).min(limit);
                            if self.tokens.compare_exchange(cur, new, Ordering::SeqCst, Ordering::Relaxed).is_ok() {
                                break;
                            }
                        }
                        *last = now;
                    }
                }
            }

            // Try to consume
            let cur = self.tokens.load(Ordering::Acquire);
            if cur > 0 {
                let consume = requested.min(cur);
                if self.tokens.compare_exchange(cur, cur - consume, Ordering::SeqCst, Ordering::Relaxed).is_ok() {
                    return consume;
                }
                continue; // CAS race, retry
            }

            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

// ─── Download registration ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthConfig {
    /// Priority 1-10 (10 = highest). Default: 5.
    #[serde(default = "default_priority")]
    pub priority: u8,
    /// Guaranteed minimum bytes/sec. The allocator will try to honour this
    /// even if it means other downloads get less. Default: 0 (no guarantee).
    #[serde(default)]
    pub min_bps: u64,
    /// Hard cap bytes/sec. 0 = follow allocator. Default: 0.
    #[serde(default)]
    pub max_bps: u64,
}

fn default_priority() -> u8 { 5 }

impl Default for BandwidthConfig {
    fn default() -> Self {
        Self { priority: 5, min_bps: 0, max_bps: 0 }
    }
}

struct DownloadEntry {
    config: BandwidthConfig,
    bucket: Arc<TokenBucket>,
    allocated_bps: u64,
}

// ─── Allocator snapshot (for UI) ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct AllocationSnapshot {
    pub download_id: String,
    pub priority: u8,
    pub allocated_bps: u64,
    pub min_bps: u64,
    pub max_bps: u64,
}

// ─── Core allocator ─────────────────────────────────────────────────────────

pub struct BandwidthAllocator {
    downloads: Mutex<HashMap<String, DownloadEntry>>,
}

impl BandwidthAllocator {
    fn new() -> Self {
        Self {
            downloads: Mutex::new(HashMap::new()),
        }
    }

    /// Register a download. Call this when a download starts.
    /// If the download is already registered, its config is updated.
    pub fn register(&self, id: &str, config: BandwidthConfig) {
        let mut map = self.downloads.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = map.get_mut(id) {
            entry.config = config;
        } else {
            map.insert(id.to_string(), DownloadEntry {
                config,
                bucket: Arc::new(TokenBucket::new(0)), // 0 = unlimited until first rebalance
                allocated_bps: 0,
            });
        }
    }

    /// Deregister a download. Call this when a download finishes/errors/pauses.
    pub fn deregister(&self, id: &str) {
        let mut map = self.downloads.lock().unwrap_or_else(|e| e.into_inner());
        map.remove(id);
    }

    /// Acquire bytes for a specific download (per-download throttle).
    /// Returns how many bytes are allowed; blocks if necessary.
    /// If the download isn't registered, returns `requested` (no limit).
    pub async fn acquire(&self, id: &str, requested: u64) -> u64 {
        let bucket = {
            let map = self.downloads.lock().unwrap_or_else(|e| e.into_inner());
            map.get(id).map(|e| e.bucket.clone())
        };
        match bucket {
            Some(b) => b.acquire(requested).await,
            None => requested,
        }
    }

    /// Rebalance allocations across all active downloads.
    /// Called by the background scheduler every 2 seconds.
    pub fn rebalance(&self) {
        let global_limit = crate::speed_limiter::GLOBAL_LIMITER.get_limit();
        // If global limiting is disabled, set all per-download limits to 0 (unlimited)
        if global_limit == 0 {
            let map = self.downloads.lock().unwrap_or_else(|e| e.into_inner());
            for entry in map.values() {
                let cap = entry.config.max_bps;
                entry.bucket.set_limit(cap); // 0 means unlimited
            }
            return;
        }

        let mut map = self.downloads.lock().unwrap_or_else(|e| e.into_inner());
        let count = map.len();
        if count == 0 { return; }

        // Phase 1: compute total weight (priority squared for steeper differentiation)
        let total_weight: f64 = map.values()
            .map(|e| (e.config.priority as f64).powi(2))
            .sum();

        if total_weight == 0.0 { return; }

        // Phase 2: guarantee minimums — subtract from budget
        let mut budget = global_limit;
        let mut guaranteed: HashMap<String, u64> = HashMap::new();
        for (id, entry) in map.iter() {
            if entry.config.min_bps > 0 {
                let g = entry.config.min_bps.min(budget);
                guaranteed.insert(id.clone(), g);
                budget = budget.saturating_sub(g);
            }
        }

        // Phase 3: distribute remaining budget by weighted fair-share
        // Only downloads that haven't hit their max_bps get a share
        let mut allocations: HashMap<String, u64> = HashMap::new();

        // First pass: compute ideal shares
        for (id, entry) in map.iter() {
            let weight = (entry.config.priority as f64).powi(2);
            let share = ((weight / total_weight) * budget as f64) as u64;
            let base = guaranteed.get(id).copied().unwrap_or(0);
            let total = base + share;

            // Apply max cap
            let capped = if entry.config.max_bps > 0 {
                total.min(entry.config.max_bps)
            } else {
                total
            };
            allocations.insert(id.clone(), capped);
        }

        // Phase 4: redistribute excess from capped downloads
        let total_allocated: u64 = allocations.values().sum();
        if total_allocated < global_limit {
            let excess = global_limit - total_allocated;
            // Give excess to uncapped downloads proportionally
            let uncapped_weight: f64 = map.iter()
                .filter(|(id, e)| {
                    e.config.max_bps == 0 || allocations.get(*id).copied().unwrap_or(0) < e.config.max_bps
                })
                .map(|(_, e)| (e.config.priority as f64).powi(2))
                .sum();

            if uncapped_weight > 0.0 {
                for (id, entry) in map.iter() {
                    let current = allocations.get(id).copied().unwrap_or(0);
                    let at_cap = entry.config.max_bps > 0 && current >= entry.config.max_bps;
                    if !at_cap {
                        let w = (entry.config.priority as f64).powi(2);
                        let bonus = ((w / uncapped_weight) * excess as f64) as u64;
                        let new_total = current + bonus;
                        let capped = if entry.config.max_bps > 0 {
                            new_total.min(entry.config.max_bps)
                        } else {
                            new_total
                        };
                        allocations.insert(id.clone(), capped);
                    }
                }
            }
        }

        // Phase 5: apply allocations to buckets
        for (id, entry) in map.iter_mut() {
            let alloc = allocations.get(id).copied().unwrap_or(0);
            entry.allocated_bps = alloc;
            entry.bucket.set_limit(alloc);
        }
    }

    /// Get a snapshot of all allocations (for UI display).
    pub fn snapshot(&self) -> Vec<AllocationSnapshot> {
        let map = self.downloads.lock().unwrap_or_else(|e| e.into_inner());
        map.iter().map(|(id, entry)| AllocationSnapshot {
            download_id: id.clone(),
            priority: entry.config.priority,
            allocated_bps: entry.allocated_bps,
            min_bps: entry.config.min_bps,
            max_bps: entry.config.max_bps,
        }).collect()
    }
}

// ─── Global static ──────────────────────────────────────────────────────────

lazy_static::lazy_static! {
    pub static ref ALLOCATOR: BandwidthAllocator = BandwidthAllocator::new();
}

/// Start the background rebalancer. Call once during app setup.
pub fn start_rebalancer() {
    tauri::async_runtime::spawn(async {
        loop {
            ALLOCATOR.rebalance();
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_deregister() {
        let alloc = BandwidthAllocator::new();
        alloc.register("d1", BandwidthConfig::default());
        assert_eq!(alloc.snapshot().len(), 1);
        alloc.deregister("d1");
        assert_eq!(alloc.snapshot().len(), 0);
    }

    #[test]
    fn test_priority_weighting() {
        let alloc = BandwidthAllocator::new();
        alloc.register("low", BandwidthConfig { priority: 1, min_bps: 0, max_bps: 0 });
        alloc.register("high", BandwidthConfig { priority: 10, min_bps: 0, max_bps: 0 });

        // Manually set global limiter for this test
        crate::speed_limiter::GLOBAL_LIMITER.set_limit(1_000_000);
        alloc.rebalance();

        let snap: HashMap<String, u64> = alloc.snapshot().into_iter()
            .map(|s| (s.download_id, s.allocated_bps))
            .collect();

        // Priority 10 (weight 100) should get ~100x more than priority 1 (weight 1)
        let high = snap["high"];
        let low = snap["low"];
        assert!(high > low * 50, "high={} should be >> low={}", high, low);
    }

    #[test]
    fn test_min_guarantee() {
        let alloc = BandwidthAllocator::new();
        alloc.register("guaranteed", BandwidthConfig { priority: 1, min_bps: 500_000, max_bps: 0 });
        alloc.register("normal", BandwidthConfig { priority: 10, min_bps: 0, max_bps: 0 });

        crate::speed_limiter::GLOBAL_LIMITER.set_limit(1_000_000);
        alloc.rebalance();

        let snap: HashMap<String, u64> = alloc.snapshot().into_iter()
            .map(|s| (s.download_id, s.allocated_bps))
            .collect();

        assert!(snap["guaranteed"] >= 500_000, "guaranteed={} should be >= 500000", snap["guaranteed"]);
    }

    #[test]
    fn test_max_cap() {
        let alloc = BandwidthAllocator::new();
        alloc.register("capped", BandwidthConfig { priority: 10, min_bps: 0, max_bps: 100_000 });
        alloc.register("uncapped", BandwidthConfig { priority: 5, min_bps: 0, max_bps: 0 });

        crate::speed_limiter::GLOBAL_LIMITER.set_limit(1_000_000);
        alloc.rebalance();

        let snap: HashMap<String, u64> = alloc.snapshot().into_iter()
            .map(|s| (s.download_id, s.allocated_bps))
            .collect();

        assert!(snap["capped"] <= 100_000, "capped={} should be <= 100000", snap["capped"]);
        // Uncapped should get the excess
        assert!(snap["uncapped"] > 500_000, "uncapped={} should get excess bandwidth", snap["uncapped"]);
    }
}
