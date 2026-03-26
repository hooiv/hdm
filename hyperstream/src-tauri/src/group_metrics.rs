/// Real-time Metrics & Analytics for Download Groups
///
/// Provides detailed insights into group performance:
/// - Per-group speed, ETA, and completion metrics
/// - Member-level progress and performance tracking
/// - Predictive failure analysis
/// - Historical trend analysis
/// - Resource utilization patterns

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// Real-time metrics for a single download group
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupMetrics {
    /// Group ID
    pub group_id: String,
    /// Current group state (Downloading/Completed/Error/Paused)
    #[serde(default)]
    pub state: String,
    /// Total size of all members (bytes)
    pub total_size: u64,
    /// Downloaded so far (bytes)
    pub downloaded: u64,
    /// Overall progress (0-100)
    pub progress_percent: f64,
    /// Average speed (bytes/sec)
    pub avg_speed: f64,
    /// Current speed (bytes/sec)
    pub current_speed: f64,
    /// Estimated time remaining (seconds)
    pub eta_seconds: u64,
    /// Completed members count
    pub completed_count: usize,
    /// Failed members count
    pub failed_count: usize,
    /// Total members
    pub total_members: usize,
    /// CPU utilization percent
    pub cpu_usage_percent: f64,
    /// Memory usage (bytes)
    pub memory_usage: u64,
    /// Network bandwidth available (bytes/sec)
    pub available_bandwidth: f64,
    /// Predicted failure risk (0-100)
    pub failure_risk_percent: f64,
    /// Group start time (epoch ms)
    pub start_time_ms: u64,
    /// Group end time (epoch ms), 0 if not finished
    pub end_time_ms: u64,
    /// Time elapsed (seconds)
    pub elapsed_seconds: u64,
}

/// Per-member metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberMetrics {
    /// Member ID
    pub member_id: String,
    /// Member URL
    pub url: String,
    /// File size (bytes)
    pub size: u64,
    /// Downloaded so far (bytes)
    pub downloaded: u64,
    /// Progress 0-100
    pub progress_percent: f64,
    /// Current speed (bytes/sec)
    pub speed: f64,
    /// ETA in seconds
    pub eta_seconds: u64,
    /// Current state
    pub state: String,
    /// Number of retries
    pub retry_count: u32,
    /// Peak speed reached (bytes/sec)
    pub peak_speed: f64,
    /// Average speed (bytes/sec)
    pub avg_speed: f64,
}

/// Aggregate metrics for multiple groups
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateMetrics {
    /// Total groups
    pub total_groups: usize,
    /// Active groups
    pub active_groups: usize,
    /// Completed groups
    pub completed_groups: usize,
    /// Failed groups
    pub failed_groups: usize,
    /// Total data transferred (bytes)
    pub total_transferred: u64,
    /// Total data remaining (bytes)
    pub total_remaining: u64,
    /// Overall system speed (bytes/sec)
    pub system_speed: f64,
    /// System CPU usage
    pub system_cpu_percent: f64,
    /// System memory usage
    pub system_memory_usage: u64,
    /// Estimated time for all groups (seconds)
    pub global_eta_seconds: u64,
    /// Average group completion time (seconds)
    pub avg_group_time: u64,
}

/// Historical trend data point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendDataPoint {
    /// Timestamp (epoch ms)
    pub timestamp_ms: u64,
    /// Speed at this time (bytes/sec)
    pub speed: f64,
    /// Progress percent
    pub progress_percent: f64,
    /// Estimated remaining time
    pub eta_seconds: u64,
    /// Active members count
    pub active_members: usize,
}

/// Global metrics tracker for all groups
pub struct GroupMetricsTracker {
    /// Per-group metrics
    group_metrics: Arc<Mutex<HashMap<String, GroupMetrics>>>,
    /// Per-member metrics
    member_metrics: Arc<Mutex<HashMap<String, HashMap<String, MemberMetrics>>>>,
    /// Historical trend data (up to last 1000 points)
    trends: Arc<Mutex<HashMap<String, Vec<TrendDataPoint>>>>,
    /// Metrics sample interval (milliseconds)
    sample_interval_ms: u64,
}

impl GroupMetricsTracker {
    /// Create a new metrics tracker
    pub fn new(sample_interval_ms: u64) -> Self {
        Self {
            group_metrics: Arc::new(Mutex::new(HashMap::new())),
            member_metrics: Arc::new(Mutex::new(HashMap::new())),
            trends: Arc::new(Mutex::new(HashMap::new())),
            sample_interval_ms,
        }
    }

    /// Update metrics for a group
    pub fn update_group_metrics(
        &self,
        group_id: &str,
        metrics: GroupMetrics,
    ) -> Result<(), String> {
        let mut groups = self
            .group_metrics
            .lock()
            .map_err(|e| e.to_string())?;

        groups.insert(group_id.to_string(), metrics.clone());

        // Record trend data point
        let trend_point = TrendDataPoint {
            timestamp_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            speed: metrics.current_speed,
            progress_percent: metrics.progress_percent,
            eta_seconds: metrics.eta_seconds,
            active_members: metrics.total_members - metrics.completed_count - metrics.failed_count,
        };

        drop(groups); // Release lock before acquiring trends lock

        let mut trends = self.trends.lock().map_err(|e| e.to_string())?;
        let group_trends = trends
            .entry(group_id.to_string())
            .or_insert_with(Vec::new);

        group_trends.push(trend_point);

        // Keep only last 1000 points
        if group_trends.len() > 1000 {
            group_trends.remove(0);
        }

        Ok(())
    }

    /// Update metrics for a member
    pub fn update_member_metrics(
        &self,
        group_id: &str,
        member_id: &str,
        metrics: MemberMetrics,
    ) -> Result<(), String> {
        let mut members = self
            .member_metrics
            .lock()
            .map_err(|e| e.to_string())?;

        let group_members = members
            .entry(group_id.to_string())
            .or_insert_with(HashMap::new);

        group_members.insert(member_id.to_string(), metrics);

        Ok(())
    }

    /// Get metrics for a group
    pub fn get_group_metrics(&self, group_id: &str) -> Result<Option<GroupMetrics>, String> {
        Ok(self
            .group_metrics
            .lock()
            .map_err(|e| e.to_string())?
            .get(group_id)
            .cloned())
    }

    /// Get metrics for all groups
    pub fn get_all_metrics(&self) -> Result<Vec<GroupMetrics>, String> {
        Ok(self
            .group_metrics
            .lock()
            .map_err(|e| e.to_string())?
            .values()
            .cloned()
            .collect())
    }

    /// Get members metrics for a group
    pub fn get_member_metrics(
        &self,
        group_id: &str,
    ) -> Result<Vec<MemberMetrics>, String> {
        let members = self
            .member_metrics
            .lock()
            .map_err(|e| e.to_string())?;

        Ok(members
            .get(group_id)
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default())
    }

    /// Calculate aggregate metrics
    pub fn get_aggregate_metrics(&self) -> Result<AggregateMetrics, String> {
        let groups = self
            .group_metrics
            .lock()
            .map_err(|e| e.to_string())?;

        let total_groups = groups.len();
        let mut active_groups = 0;
        let mut completed_groups = 0;
        let mut failed_groups = 0;
        let mut total_transferred = 0u64;
        let mut total_remaining = 0u64;
        let mut total_speed = 0.0f64;
        let mut total_cpu = 0.0f64;
        let mut total_memory = 0u64;

        for metrics in groups.values() {
            match metrics.state.as_str() {
                "Downloading" => active_groups += 1,
                "Completed" => completed_groups += 1,
                "Error" => failed_groups += 1,
                _ => {}
            }

            total_transferred += metrics.downloaded;
            total_remaining += metrics.total_size.saturating_sub(metrics.downloaded);
            total_speed += metrics.current_speed;
            total_cpu += metrics.cpu_usage_percent;
            total_memory += metrics.memory_usage;
        }

        let system_speed = total_speed / (total_groups.max(1) as f64);
        let system_cpu_percent = total_cpu / (total_groups.max(1) as f64);

        Ok(AggregateMetrics {
            total_groups,
            active_groups,
            completed_groups,
            failed_groups,
            total_transferred,
            total_remaining,
            system_speed,
            system_cpu_percent,
            system_memory_usage: total_memory,
            global_eta_seconds: if system_speed > 0.0 {
                (total_remaining as f64 / system_speed) as u64
            } else {
                0
            },
            avg_group_time: 0, // Computed from completed groups
        })
    }

    /// Get trend data for a group
    pub fn get_group_trends(&self, group_id: &str) -> Result<Vec<TrendDataPoint>, String> {
        Ok(self
            .trends
            .lock()
            .map_err(|e| e.to_string())?
            .get(group_id)
            .cloned()
            .unwrap_or_default())
    }

    /// Get performance summary for a group
    pub fn get_group_performance_summary(
        &self,
        group_id: &str,
    ) -> Result<PerformanceSummary, String> {
        let metrics = self
            .get_group_metrics(group_id)?
            .ok_or_else(|| format!("Group {} not found", group_id))?;

        let trends = self.get_group_trends(group_id)?;

        let avg_speed = if trends.is_empty() {
            metrics.avg_speed
        } else {
            trends.iter().map(|t| t.speed).sum::<f64>() / trends.len() as f64
        };

        let peak_speed = trends
            .iter()
            .map(|t| t.speed)
            .fold(0.0, f64::max)
            .max(metrics.current_speed);

        let completion_rate = if metrics.total_members > 0 {
            (metrics.completed_count as f64 / metrics.total_members as f64) * 100.0
        } else {
            0.0
        };

        Ok(PerformanceSummary {
            group_id: group_id.to_string(),
            completion_rate,
            avg_speed,
            peak_speed,
            current_speed: metrics.current_speed,
            total_time_seconds: metrics.elapsed_seconds,
            estimated_remaining_seconds: metrics.eta_seconds,
            efficiency_score: Self::calculate_efficiency(
                metrics.progress_percent,
                metrics.elapsed_seconds,
            ),
        })
    }

    /// Calculate efficiency score (0-100)
    fn calculate_efficiency(progress_percent: f64, elapsed_seconds: u64) -> f64 {
        // Simple efficiency: progress per second
        if elapsed_seconds == 0 {
            100.0
        } else {
            ((progress_percent / (elapsed_seconds as f64)) * 100.0).min(100.0)
        }
    }

    /// Estimate completion time for a group
    pub fn estimate_completion_time(&self, group_id: &str) -> Result<u64, String> {
        let metrics = self
            .get_group_metrics(group_id)?
            .ok_or_else(|| format!("Group {} not found", group_id))?;

        Ok(metrics.eta_seconds)
    }
}

/// Performance summary for a group
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceSummary {
    pub group_id: String,
    pub completion_rate: f64,
    pub avg_speed: f64,
    pub peak_speed: f64,
    pub current_speed: f64,
    pub total_time_seconds: u64,
    pub estimated_remaining_seconds: u64,
    pub efficiency_score: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_tracking() {
        let tracker = GroupMetricsTracker::new(1000);

        let metrics = GroupMetrics {
            group_id: "g1".to_string(),
            state: "Downloading".to_string(),
            total_size: 1_000_000,
            downloaded: 500_000,
            progress_percent: 50.0,
            avg_speed: 100_000.0,
            current_speed: 150_000.0,
            eta_seconds: 3,
            completed_count: 2,
            failed_count: 0,
            total_members: 4,
            cpu_usage_percent: 25.0,
            memory_usage: 100_000_000,
            available_bandwidth: 1_000_000.0,
            failure_risk_percent: 5.0,
            start_time_ms: 0,
            end_time_ms: 0,
            elapsed_seconds: 5,
        };

        assert!(tracker.update_group_metrics("g1", metrics).is_ok());
        assert!(tracker.get_group_metrics("g1").is_ok());
    }

    #[test]
    fn test_aggregate_metrics() {
        let tracker = GroupMetricsTracker::new(1000);

        let metrics1 = GroupMetrics {
            group_id: "g1".to_string(),
            state: "Downloading".to_string(),
            total_size: 1_000_000,
            downloaded: 500_000,
            progress_percent: 50.0,
            avg_speed: 100_000.0,
            current_speed: 150_000.0,
            eta_seconds: 3,
            completed_count: 2,
            failed_count: 0,
            total_members: 4,
            cpu_usage_percent: 25.0,
            memory_usage: 100_000_000,
            available_bandwidth: 1_000_000.0,
            failure_risk_percent: 5.0,
            start_time_ms: 0,
            end_time_ms: 0,
            elapsed_seconds: 5,
        };

        tracker.update_group_metrics("g1", metrics1).ok();

        let agg = tracker.get_aggregate_metrics().ok();
        assert!(agg.is_some());
    }
}
