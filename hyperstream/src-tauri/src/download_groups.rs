use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Represents the state of a download group
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GroupState {
    Pending,
    Downloading,
    Paused,
    Completed,
    Error,
}

/// Strategy for executing members of a download group
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionStrategy {
    Sequential,
    Parallel,
    Hybrid,
}

impl Default for ExecutionStrategy {
    fn default() -> Self {
        ExecutionStrategy::Hybrid
    }
}

/// Represents a single member of a download group
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupMember {
    /// Unique identifier for this member
    pub id: String,
    /// Download URL
    pub url: String,
    /// Current progress as percentage (0-100)
    pub progress_percent: f64,
    /// Current state of the download
    pub state: GroupState,
    /// IDs of members that must complete before this one can start
    pub dependencies: Vec<String>,
}

impl GroupMember {
    /// Create a new group member
    fn new(url: String, dependencies: Vec<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            url,
            progress_percent: 0.0,
            state: GroupState::Pending,
            dependencies,
        }
    }

    /// Check if this member can start (all dependencies completed)
    fn can_start(&self, members: &HashMap<String, GroupMember>) -> bool {
        if self.state != GroupState::Pending {
            return false;
        }

        self.dependencies.iter().all(|dep_id| {
            members
                .get(dep_id)
                .map(|m| m.state == GroupState::Completed)
                .unwrap_or(false)
        })
    }

    /// Check if this member is complete
    fn is_complete(&self) -> bool {
        self.state == GroupState::Completed && self.progress_percent >= 100.0
    }
}

/// Represents a group of related downloads that should be managed together
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadGroup {
    /// Unique identifier for this group
    pub id: String,
    /// User-friendly name for this group
    pub name: String,
    /// Current state of the group
    pub state: GroupState,
    /// Members of this group
    pub members: HashMap<String, GroupMember>,
    /// Strategy for executing members
    pub strategy: ExecutionStrategy,
    /// Timestamp when group was created (milliseconds since epoch)
    pub created_at_ms: u64,
    /// Timestamp when group was completed (milliseconds since epoch), or 0 if not completed
    pub completed_at_ms: u64,
}

impl DownloadGroup {
    /// Create a new download group with the given name
    pub fn new(name: &str) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        Self {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            state: GroupState::Pending,
            members: HashMap::new(),
            strategy: ExecutionStrategy::Hybrid,
            created_at_ms: now,
            completed_at_ms: 0,
        }
    }

    /// Add a member to the group with optional dependencies
    /// Returns the ID of the newly added member
    pub fn add_member(&mut self, url: &str, dependencies: Option<Vec<String>>) -> String {
        let deps = dependencies.unwrap_or_default();
        let member = GroupMember::new(url.to_string(), deps);
        let member_id = member.id.clone();
        self.members.insert(member_id.clone(), member);
        member_id
    }

    /// Add a dependency between two members
    /// The dependent_id member will wait for prerequisite_id to complete before starting
    pub fn add_dependency(&mut self, dependent_id: String, prerequisite_id: String) {
        if let Some(member) = self.members.get_mut(&dependent_id) {
            if !member.dependencies.contains(&prerequisite_id) {
                member.dependencies.push(prerequisite_id);
            }
        }
    }

    /// Resolve the execution order using topological sort (DFS-based)
    /// Returns a Vec of member IDs in the order they should execute
    /// Returns empty vec if there are circular dependencies
    pub fn resolve_execution_order(&self) -> Vec<String> {
        if self.members.is_empty() {
            return vec![];
        }

        let mut visited = HashMap::new();
        let mut rec_stack = HashMap::new();
        let mut order = Vec::new();

        // Try DFS from each node
        for member_id in self.members.keys() {
            if !visited.contains_key(member_id) {
                if self.has_cycle() {
                    return vec![]; // Circular dependency detected
                }
                self.dfs(member_id, &mut visited, &mut rec_stack, &mut order);
            }
        }

        // Reverse to get topological order (dependencies first)
        order.reverse();
        order
    }

    /// DFS helper for topological sort
    fn dfs(
        &self,
        node: &str,
        visited: &mut HashMap<String, bool>,
        rec_stack: &mut HashMap<String, bool>,
        order: &mut Vec<String>,
    ) {
        visited.insert(node.to_string(), true);
        rec_stack.insert(node.to_string(), true);

        if let Some(member) = self.members.get(node) {
            for dep in &member.dependencies {
                if !visited.contains_key(dep) {
                    self.dfs(dep, visited, rec_stack, order);
                } else if rec_stack.get(dep).copied().unwrap_or(false) {
                    // Cycle detected, but we'll handle it in has_cycle()
                }
            }
        }

        rec_stack.insert(node.to_string(), false);
        order.push(node.to_string());
    }

    /// Check if there are circular dependencies
    fn has_cycle(&self) -> bool {
        let mut visited = HashMap::new();
        let mut rec_stack = HashMap::new();

        for member_id in self.members.keys() {
            if !visited.contains_key(member_id) {
                if self.visit_cycle_check(member_id, &mut visited, &mut rec_stack) {
                    return true;
                }
            }
        }
        false
    }

    /// Helper for cycle detection
    fn visit_cycle_check(
        &self,
        node: &str,
        visited: &mut HashMap<String, bool>,
        rec_stack: &mut HashMap<String, bool>,
    ) -> bool {
        visited.insert(node.to_string(), true);
        rec_stack.insert(node.to_string(), true);

        if let Some(member) = self.members.get(node) {
            for dep in &member.dependencies {
                if !visited.contains_key(dep) {
                    if self.visit_cycle_check(dep, visited, rec_stack) {
                        return true;
                    }
                } else if rec_stack.get(dep).copied().unwrap_or(false) {
                    return true;
                }
            }
        }

        rec_stack.insert(node.to_string(), false);
        false
    }

    /// Start downloading all members in the group
    pub fn start_downloading(&mut self) -> Result<(), String> {
        if self.members.is_empty() {
            return Err("Cannot start downloading: group has no members".to_string());
        }

        if self.has_cycle() {
            return Err("Cannot start downloading: circular dependencies detected".to_string());
        }

        self.state = GroupState::Downloading;

        // Update all members to Downloading state if they can start or have no dependencies
        for member in self.members.values_mut() {
            if member.dependencies.is_empty() {
                member.state = GroupState::Downloading;
            }
        }

        Ok(())
    }

    /// Calculate overall progress as percentage (0-100)
    pub fn overall_progress(&self) -> f64 {
        if self.members.is_empty() {
            return 0.0;
        }

        let total: f64 = self.members.values().map(|m| m.progress_percent).sum();
        total / self.members.len() as f64
    }

    /// Count the number of completed members
    pub fn completed_count(&self) -> usize {
        self.members
            .values()
            .filter(|m| m.state == GroupState::Completed && m.progress_percent >= 100.0)
            .count()
    }

    /// Check if the entire group is complete
    pub fn is_complete(&self) -> bool {
        if self.members.is_empty() {
            return false;
        }

        self.members
            .values()
            .all(|m| m.state == GroupState::Completed && m.progress_percent >= 100.0)
    }

    /// Update a member's progress (0-100)
    pub fn update_member_progress(&mut self, member_id: &str, progress: f64) {
        if let Some(member) = self.members.get_mut(member_id) {
            member.progress_percent = progress.clamp(0.0, 100.0);
            if member.progress_percent >= 100.0 {
                member.state = GroupState::Completed;
            }
        }
    }

    /// Update a member's state
    pub fn update_member_state(&mut self, member_id: &str, state: GroupState) {
        if let Some(member) = self.members.get_mut(member_id) {
            member.state = state;
        }
    }

    /// Get the total number of members
    pub fn member_count(&self) -> usize {
        self.members.len()
    }

    /// Check if a member can start (dependencies satisfied)
    pub fn can_start_member(&self, member_id: &str) -> bool {
        if let Some(member) = self.members.get(member_id) {
            member.can_start(&self.members)
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_empty_group() {
        let group = DownloadGroup::new("Test Group");
        assert_eq!(group.name, "Test Group");
        assert_eq!(group.state, GroupState::Pending);
        assert_eq!(group.members.len(), 0);
        assert_eq!(group.strategy, ExecutionStrategy::Hybrid);
        assert_eq!(group.completed_at_ms, 0);
        assert!(group.created_at_ms > 0);
    }

    #[test]
    fn test_add_single_member() {
        let mut group = DownloadGroup::new("Test Group");
        let member_id = group.add_member("https://example.com/file.zip", None);

        assert_eq!(group.members.len(), 1);
        assert!(group.members.contains_key(&member_id));

        let member = &group.members[&member_id];
        assert_eq!(member.url, "https://example.com/file.zip");
        assert_eq!(member.progress_percent, 0.0);
        assert_eq!(member.state, GroupState::Pending);
        assert_eq!(member.dependencies.len(), 0);
    }

    #[test]
    fn test_add_multiple_members() {
        let mut group = DownloadGroup::new("Test Group");
        let id1 = group.add_member("https://example.com/file1.zip", None);
        let id2 = group.add_member("https://example.com/file2.zip", None);
        let id3 = group.add_member("https://example.com/file3.zip", None);

        assert_eq!(group.members.len(), 3);
        assert!(group.members.contains_key(&id1));
        assert!(group.members.contains_key(&id2));
        assert!(group.members.contains_key(&id3));
    }

    #[test]
    fn test_add_member_with_dependencies() {
        let mut group = DownloadGroup::new("Test Group");
        let id1 = group.add_member("https://example.com/file1.zip", None);
        let id2 = group.add_member("https://example.com/file2.zip", Some(vec![id1.clone()]));

        assert_eq!(group.members[&id2].dependencies.len(), 1);
        assert_eq!(group.members[&id2].dependencies[0], id1);
    }

    #[test]
    fn test_add_dependency() {
        let mut group = DownloadGroup::new("Test Group");
        let id1 = group.add_member("https://example.com/file1.zip", None);
        let id2 = group.add_member("https://example.com/file2.zip", None);

        group.add_dependency(id2.clone(), id1.clone());

        assert_eq!(group.members[&id2].dependencies.len(), 1);
        assert_eq!(group.members[&id2].dependencies[0], id1);
    }

    #[test]
    fn test_resolve_execution_order_no_dependencies() {
        let mut group = DownloadGroup::new("Test Group");
        let id1 = group.add_member("https://example.com/file1.zip", None);
        let id2 = group.add_member("https://example.com/file2.zip", None);
        let id3 = group.add_member("https://example.com/file3.zip", None);

        let order = group.resolve_execution_order();

        assert_eq!(order.len(), 3);
        assert!(order.contains(&id1));
        assert!(order.contains(&id2));
        assert!(order.contains(&id3));
    }

    #[test]
    fn test_resolve_execution_order_with_dependencies() {
        let mut group = DownloadGroup::new("Test Group");
        let id1 = group.add_member("https://example.com/file1.zip", None);
        let id2 = group.add_member("https://example.com/file2.zip", Some(vec![id1.clone()]));
        let id3 =
            group.add_member("https://example.com/file3.zip", Some(vec![id2.clone()]));

        let order = group.resolve_execution_order();

        assert_eq!(order.len(), 3);
        // id1 should come before id2, id2 should come before id3
        let pos_id1 = order.iter().position(|x| x == &id1).unwrap();
        let pos_id2 = order.iter().position(|x| x == &id2).unwrap();
        let pos_id3 = order.iter().position(|x| x == &id3).unwrap();

        assert!(pos_id1 < pos_id2);
        assert!(pos_id2 < pos_id3);
    }

    #[test]
    fn test_circular_dependency_detection() {
        let mut group = DownloadGroup::new("Test Group");
        let id1 = group.add_member("https://example.com/file1.zip", None);
        let id2 = group.add_member("https://example.com/file2.zip", Some(vec![id1.clone()]));
        
        // Create circular dependency: id1 depends on id2, id2 depends on id1
        group.add_dependency(id1.clone(), id2.clone());

        assert!(group.has_cycle());
        let order = group.resolve_execution_order();
        assert_eq!(order.len(), 0); // Should return empty due to cycle
    }

    #[test]
    fn test_start_downloading_success() {
        let mut group = DownloadGroup::new("Test Group");
        group.add_member("https://example.com/file1.zip", None);

        let result = group.start_downloading();
        assert!(result.is_ok());
        assert_eq!(group.state, GroupState::Downloading);
    }

    #[test]
    fn test_start_downloading_empty_group() {
        let mut group = DownloadGroup::new("Test Group");

        let result = group.start_downloading();
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Cannot start downloading: group has no members"
        );
    }

    #[test]
    fn test_overall_progress_empty() {
        let group = DownloadGroup::new("Test Group");
        assert_eq!(group.overall_progress(), 0.0);
    }

    #[test]
    fn test_overall_progress_with_members() {
        let mut group = DownloadGroup::new("Test Group");
        let id1 = group.add_member("https://example.com/file1.zip", None);
        let id2 = group.add_member("https://example.com/file2.zip", None);
        let id3 = group.add_member("https://example.com/file3.zip", None);

        group.update_member_progress(&id1, 50.0);
        group.update_member_progress(&id2, 75.0);
        group.update_member_progress(&id3, 100.0);

        let progress = group.overall_progress();
        assert!((progress - 75.0).abs() < 0.01); // (50 + 75 + 100) / 3 = 75
    }

    #[test]
    fn test_completed_count() {
        let mut group = DownloadGroup::new("Test Group");
        let id1 = group.add_member("https://example.com/file1.zip", None);
        let id2 = group.add_member("https://example.com/file2.zip", None);
        let id3 = group.add_member("https://example.com/file3.zip", None);

        group.update_member_progress(&id1, 100.0);
        group.update_member_state(&id1, GroupState::Completed);

        group.update_member_progress(&id2, 50.0);

        group.update_member_progress(&id3, 100.0);
        group.update_member_state(&id3, GroupState::Completed);

        assert_eq!(group.completed_count(), 2);
    }

    #[test]
    fn test_is_complete() {
        let mut group = DownloadGroup::new("Test Group");
        let id1 = group.add_member("https://example.com/file1.zip", None);
        let id2 = group.add_member("https://example.com/file2.zip", None);

        assert!(!group.is_complete());

        group.update_member_progress(&id1, 100.0);
        group.update_member_state(&id1, GroupState::Completed);
        assert!(!group.is_complete());

        group.update_member_progress(&id2, 100.0);
        group.update_member_state(&id2, GroupState::Completed);
        assert!(group.is_complete());
    }

    #[test]
    fn test_update_member_progress_clamping() {
        let mut group = DownloadGroup::new("Test Group");
        let id1 = group.add_member("https://example.com/file1.zip", None);

        group.update_member_progress(&id1, 150.0);
        assert_eq!(group.members[&id1].progress_percent, 100.0);

        group.update_member_progress(&id1, -10.0);
        assert_eq!(group.members[&id1].progress_percent, 0.0);
    }

    #[test]
    fn test_can_start_member_no_dependencies() {
        let mut group = DownloadGroup::new("Test Group");
        let id1 = group.add_member("https://example.com/file1.zip", None);

        assert!(group.can_start_member(&id1));
    }

    #[test]
    fn test_can_start_member_with_unsatisfied_dependency() {
        let mut group = DownloadGroup::new("Test Group");
        let id1 = group.add_member("https://example.com/file1.zip", None);
        let id2 = group.add_member("https://example.com/file2.zip", Some(vec![id1.clone()]));

        assert!(!group.can_start_member(&id2));
        assert!(group.can_start_member(&id1));
    }

    #[test]
    fn test_can_start_member_with_satisfied_dependency() {
        let mut group = DownloadGroup::new("Test Group");
        let id1 = group.add_member("https://example.com/file1.zip", None);
        let id2 = group.add_member("https://example.com/file2.zip", Some(vec![id1.clone()]));

        // Complete the dependency
        group.update_member_state(&id1, GroupState::Completed);
        group.update_member_progress(&id1, 100.0);

        assert!(group.can_start_member(&id2));
    }

    #[test]
    fn test_member_count() {
        let mut group = DownloadGroup::new("Test Group");
        assert_eq!(group.member_count(), 0);

        group.add_member("https://example.com/file1.zip", None);
        assert_eq!(group.member_count(), 1);

        group.add_member("https://example.com/file2.zip", None);
        assert_eq!(group.member_count(), 2);
    }

    #[test]
    fn test_serialization() {
        let mut group = DownloadGroup::new("Test Group");
        group.add_member("https://example.com/file1.zip", None);
        group.strategy = ExecutionStrategy::Sequential;

        let json = serde_json::to_string(&group).expect("Serialization failed");
        let deserialized: DownloadGroup =
            serde_json::from_str(&json).expect("Deserialization failed");

        assert_eq!(deserialized.name, group.name);
        assert_eq!(deserialized.strategy, ExecutionStrategy::Sequential);
        assert_eq!(deserialized.member_count(), 1);
    }
}
