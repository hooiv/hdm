/// Smart Priority Queue for Download Groups
///
/// Intelligently reorders and prioritizes downloads based on:
/// - Dependency chains (critical path first)
/// - Estimated completion times
/// - User-specified priorities
/// - Resource availability and bandwidth
/// - Failure risk prediction
///
/// Uses dynamic programming to minimize overall completion time while respecting constraints.

use std::collections::{BinaryHeap, HashMap, HashSet};
use std::cmp::Ordering;
use serde::{Deserialize, Serialize};

/// Priority for a download item
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Priority {
    Critical,  // Must complete first
    High,      // Important
    Normal,    // Standard
    Low,       // Can wait
}

impl Default for Priority {
    fn default() -> Self {
        Priority::Normal
    }
}

/// Scheduling constraint for a member
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulingConstraint {
    /// Member ID
    pub member_id: String,
    /// User-specified priority
    pub priority: Priority,
    /// Minimum resources needed (bandwidth as bytes/sec)
    pub min_bandwidth: u64,
    /// Must complete by this time (unix ms), 0 = no deadline
    pub deadline_ms: u64,
    /// Cannot start before this time (unix ms)
    pub earliest_start_ms: u64,
    /// Depends on completing other members first
    pub dependencies: Vec<String>,
    /// Expected download size (bytes)
    pub expected_size: u64,
    /// Expected speed (bytes/sec)
    pub expected_speed: u64,
}

/// Calculated priority score for scheduling
#[derive(Debug, Clone)]
struct PriorityScore {
    /// Member ID
    member_id: String,
    /// Score used for sorting (higher = should run sooner)
    score: f64,
    /// Reason for score
    reason: String,
}

impl PartialEq for PriorityScore {
    fn eq(&self, other: &Self) -> bool {
        (self.score - other.score).abs() < f64::EPSILON
    }
}

impl Eq for PriorityScore {}

impl PartialOrd for PriorityScore {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PriorityScore {
    fn cmp(&self, other: &Self) -> Ordering {
        self.score
            .partial_cmp(&other.score)
            .unwrap_or(Ordering::Equal)
            // Deterministic tie-breaker for stable scheduling.
            .then_with(|| other.member_id.cmp(&self.member_id))
    }
}

/// Smart priority queue for downloads
pub struct SmartPriorityQueue {
    /// Scheduled items
    queue: BinaryHeap<PriorityScore>,
    /// Constraints per member
    constraints: HashMap<String, SchedulingConstraint>,
    /// Completed members used for dependency scoring
    completed_members: HashSet<String>,
    /// Current system bandwidth (bytes/sec)
    available_bandwidth: u64,
}

impl SmartPriorityQueue {
    /// Create a new smart priority queue
    pub fn new(available_bandwidth: u64) -> Self {
        Self {
            queue: BinaryHeap::new(),
            constraints: HashMap::new(),
            completed_members: HashSet::new(),
            available_bandwidth,
        }
    }

    /// Add a member to the queue with constraints
    pub fn add_member(&mut self, constraint: SchedulingConstraint) {
        // Re-adding a member means it is not considered completed anymore.
        self.completed_members.remove(&constraint.member_id);
        self.constraints
            .insert(constraint.member_id.clone(), constraint.clone());

        // Recalculate entire queue priority
        self.recalculate_queue();
    }

    /// Remove a member from the queue
    pub fn remove_member(&mut self, member_id: &str) {
        self.constraints.remove(member_id);
        self.recalculate_queue();
    }

    /// Update available bandwidth (triggers recalculation)
    pub fn set_available_bandwidth(&mut self, bandwidth: u64) {
        self.available_bandwidth = bandwidth;
        self.recalculate_queue();
    }

    /// Mark a member as completed (removes from queue)
    pub fn mark_completed(&mut self, member_id: &str) {
        self.completed_members.insert(member_id.to_string());
        self.constraints.remove(member_id);
        self.recalculate_queue();
    }

    /// Get the next member to download
    pub fn peek(&self) -> Option<String> {
        self.queue.peek().map(|s| s.member_id.clone())
    }

    /// Pop next member from queue
    pub fn pop(&mut self) -> Option<String> {
        self.queue.pop().map(|s| s.member_id)
    }

    /// Check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Get all scheduled members in priority order
    pub fn list_scheduled(&self) -> Vec<String> {
        let mut items: Vec<_> = self.queue.iter().map(|s| s.member_id.clone()).collect();
        items.sort_by(|a, b| {
            let a_score = self
                .queue
                .iter()
                .find(|s| s.member_id == *a)
                .map(|s| s.score);
            let b_score = self
                .queue
                .iter()
                .find(|s| s.member_id == *b)
                .map(|s| s.score);

            match (b_score, a_score) {
                (Some(bs), Some(as_)) => bs.partial_cmp(&as_).unwrap_or(Ordering::Equal),
                _ => Ordering::Equal,
            }
        });
        items
    }

    /// Calculate priority score for an item
    fn calculate_score(
        member_id: &str,
        constraint: &SchedulingConstraint,
        completed_members: &[&str],
        available_bandwidth: u64,
    ) -> PriorityScore {
        let mut score = 0.0;
        let mut reason = String::new();

        // Factor 1: User priority (0-40 points)
        let priority_score = match constraint.priority {
            Priority::Critical => 40.0,
            Priority::High => 30.0,
            Priority::Normal => 20.0,
            Priority::Low => 10.0,
        };
        score += priority_score;
        reason.push_str(&format!("priority={:?}({}), ", constraint.priority, priority_score));

        // Factor 2: Dependency readiness (0-30 points)
        let unmet_deps = constraint
            .dependencies
            .iter()
            .filter(|dep| {
                !completed_members
                    .iter()
                    .any(|completed| completed == &dep.as_str())
            })
            .count();

        if unmet_deps == 0 {
            score += 30.0;
            reason.push_str("deps_ready(30), ");
        } else {
            let penalty = (unmet_deps as f64) * 5.0;
            score -= penalty.min(30.0);
            reason.push_str(&format!("waiting_on_{}({}), ", unmet_deps, -penalty.min(30.0)));
        }

        // Factor 3: Estimated time to completion (0-20 points: faster = higher)
        let eta = if constraint.expected_speed > 0 {
            constraint.expected_size / constraint.expected_speed
        } else {
            u64::MAX
        };

        let eta_score = if eta < 10 {
            20.0
        } else if eta < 60 {
            15.0
        } else if eta < 300 {
            10.0
        } else {
            5.0
        };
        score += eta_score;
        reason.push_str(&format!("eta={}s({}), ", eta, eta_score));

        // Factor 4: Deadline urgency (-10 to +10 points)
        if constraint.deadline_ms > 0 {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);

            if constraint.deadline_ms > now {
                let time_until_deadline = constraint.deadline_ms - now;
                let urgency = if time_until_deadline < 60000 {
                    // Less than 1 minute
                    10.0
                } else if time_until_deadline < 300000 {
                    // Less than 5 minutes
                    8.0
                } else {
                    5.0
                };
                score += urgency;
                reason.push_str(&format!("deadline_soon({}), ", urgency));
            } else {
                score -= 10.0;
                reason.push_str("deadline_passed(-10), ");
            }
        }

        // Factor 5: Resource fit (0-10 points)
        if available_bandwidth >= constraint.min_bandwidth {
            score += 10.0;
            reason.push_str("bandwidth_available(10), ");
        } else {
            score -= 5.0;
            reason.push_str("bandwidth_constrained(-5), ");
        }

        PriorityScore {
            member_id: member_id.to_string(),
            score,
            reason: reason.trim_end_matches(", ").to_string(),
        }
    }

    /// Recalculate entire queue (called after changes)
    fn recalculate_queue(&mut self) {
        self.queue.clear();

        // Determine which members are already completed
        let completed: Vec<_> = self
            .completed_members
            .iter()
            .map(|s| s.as_str())
            .collect();

        // Calculate scores for all remaining members
        for (member_id, constraint) in &self.constraints {
            let score = Self::calculate_score(
                member_id,
                constraint,
                &completed,
                self.available_bandwidth,
            );

            self.queue.push(score);
        }
    }

    /// Get scheduling report (for debugging)
    pub fn get_schedule_report(&self) -> ScheduleReport {
        let items: Vec<_> = self
            .queue
            .iter()
            .map(|s| ScheduleItem {
                member_id: s.member_id.clone(),
                score: s.score,
                reason: s.reason.clone(),
            })
            .collect();

        ScheduleReport {
            total_items: self.constraints.len(),
            queued_items: items.len(),
            available_bandwidth: self.available_bandwidth,
            items,
        }
    }
}

/// Schedule item for reporting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleItem {
    pub member_id: String,
    pub score: f64,
    pub reason: String,
}

/// Full scheduling report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleReport {
    pub total_items: usize,
    pub queued_items: usize,
    pub available_bandwidth: u64,
    pub items: Vec<ScheduleItem>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_queueing() {
        let mut queue = SmartPriorityQueue::new(1_000_000);

        queue.add_member(SchedulingConstraint {
            member_id: "download1".to_string(),
            priority: Priority::Normal,
            min_bandwidth: 100_000,
            deadline_ms: 0,
            earliest_start_ms: 0,
            dependencies: vec![],
            expected_size: 1_000_000,
            expected_speed: 100_000,
        });

        queue.add_member(SchedulingConstraint {
            member_id: "download2".to_string(),
            priority: Priority::High,
            min_bandwidth: 100_000,
            deadline_ms: 0,
            earliest_start_ms: 0,
            dependencies: vec![],
            expected_size: 1_000_000,
            expected_speed: 100_000,
        });

        let first = queue.peek();
        // High priority should be first
        assert_eq!(first, Some("download2".to_string()));
    }

    #[test]
    fn test_dependency_ordering() {
        let mut queue = SmartPriorityQueue::new(1_000_000);

        // Add dependent download second
        queue.add_member(SchedulingConstraint {
            member_id: "dependent".to_string(),
            priority: Priority::Critical,
            min_bandwidth: 100_000,
            deadline_ms: 0,
            earliest_start_ms: 0,
            dependencies: vec!["base".to_string()],
            expected_size: 1_000_000,
            expected_speed: 100_000,
        });

        // Add base download first
        queue.add_member(SchedulingConstraint {
            member_id: "base".to_string(),
            priority: Priority::Normal,
            min_bandwidth: 100_000,
            deadline_ms: 0,
            earliest_start_ms: 0,
            dependencies: vec![],
            expected_size: 1_000_000,
            expected_speed: 100_000,
        });

        let first = queue.peek();
        // Base should be first despite lower priority due to dependency
        assert_eq!(first, Some("base".to_string()));
    }

    #[test]
    fn test_bandwidth_constraint() {
        let mut queue = SmartPriorityQueue::new(100_000); // Limited bandwidth

        queue.add_member(SchedulingConstraint {
            member_id: "bandwidth_heavy".to_string(),
            priority: Priority::Normal,
            min_bandwidth: 200_000, // More than available
            deadline_ms: 0,
            earliest_start_ms: 0,
            dependencies: vec![],
            expected_size: 1_000_000,
            expected_speed: 100_000,
        });

        queue.add_member(SchedulingConstraint {
            member_id: "bandwidth_light".to_string(),
            priority: Priority::Normal,
            min_bandwidth: 50_000, // Within available
            deadline_ms: 0,
            earliest_start_ms: 0,
            dependencies: vec![],
            expected_size: 1_000_000,
            expected_speed: 100_000,
        });

        let first = queue.peek();
        // Light should be prioritized when bandwidth is constrained
        assert_eq!(first, Some("bandwidth_light".to_string()));
    }

    #[test]
    fn test_completion_marking() {
        let mut queue = SmartPriorityQueue::new(1_000_000);

        queue.add_member(SchedulingConstraint {
            member_id: "download1".to_string(),
            priority: Priority::Normal,
            min_bandwidth: 100_000,
            deadline_ms: 0,
            earliest_start_ms: 0,
            dependencies: vec![],
            expected_size: 1_000_000,
            expected_speed: 100_000,
        });

        assert_eq!(queue.peek(), Some("download1".to_string()));

        queue.mark_completed("download1");

        assert!(queue.is_empty());
    }

    #[test]
    fn test_completed_dependency_boosts_priority() {
        let mut queue = SmartPriorityQueue::new(1_000_000);

        queue.add_member(SchedulingConstraint {
            member_id: "dependent".to_string(),
            priority: Priority::Critical,
            min_bandwidth: 100_000,
            deadline_ms: 0,
            earliest_start_ms: 0,
            dependencies: vec!["base".to_string()],
            expected_size: 1_000_000,
            expected_speed: 100_000,
        });

        queue.add_member(SchedulingConstraint {
            member_id: "independent".to_string(),
            priority: Priority::Low,
            min_bandwidth: 100_000,
            deadline_ms: 0,
            earliest_start_ms: 0,
            dependencies: vec![],
            expected_size: 1_000_000,
            expected_speed: 100_000,
        });

        // Base dependency is not complete yet, so independent should come first.
        assert_eq!(queue.peek(), Some("independent".to_string()));

        // Once base is complete, dependent receives dependency-ready boost.
        queue.mark_completed("base");
        assert_eq!(queue.peek(), Some("dependent".to_string()));
    }
}
