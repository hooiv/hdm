//! Failover Metrics & Observability
//!
//! Tracks statistics about failovers, recovery attempts, and overall system health.
//! Enables monitoring and optimization of the failover system.

use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// Metrics for a single failover event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailoverEvent {
    pub timestamp_ms: u64,
    pub download_id: String,
    pub from_mirror: String,
    pub to_mirrors: Vec<String>,
    pub success: bool,
    pub duration_ms: u64,
    pub bytes_recovered: u64,
    pub reason: String,
}

/// Overall failover metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailoverMetrics {
    pub total_failovers: u32,
    pub successful_recoveries: u32,
    pub failed_recoveries: u32,
    pub avg_recovery_time_ms: u64,
    pub last_30_min_failovers: u32,
    pub mirrors_recovered: HashMap<String, u32>,      // mirror_url -> times_recovered
    pub mirrors_permanently_disabled: Vec<String>,
    pub total_bytes_recovered: u64,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

impl Default for FailoverMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl FailoverMetrics {
    /// Create new metrics tracker
    pub fn new() -> Self {
        let now = now_ms();
        Self {
            total_failovers: 0,
            successful_recoveries: 0,
            failed_recoveries: 0,
            avg_recovery_time_ms: 0,
            last_30_min_failovers: 0,
            mirrors_recovered: HashMap::new(),
            mirrors_permanently_disabled: Vec::new(),
            total_bytes_recovered: 0,
            created_at_ms: now,
            updated_at_ms: now,
        }
    }

    /// Calculate success rate as percentage
    pub fn success_rate_percent(&self) -> f64 {
        let total = self.successful_recoveries + self.failed_recoveries;
        if total == 0 {
            return 100.0;
        }
        (self.successful_recoveries as f64 / total as f64) * 100.0
    }

    /// Calculate MTTR (Mean Time To Recovery)
    pub fn mttr_ms(&self) -> u64 {
        if self.successful_recoveries == 0 {
            return 0;
        }
        self.avg_recovery_time_ms
    }

    /// Has the system been stable recently?
    pub fn is_stable(&self) -> bool {
        self.last_30_min_failovers < 3 && self.failed_recoveries < 2
    }

    /// Apply a failover event to metrics
    pub fn record_failover_attempt(&mut self, event: &FailoverEvent) {
        self.total_failovers += 1;
        self.updated_at_ms = now_ms();

        if event.success {
            self.successful_recoveries += 1;

            // Update average recovery time (exponential smoothing)
            if self.avg_recovery_time_ms == 0 {
                self.avg_recovery_time_ms = event.duration_ms;
            } else {
                self.avg_recovery_time_ms =
                    (self.avg_recovery_time_ms * 7 + event.duration_ms) / 8;
            }

            // Track which mirrors enabled recovery
            for mirror in &event.to_mirrors {
                *self
                    .mirrors_recovered
                    .entry(mirror.clone())
                    .or_insert(0) += 1;
            }

            self.total_bytes_recovered += event.bytes_recovered;
        } else {
            self.failed_recoveries += 1;

            // If mirror failed recovery, add to permanently disabled list
            if !self.mirrors_permanently_disabled.contains(&event.from_mirror) {
                self.mirrors_permanently_disabled
                    .push(event.from_mirror.clone());
            }
        }

        // Update 30-minute counter (simple: just count recent events)
        self.last_30_min_failovers += 1;
    }

    /// Get summary statistics
    pub fn summary(&self) -> String {
        format!(
            "Failover Summary: {} total, {} successful ({:.1}%), {} failed, MTTR: {}ms",
            self.total_failovers,
            self.successful_recoveries,
            self.success_rate_percent(),
            self.failed_recoveries,
            self.mttr_ms()
        )
    }
}

/// Thread-safe failover metrics tracker
pub struct FailoverMetricsTracker {
    metrics: Arc<Mutex<FailoverMetrics>>,
    events: Arc<Mutex<Vec<FailoverEvent>>>,
    max_events: usize,
}

impl Clone for FailoverMetricsTracker {
    fn clone(&self) -> Self {
        Self {
            metrics: Arc::clone(&self.metrics),
            events: Arc::clone(&self.events),
            max_events: self.max_events,
        }
    }
}

impl FailoverMetricsTracker {
    /// Create new tracker
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(Mutex::new(FailoverMetrics::new())),
            events: Arc::new(Mutex::new(Vec::new())),
            max_events: 1000, // Keep last 1000 events
        }
    }

    /// Record a failover event
    pub fn record_event(&self, event: FailoverEvent) {
        let mut metrics = self.metrics.lock().unwrap();
        metrics.record_failover_attempt(&event);
        drop(metrics);

        let mut events = self.events.lock().unwrap();
        events.push(event);

        // Trim old events if we exceed limit
        if events.len() > self.max_events {
            *events = events
                .iter()
                .skip(events.len() - self.max_events)
                .cloned()
                .collect();
        }
    }

    /// Get current metrics snapshot
    pub fn get_metrics(&self) -> FailoverMetrics {
        self.metrics.lock().unwrap().clone()
    }

    /// Get recent events (last N)
    pub fn get_recent_events(&self, count: usize) -> Vec<FailoverEvent> {
        let events = self.events.lock().unwrap();
        events
            .iter()
            .rev()
            .take(count)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    /// Get all events
    pub fn get_all_events(&self) -> Vec<FailoverEvent> {
        self.events.lock().unwrap().clone()
    }

    /// Reset metrics (for testing/admin)
    pub fn reset(&self) {
        let mut metrics = self.metrics.lock().unwrap();
        *metrics = FailoverMetrics::new();
        drop(metrics);

        let mut events = self.events.lock().unwrap();
        events.clear();
    }

    /// Get stats by mirror
    pub fn get_stats_by_mirror(&self) -> HashMap<String, MirrorFailoverStats> {
        let _metrics = self.metrics.lock().unwrap();
        let events = self.events.lock().unwrap();

        let mut stats = HashMap::new();

        for event in events.iter() {
            let entry = stats
                .entry(event.from_mirror.clone())
                .or_insert_with(MirrorFailoverStats::new);

            entry.total_failovers += 1;
            if event.success {
                entry.successful_recoveries += 1;
                entry.total_recovery_time_ms += event.duration_ms;
            } else {
                entry.failed_recoveries += 1;
            }
        }

        // Calculate averages
        for stat in stats.values_mut() {
            if stat.successful_recoveries > 0 {
                stat.avg_recovery_time_ms = stat.total_recovery_time_ms / stat.successful_recoveries as u64;
            }
        }

        stats
    }
}

impl Default for FailoverMetricsTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-mirror failover statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirrorFailoverStats {
    pub total_failovers: u32,
    pub successful_recoveries: u32,
    pub failed_recoveries: u32,
    pub avg_recovery_time_ms: u64,
    pub total_recovery_time_ms: u64,
}

impl MirrorFailoverStats {
    pub fn new() -> Self {
        Self {
            total_failovers: 0,
            successful_recoveries: 0,
            failed_recoveries: 0,
            avg_recovery_time_ms: 0,
            total_recovery_time_ms: 0,
        }
    }

    pub fn success_rate_percent(&self) -> f64 {
        let total = self.successful_recoveries + self.failed_recoveries;
        if total == 0 {
            return 100.0;
        }
        (self.successful_recoveries as f64 / total as f64) * 100.0
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_failover_metrics_calculation() {
        let mut metrics = FailoverMetrics::new();

        let event1 = FailoverEvent {
            timestamp_ms: now_ms(),
            download_id: "dl1".into(),
            from_mirror: "mirror1.com".into(),
            to_mirrors: vec!["mirror2.com".into()],
            success: true,
            duration_ms: 1000,
            bytes_recovered: 1000000,
            reason: "Primary timeout".into(),
        };

        metrics.record_failover_attempt(&event1);
        assert_eq!(metrics.successful_recoveries, 1);
        assert_eq!(metrics.total_failovers, 1);
        assert_eq!(metrics.success_rate_percent(), 100.0);
        assert_eq!(metrics.total_bytes_recovered, 1000000);
    }

    #[test]
    fn test_failover_tracker() {
        let tracker = FailoverMetricsTracker::new();

        let event = FailoverEvent {
            timestamp_ms: now_ms(),
            download_id: "dl1".into(),
            from_mirror: "mirror1.com".into(),
            to_mirrors: vec!["mirror2.com".into()],
            success: true,
            duration_ms: 500,
            bytes_recovered: 500000,
            reason: "Primary failure".into(),
        };

        tracker.record_event(event.clone());

        let metrics = tracker.get_metrics();
        assert_eq!(metrics.total_failovers, 1);
        assert_eq!(metrics.successful_recoveries, 1);

        let recent = tracker.get_recent_events(10);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].download_id, "dl1");
    }

    #[test]
    fn test_mirror_stats_calculation() {
        let tracker = FailoverMetricsTracker::new();

        for i in 0..3 {
            let event = FailoverEvent {
                timestamp_ms: now_ms(),
                download_id: format!("dl{}", i),
                from_mirror: "mirror1.com".into(),
                to_mirrors: vec!["mirror2.com".into()],
                success: i < 2, // 2 successful, 1 failed
                duration_ms: 500,
                bytes_recovered: 500000,
                reason: "Primary failure".into(),
            };
            tracker.record_event(event);
        }

        let stats = tracker.get_stats_by_mirror();
        let mirror_stats = stats.get("mirror1.com").unwrap();

        assert_eq!(mirror_stats.total_failovers, 3);
        assert_eq!(mirror_stats.successful_recoveries, 2);
        assert_eq!(mirror_stats.failed_recoveries, 1);
        assert!((mirror_stats.success_rate_percent() - 66.67).abs() < 1.0);
    }
}
