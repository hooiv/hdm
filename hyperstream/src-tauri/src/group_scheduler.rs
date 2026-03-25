use crate::download_groups::{DownloadGroup, ExecutionStrategy, GroupState};
use std::collections::HashMap;
use std::sync::Mutex;

/// Tracks the execution state of a download group
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionState {
    Pending,
    Running,
    Paused,
    Completed,
    Failed,
}

/// Manages the scheduling and execution of download groups
/// with support for Sequential, Parallel, and Hybrid strategies
#[derive(Debug, Clone)]
pub struct GroupScheduler {
    /// Active groups indexed by group ID
    groups: HashMap<String, DownloadGroup>,
    /// Execution state per group
    execution_states: HashMap<String, ExecutionState>,
}

impl GroupScheduler {
    /// Create a new scheduler
    pub fn new() -> Self {
        Self {
            groups: HashMap::new(),
            execution_states: HashMap::new(),
        }
    }

    /// Add a group to the scheduler
    /// Returns error if a group with the same ID already exists
    pub fn schedule_group(&mut self, group: DownloadGroup) -> Result<(), String> {
        if self.groups.contains_key(&group.id) {
            return Err(format!("Group {} already scheduled", group.id));
        }

        let group_id = group.id.clone();
        self.groups.insert(group_id.clone(), group);
        self.execution_states.insert(group_id, ExecutionState::Pending);

        Ok(())
    }

    /// Retrieve a read-only reference to a group by ID
    pub fn get_group(&self, group_id: &str) -> Option<&DownloadGroup> {
        self.groups.get(group_id)
    }

    /// Retrieve a mutable reference to a group by ID
    pub fn get_group_mut(&mut self, group_id: &str) -> Option<&mut DownloadGroup> {
        self.groups.get_mut(group_id)
    }

    /// Helper: Check if a member's dependencies are all satisfied
    fn satisfies_dependencies(&self, group: &DownloadGroup, member_id: &str) -> bool {
        if let Some(member) = group.members.get(member_id) {
            member.dependencies.iter().all(|dep_id| {
                group
                    .members
                    .get(dep_id)
                    .map(|m| m.state == GroupState::Completed)
                    .unwrap_or(false)
            })
        } else {
            false
        }
    }

    /// Get the first member ready to download based on dependencies
    /// For Sequential: returns the first Pending member whose dependencies are satisfied
    /// For Parallel/Hybrid: returns the first Pending member (doesn't matter which one)
    pub fn get_next_member(&self, group_id: &str) -> Option<String> {
        self.get_group(group_id).and_then(|group| {
            group.members.iter().find_map(|(id, member)| {
                if member.state == GroupState::Pending && self.satisfies_dependencies(group, id) {
                    Some(id.clone())
                } else {
                    None
                }
            })
        })
    }

    /// Get all members that can run in parallel, respecting dependencies
    /// Returns members whose dependencies are satisfied and state is Pending
    pub fn get_ready_members(&self, group_id: &str) -> Vec<String> {
        self.get_group(group_id)
            .map(|group| {
                group
                    .members
                    .iter()
                    .filter_map(|(id, member)| {
                        if member.state == GroupState::Pending
                            && self.satisfies_dependencies(group, id)
                        {
                            Some(id.clone())
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Check if a member is ready to start (dependencies satisfied and state is Pending)
    pub fn can_start_member(&self, group_id: &str, member_id: &str) -> bool {
        self.get_group(group_id)
            .and_then(|group| {
                group.members.get(member_id).and_then(|member| {
                    if member.state == GroupState::Pending
                        && self.satisfies_dependencies(group, member_id)
                    {
                        Some(true)
                    } else {
                        None
                    }
                })
            })
            .unwrap_or(false)
    }

    /// Update a member's progress (0-100), auto-completes at 100%
    pub fn update_member_progress(&mut self, group_id: &str, member_id: &str, progress: f64) {
        if let Some(group) = self.get_group_mut(group_id) {
            if let Some(member) = group.members.get_mut(member_id) {
                member.progress_percent = progress.clamp(0.0, 100.0);
                if member.progress_percent >= 100.0 {
                    member.state = GroupState::Completed;
                }
            }
        }
    }

    /// Mark a member as completed and handle cascade effects
    /// Returns error if member doesn't exist
    pub fn complete_member(&mut self, group_id: &str, member_id: &str) -> Result<(), String> {
        if !self.groups.contains_key(group_id) {
            return Err(format!("Group {} not found", group_id));
        }

        if let Some(group) = self.get_group_mut(group_id) {
            if !group.members.contains_key(member_id) {
                return Err(format!("Member {} not found in group {}", member_id, group_id));
            }

            if let Some(member) = group.members.get_mut(member_id) {
                if member.state == GroupState::Completed {
                    return Err(format!(
                        "Member {} is already completed",
                        member_id
                    ));
                }
                member.state = GroupState::Completed;
                member.progress_percent = 100.0;
            }

            // Check if group is now complete
            if group.is_complete() {
                group.state = GroupState::Completed;
                self.execution_states
                    .insert(group_id.to_string(), ExecutionState::Completed);
            }
        }

        Ok(())
    }

    /// Mark a member as failed with a reason
    /// Returns error if member doesn't exist
    pub fn fail_member(
        &mut self,
        group_id: &str,
        member_id: &str,
        _reason: &str,
    ) -> Result<(), String> {
        if !self.groups.contains_key(group_id) {
            return Err(format!("Group {} not found", group_id));
        }

        if let Some(group) = self.get_group_mut(group_id) {
            if !group.members.contains_key(member_id) {
                return Err(format!("Member {} not found in group {}", member_id, group_id));
            }

            if let Some(member) = group.members.get_mut(member_id) {
                member.state = GroupState::Error;
                // Don't reset progress, keep it for debugging
            }

            // Mark group as failed
            group.state = GroupState::Error;
            self.execution_states
                .insert(group_id.to_string(), ExecutionState::Failed);
        }

        Ok(())
    }

    /// Get overall progress of a group (0-100)
    pub fn get_group_progress(&self, group_id: &str) -> Option<f64> {
        self.get_group(group_id)
            .map(|group| group.overall_progress())
    }

    /// Check if a group is complete (all members finished)
    pub fn is_group_complete(&self, group_id: &str) -> bool {
        self.get_group(group_id)
            .map(|group| group.is_complete())
            .unwrap_or(false)
    }

    /// Get list of completed member IDs in a group
    pub fn get_completed_members(&self, group_id: &str) -> Vec<String> {
        self.get_group(group_id)
            .map(|group| {
                group
                    .members
                    .iter()
                    .filter_map(|(id, member)| {
                        if member.state == GroupState::Completed {
                            Some(id.clone())
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get list of pending member IDs in a group
    pub fn get_pending_members(&self, group_id: &str) -> Vec<String> {
        self.get_group(group_id)
            .map(|group| {
                group
                    .members
                    .iter()
                    .filter_map(|(id, member)| {
                        if member.state == GroupState::Pending {
                            Some(id.clone())
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Start executing a group
    pub fn start_group(&mut self, group_id: &str) -> Result<(), String> {
        if !self.groups.contains_key(group_id) {
            return Err(format!("Group {} not found", group_id));
        }

        if let Some(group) = self.get_group_mut(group_id) {
            group.start_downloading()?;
        }

        self.execution_states
            .insert(group_id.to_string(), ExecutionState::Running);

        Ok(())
    }

    /// Pause a group
    pub fn pause_group(&mut self, group_id: &str) -> Result<(), String> {
        if !self.groups.contains_key(group_id) {
            return Err(format!("Group {} not found", group_id));
        }

        if let Some(group) = self.get_group_mut(group_id) {
            group.state = GroupState::Paused;
        }

        self.execution_states
            .insert(group_id.to_string(), ExecutionState::Paused);

        Ok(())
    }

    /// Get execution state of a group
    pub fn get_execution_state(&self, group_id: &str) -> Option<ExecutionState> {
        self.execution_states.get(group_id).cloned()
    }

    /// Get all groups currently in the scheduler
    pub fn get_all_groups(&self) -> Vec<&DownloadGroup> {
        self.groups.values().collect()
    }
}

impl Default for GroupScheduler {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Global static ──────────────────────────────────────────────────────────

lazy_static::lazy_static! {
    pub static ref GLOBAL_GROUP_SCHEDULER: Mutex<GroupScheduler> = Mutex::new(GroupScheduler::new());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_scheduler() -> GroupScheduler {
        GroupScheduler::new()
    }

    fn create_test_group(name: &str) -> DownloadGroup {
        DownloadGroup::new(name)
    }

    #[test]
    fn test_scheduler_creation() {
        let scheduler = create_test_scheduler();
        assert_eq!(scheduler.groups.len(), 0);
        assert_eq!(scheduler.execution_states.len(), 0);
    }

    #[test]
    fn test_schedule_group() {
        let mut scheduler = create_test_scheduler();
        let group = create_test_group("Test Group");
        let group_id = group.id.clone();

        let result = scheduler.schedule_group(group);
        assert!(result.is_ok());
        assert_eq!(scheduler.groups.len(), 1);
        assert!(scheduler.groups.contains_key(&group_id));
    }

    #[test]
    fn test_schedule_duplicate_group_fails() {
        let mut scheduler = create_test_scheduler();
        let group = create_test_group("Test Group");

        let _ = scheduler.schedule_group(group.clone());
        let result = scheduler.schedule_group(group);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already scheduled"));
    }

    #[test]
    fn test_get_group() {
        let mut scheduler = create_test_scheduler();
        let group = create_test_group("Test Group");
        let group_id = group.id.clone();

        scheduler.schedule_group(group).unwrap();
        let retrieved = scheduler.get_group(&group_id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "Test Group");
    }

    #[test]
    fn test_get_group_nonexistent() {
        let scheduler = create_test_scheduler();
        let retrieved = scheduler.get_group("nonexistent");
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_simple_dependency_resolution() {
        let mut scheduler = create_test_scheduler();
        let mut group = create_test_group("Test Group");

        let member1_id = group.add_member("https://example.com/file1.zip", None);
        let member2_id = group.add_member("https://example.com/file2.zip", Some(vec![member1_id.clone()]));

        scheduler.schedule_group(group).unwrap();
        let group_id = scheduler.groups.keys().next().unwrap().clone();
        scheduler.start_group(&group_id).unwrap();

        // Initially, only member1 should be ready (no dependencies)
        let ready = scheduler.get_ready_members(&group_id);
        assert_eq!(ready.len(), 1);
        assert!(ready.contains(&member1_id));

        // After completing member1, member2 should be ready
        scheduler.complete_member(&group_id, &member1_id).unwrap();
        let ready = scheduler.get_ready_members(&group_id);
        assert!(ready.contains(&member2_id));
    }

    #[test]
    fn test_can_start_member() {
        let mut scheduler = create_test_scheduler();
        let mut group = create_test_group("Test Group");

        let member1_id = group.add_member("https://example.com/file1.zip", None);
        let member2_id = group.add_member("https://example.com/file2.zip", Some(vec![member1_id.clone()]));

        scheduler.schedule_group(group).unwrap();
        let group_id = scheduler.groups.keys().next().unwrap().clone();
        scheduler.start_group(&group_id).unwrap();

        // Member1 can start (no dependencies)
        assert!(scheduler.can_start_member(&group_id, &member1_id));

        // Member2 cannot start (dependency not satisfied)
        assert!(!scheduler.can_start_member(&group_id, &member2_id));

        // Complete member1
        scheduler.complete_member(&group_id, &member1_id).unwrap();

        // Now member2 can start
        assert!(scheduler.can_start_member(&group_id, &member2_id));
    }

    #[test]
    fn test_get_next_member() {
        let mut scheduler = create_test_scheduler();
        let mut group = create_test_group("Test Group");

        let member1_id = group.add_member("https://example.com/file1.zip", None);
        let _member2_id = group.add_member("https://example.com/file2.zip", Some(vec![member1_id.clone()]));

        scheduler.schedule_group(group).unwrap();
        let group_id = scheduler.groups.keys().next().unwrap().clone();
        scheduler.start_group(&group_id).unwrap();

        let next = scheduler.get_next_member(&group_id);
        assert!(next.is_some());
        assert_eq!(next.unwrap(), member1_id);
    }

    #[test]
    fn test_update_member_progress() {
        let mut scheduler = create_test_scheduler();
        let mut group = create_test_group("Test Group");

        let member_id = group.add_member("https://example.com/file1.zip", None);
        scheduler.schedule_group(group).unwrap();
        let group_id = scheduler.groups.keys().next().unwrap().clone();
        scheduler.start_group(&group_id).unwrap();

        scheduler.update_member_progress(&group_id, &member_id, 50.0);
        let updated_group = scheduler.get_group(&group_id).unwrap();
        let member = &updated_group.members[&member_id];
        assert_eq!(member.progress_percent, 50.0);
        assert_ne!(member.state, GroupState::Completed);

        // Progress at 100% should auto-complete
        scheduler.update_member_progress(&group_id, &member_id, 100.0);
        let updated_group = scheduler.get_group(&group_id).unwrap();
        let member = &updated_group.members[&member_id];
        assert_eq!(member.state, GroupState::Completed);
    }

    #[test]
    fn test_complete_member() {
        let mut scheduler = create_test_scheduler();
        let mut group = create_test_group("Test Group");

        let member_id = group.add_member("https://example.com/file1.zip", None);
        scheduler.schedule_group(group).unwrap();
        let group_id = scheduler.groups.keys().next().unwrap().clone();

        let result = scheduler.complete_member(&group_id, &member_id);
        assert!(result.is_ok());

        let updated_group = scheduler.get_group(&group_id).unwrap();
        let member = &updated_group.members[&member_id];
        assert_eq!(member.state, GroupState::Completed);
        assert_eq!(member.progress_percent, 100.0);
    }

    #[test]
    fn test_complete_already_completed_member_fails() {
        let mut scheduler = create_test_scheduler();
        let mut group = create_test_group("Test Group");

        let member_id = group.add_member("https://example.com/file1.zip", None);
        scheduler.schedule_group(group).unwrap();
        let group_id = scheduler.groups.keys().next().unwrap().clone();

        scheduler.complete_member(&group_id, &member_id).unwrap();
        let result = scheduler.complete_member(&group_id, &member_id);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already completed"));
    }

    #[test]
    fn test_complete_nonexistent_member_fails() {
        let mut scheduler = create_test_scheduler();
        let mut group = create_test_group("Test Group");

        let _member_id = group.add_member("https://example.com/file1.zip", None);
        scheduler.schedule_group(group).unwrap();
        let group_id = scheduler.groups.keys().next().unwrap().clone();

        let result = scheduler.complete_member(&group_id, "nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_fail_member() {
        let mut scheduler = create_test_scheduler();
        let mut group = create_test_group("Test Group");

        let member_id = group.add_member("https://example.com/file1.zip", None);
        scheduler.schedule_group(group).unwrap();
        let group_id = scheduler.groups.keys().next().unwrap().clone();

        let result = scheduler.fail_member(&group_id, &member_id, "Network error");
        assert!(result.is_ok());

        let updated_group = scheduler.get_group(&group_id).unwrap();
        assert_eq!(updated_group.state, GroupState::Error);
        let member = &updated_group.members[&member_id];
        assert_eq!(member.state, GroupState::Error);
    }

    #[test]
    fn test_group_progress() {
        let mut scheduler = create_test_scheduler();
        let mut group = create_test_group("Test Group");

        let member1_id = group.add_member("https://example.com/file1.zip", None);
        let member2_id = group.add_member("https://example.com/file2.zip", None);

        scheduler.schedule_group(group).unwrap();
        let group_id = scheduler.groups.keys().next().unwrap().clone();

        scheduler.update_member_progress(&group_id, &member1_id, 50.0);
        scheduler.update_member_progress(&group_id, &member2_id, 100.0);

        let progress = scheduler.get_group_progress(&group_id).unwrap();
        assert_eq!(progress, 75.0); // Average of 50% and 100%
    }

    #[test]
    fn test_is_group_complete() {
        let mut scheduler = create_test_scheduler();
        let mut group = create_test_group("Test Group");

        let member1_id = group.add_member("https://example.com/file1.zip", None);
        let member2_id = group.add_member("https://example.com/file2.zip", None);

        scheduler.schedule_group(group).unwrap();
        let group_id = scheduler.groups.keys().next().unwrap().clone();

        assert!(!scheduler.is_group_complete(&group_id));

        scheduler.complete_member(&group_id, &member1_id).unwrap();
        assert!(!scheduler.is_group_complete(&group_id));

        scheduler.complete_member(&group_id, &member2_id).unwrap();
        assert!(scheduler.is_group_complete(&group_id));
    }

    #[test]
    fn test_get_completed_members() {
        let mut scheduler = create_test_scheduler();
        let mut group = create_test_group("Test Group");

        let member1_id = group.add_member("https://example.com/file1.zip", None);
        let member2_id = group.add_member("https://example.com/file2.zip", None);
        let member3_id = group.add_member("https://example.com/file3.zip", None);

        scheduler.schedule_group(group).unwrap();
        let group_id = scheduler.groups.keys().next().unwrap().clone();

        scheduler.complete_member(&group_id, &member1_id).unwrap();
        scheduler.complete_member(&group_id, &member2_id).unwrap();

        let completed = scheduler.get_completed_members(&group_id);
        assert_eq!(completed.len(), 2);
        assert!(completed.contains(&member1_id));
        assert!(completed.contains(&member2_id));
        assert!(!completed.contains(&member3_id));
    }

    #[test]
    fn test_get_pending_members() {
        let mut scheduler = create_test_scheduler();
        let mut group = create_test_group("Test Group");

        let member1_id = group.add_member("https://example.com/file1.zip", None);
        let member2_id = group.add_member("https://example.com/file2.zip", None);
        let member3_id = group.add_member("https://example.com/file3.zip", None);

        scheduler.schedule_group(group).unwrap();
        let group_id = scheduler.groups.keys().next().unwrap().clone();

        let pending = scheduler.get_pending_members(&group_id);
        assert_eq!(pending.len(), 3);

        scheduler.complete_member(&group_id, &member1_id).unwrap();

        let pending = scheduler.get_pending_members(&group_id);
        assert_eq!(pending.len(), 2);
        assert!(!pending.contains(&member1_id));
        assert!(pending.contains(&member2_id));
        assert!(pending.contains(&member3_id));
    }

    #[test]
    fn test_parallel_strategy_multiple_ready_members() {
        let mut scheduler = create_test_scheduler();
        let mut group = create_test_group("Test Group");
        group.strategy = ExecutionStrategy::Parallel;

        // Create 3 independent members (no dependencies)
        let member1_id = group.add_member("https://example.com/file1.zip", None);
        let member2_id = group.add_member("https://example.com/file2.zip", None);
        let member3_id = group.add_member("https://example.com/file3.zip", None);

        scheduler.schedule_group(group).unwrap();
        let group_id = scheduler.groups.keys().next().unwrap().clone();
        scheduler.start_group(&group_id).unwrap();

        // All should be ready simultaneously for parallel strategy
        let ready = scheduler.get_ready_members(&group_id);
        assert_eq!(ready.len(), 3);
        assert!(ready.contains(&member1_id));
        assert!(ready.contains(&member2_id));
        assert!(ready.contains(&member3_id));
    }

    #[test]
    fn test_sequential_strategy_with_cascade_completion() {
        let mut scheduler = create_test_scheduler();
        let mut group = create_test_group("Test Group");
        group.strategy = ExecutionStrategy::Sequential;

        let member1_id = group.add_member("https://example.com/file1.zip", None);
        let member2_id = group.add_member("https://example.com/file2.zip", Some(vec![member1_id.clone()]));
        let member3_id = group.add_member("https://example.com/file3.zip", Some(vec![member2_id.clone()]));

        scheduler.schedule_group(group).unwrap();
        let group_id = scheduler.groups.keys().next().unwrap().clone();
        scheduler.start_group(&group_id).unwrap();

        // Initially, only member1 is ready
        let ready = scheduler.get_ready_members(&group_id);
        assert_eq!(ready.len(), 1);
        assert!(ready[0] == member1_id);

        // Complete member1
        scheduler.complete_member(&group_id, &member1_id).unwrap();
        let ready = scheduler.get_ready_members(&group_id);
        assert_eq!(ready.len(), 1);
        assert!(ready[0] == member2_id);

        // Complete member2
        scheduler.complete_member(&group_id, &member2_id).unwrap();
        let ready = scheduler.get_ready_members(&group_id);
        assert_eq!(ready.len(), 1);
        assert!(ready[0] == member3_id);

        // Complete member3
        scheduler.complete_member(&group_id, &member3_id).unwrap();
        assert!(scheduler.is_group_complete(&group_id));
    }

    #[test]
    fn test_hybrid_strategy_respects_dependencies() {
        let mut scheduler = create_test_scheduler();
        let mut group = create_test_group("Test Group");
        group.strategy = ExecutionStrategy::Hybrid;

        // Create a graph: member1 -> {member2, member3}, {member2, member3} -> member4
        let member1_id = group.add_member("https://example.com/file1.zip", None);
        let member2_id = group.add_member("https://example.com/file2.zip", Some(vec![member1_id.clone()]));
        let member3_id = group.add_member("https://example.com/file3.zip", Some(vec![member1_id.clone()]));
        let member4_id = group.add_member(
            "https://example.com/file4.zip",
            Some(vec![member2_id.clone(), member3_id.clone()]),
        );

        scheduler.schedule_group(group).unwrap();
        let group_id = scheduler.groups.keys().next().unwrap().clone();
        scheduler.start_group(&group_id).unwrap();

        // Initially, only member1 is ready
        let ready = scheduler.get_ready_members(&group_id);
        assert_eq!(ready.len(), 1);
        assert!(ready[0] == member1_id);

        // Complete member1
        scheduler.complete_member(&group_id, &member1_id).unwrap();

        // Now member2 and member3 should both be ready (can run in parallel)
        let ready = scheduler.get_ready_members(&group_id);
        assert_eq!(ready.len(), 2);
        assert!(ready.contains(&member2_id));
        assert!(ready.contains(&member3_id));

        // Complete member2
        scheduler.complete_member(&group_id, &member2_id).unwrap();

        // Member4 still not ready (member3 not done)
        assert!(!scheduler.can_start_member(&group_id, &member4_id));

        // Complete member3
        scheduler.complete_member(&group_id, &member3_id).unwrap();

        // Now member4 is ready
        assert!(scheduler.can_start_member(&group_id, &member4_id));
    }

    #[test]
    fn test_multiple_groups_isolation() {
        let mut scheduler = create_test_scheduler();

        let mut group1 = create_test_group("Group 1");
        let member1_id = group1.add_member("https://example.com/file1.zip", None);

        let mut group2 = create_test_group("Group 2");
        let member2_id = group2.add_member("https://example.com/file2.zip", None);

        scheduler.schedule_group(group1).unwrap();
        scheduler.schedule_group(group2).unwrap();

        let group1_id = scheduler.groups.keys().next().unwrap().clone();
        let group2_id = scheduler.groups.keys().find(|id| id != &group1_id).unwrap().clone();

        scheduler.start_group(&group1_id).unwrap();
        scheduler.start_group(&group2_id).unwrap();

        // Complete member1 in group1
        scheduler.complete_member(&group1_id, &member1_id).unwrap();

        // Group1 should be complete
        assert!(scheduler.is_group_complete(&group1_id));

        // Group2 should not be affected
        assert!(!scheduler.is_group_complete(&group2_id));

        // Member2 should still be pending in group2
        let pending = scheduler.get_pending_members(&group2_id);
        assert_eq!(pending.len(), 1);
    }

    #[test]
    fn test_start_group_and_execution_state() {
        let mut scheduler = create_test_scheduler();
        let mut group = create_test_group("Test Group");

        let _member_id = group.add_member("https://example.com/file1.zip", None);
        scheduler.schedule_group(group).unwrap();
        let group_id = scheduler.groups.keys().next().unwrap().clone();

        assert_eq!(
            scheduler.get_execution_state(&group_id),
            Some(ExecutionState::Pending)
        );

        scheduler.start_group(&group_id).unwrap();
        assert_eq!(
            scheduler.get_execution_state(&group_id),
            Some(ExecutionState::Running)
        );
    }

    #[test]
    fn test_pause_group() {
        let mut scheduler = create_test_scheduler();
        let mut group = create_test_group("Test Group");

        let _member_id = group.add_member("https://example.com/file1.zip", None);
        scheduler.schedule_group(group).unwrap();
        let group_id = scheduler.groups.keys().next().unwrap().clone();

        scheduler.start_group(&group_id).unwrap();
        scheduler.pause_group(&group_id).unwrap();

        assert_eq!(
            scheduler.get_execution_state(&group_id),
            Some(ExecutionState::Paused)
        );
    }

    #[test]
    fn test_progress_clamping() {
        let mut scheduler = create_test_scheduler();
        let mut group = create_test_group("Test Group");

        let member_id = group.add_member("https://example.com/file1.zip", None);
        scheduler.schedule_group(group).unwrap();
        let group_id = scheduler.groups.keys().next().unwrap().clone();

        // Test negative clamping
        scheduler.update_member_progress(&group_id, &member_id, -50.0);
        let member = &scheduler.get_group(&group_id).unwrap().members[&member_id];
        assert_eq!(member.progress_percent, 0.0);

        // Test over 100 clamping
        scheduler.update_member_progress(&group_id, &member_id, 150.0);
        let member = &scheduler.get_group(&group_id).unwrap().members[&member_id];
        assert_eq!(member.progress_percent, 100.0);
    }
}
