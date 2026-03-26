//! Production-grade error handling and validation for Download Groups
//!
//! Provides:
//! - Circular dependency detection with detailed cycle information
//! - Member progress validation
//! - Group state consistency checks
//! - Detailed error reporting with recovery suggestions
//! - Graceful error propagation to UI

#[cfg(test)]
use crate::download_groups::GroupMember;
use crate::download_groups::{DownloadGroup, GroupState};
use crate::group_dag_solver::DagSolver;
use serde::{Deserialize, Serialize};

/// Detailed error information for group operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GroupError {
    /// Circular dependency detected in the group
    CircularDependency {
        cycle_members: Vec<String>,
        first_member: String,
        description: String,
    },
    /// Member depends on a non-existent member
    InvalidDependency {
        dependent_member: String,
        missing_dependency: String,
    },
    /// Member cannot complete because it depends on a failed member
    DependencyFailed {
        member_id: String,
        failed_dependency: String,
        cascade_reason: String,
    },
    /// Group state is inconsistent
    InconsistentState {
        group_id: String,
        issue: String,
        repair_suggestion: String,
    },
    /// Member progress is invalid
    InvalidProgress {
        member_id: String,
        progress: f64,
        reason: String,
    },
    /// Group execution violates its strategy constraints
    StrategyViolation {
        group_id: String,
        strategy: String,
        violation: String,
    },
    /// Member is stuck (deadlocked)
    Deadlock {
        member_id: String,
        stuck_reason: String,
    },
}

impl std::fmt::Display for GroupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GroupError::CircularDependency {
                cycle_members,
                first_member,
                description,
            } => write!(
                f,
                "Circular dependency: {} → {} ({})",
                first_member,
                cycle_members.join(" → "),
                description
            ),
            GroupError::InvalidDependency {
                dependent_member,
                missing_dependency,
            } => write!(
                f,
                "Member '{}' depends on non-existent member '{}'",
                dependent_member, missing_dependency
            ),
            GroupError::DependencyFailed {
                member_id,
                failed_dependency,
                cascade_reason,
            } => write!(
                f,
                "Member '{}' blocked: dependency '{}' failed ({})",
                member_id, failed_dependency, cascade_reason
            ),
            GroupError::InconsistentState {
                group_id,
                issue,
                repair_suggestion,
            } => write!(
                f,
                "Group '{}' state inconsistency: {} [fix: {}]",
                group_id, issue, repair_suggestion
            ),
            GroupError::InvalidProgress {
                member_id,
                progress,
                reason,
            } => write!(
                f,
                "Member '{}' has invalid progress {:.1}% ({})",
                member_id, progress, reason
            ),
            GroupError::StrategyViolation {
                group_id,
                strategy,
                violation,
            } => write!(
                f,
                "Group '{}' ({} mode) violation: {}",
                group_id, strategy, violation
            ),
            GroupError::Deadlock {
                member_id,
                stuck_reason,
            } => write!(
                f,
                "Member '{}' appears deadlocked: {}",
                member_id, stuck_reason
            ),
        }
    }
}

/// Result type for group operations
pub type GroupResult<T> = Result<T, GroupError>;

/// Comprehensive group validator
pub struct GroupValidator;

impl GroupValidator {
    /// Validate an entire group for consistency and correctness
    pub fn validate_group(group: &DownloadGroup) -> GroupResult<()> {
        // Check 1: No circular dependencies
        Self::check_circular_dependencies(group)?;

        // Check 2: All dependencies reference existing members
        Self::check_valid_dependencies(group)?;

        // Check 3: No invalid progress values
        Self::check_progress_validity(group)?;

        // Check 4: Group state is consistent with member states
        Self::check_state_consistency(group)?;

        // Check 5: Strategy constraints are satisfied
        Self::check_strategy_constraints(group)?;

        // Check 6: Detect potential deadlocks
        Self::check_for_deadlocks(group)?;

        Ok(())
    }

    /// Check for circular dependencies using cycle detection
    pub fn check_circular_dependencies(group: &DownloadGroup) -> GroupResult<()> {
        let cycle_info = DagSolver::detect_cycles(group);
        if cycle_info.has_cycle {
            // Build a nice error message
            let first_member = cycle_info
                .cycle_members
                .first()
                .cloned()
                .unwrap_or_default();
            let cycle_str = if cycle_info.cycle_members.len() > 1 {
                format!("{}", cycle_info.cycle_members.join(" → "))
            } else {
                "self-reference".to_string()
            };

            return Err(GroupError::CircularDependency {
                cycle_members: cycle_info.cycle_members,
                first_member,
                description: format!(
                    "A circular dependency chain was detected. To fix, remove one of these dependency edges: {}",
                    cycle_str
                ),
            });
        }
        Ok(())
    }

    /// Check that all dependencies reference existing members
    pub fn check_valid_dependencies(group: &DownloadGroup) -> GroupResult<()> {
        for (member_id, member) in &group.members {
            for dep_id in &member.dependencies {
                if !group.members.contains_key(dep_id) {
                    return Err(GroupError::InvalidDependency {
                        dependent_member: member_id.clone(),
                        missing_dependency: dep_id.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Check that all member progress values are valid (0-100)
    pub fn check_progress_validity(group: &DownloadGroup) -> GroupResult<()> {
        for (member_id, member) in &group.members {
            if member.progress_percent < 0.0 || member.progress_percent > 100.0 {
                return Err(GroupError::InvalidProgress {
                    member_id: member_id.clone(),
                    progress: member.progress_percent,
                    reason: "Progress must be between 0 and 100%".to_string(),
                });
            }

            // Check that progress is consistent with state
            if member.state == GroupState::Completed && member.progress_percent < 100.0 {
                return Err(GroupError::InvalidProgress {
                    member_id: member_id.clone(),
                    progress: member.progress_percent,
                    reason: "Member is marked complete but progress < 100%".to_string(),
                });
            }
        }
        Ok(())
    }

    /// Check that group state is consistent with member states
    pub fn check_state_consistency(group: &DownloadGroup) -> GroupResult<()> {
        let member_states: Vec<_> = group.members.values().map(|m| m.state).collect();

        // All completed
        if member_states.iter().all(|s| *s == GroupState::Completed) {
            // Group should be completed
            if group.state != GroupState::Completed {
                return Err(GroupError::InconsistentState {
                    group_id: group.id.clone(),
                    issue: "All members completed but group state is not Completed".to_string(),
                    repair_suggestion: "Manually mark group as completed".to_string(),
                });
            }
        }

        // All pending
        if member_states.iter().all(|s| *s == GroupState::Pending) {
            if group.state != GroupState::Pending {
                return Err(GroupError::InconsistentState {
                    group_id: group.id.clone(),
                    issue: "All members pending but group is not in Pending state".to_string(),
                    repair_suggestion: "Reset group to Pending state".to_string(),
                });
            }
        }

        // Any failed member
        if member_states.iter().any(|s| *s == GroupState::Error) {
            if group.state != GroupState::Error {
                return Err(GroupError::InconsistentState {
                    group_id: group.id.clone(),
                    issue: "Group has a failed member but group state is not Error".to_string(),
                    repair_suggestion: "Mark group as Error or retry failed member".to_string(),
                });
            }
        }

        Ok(())
    }

    /// Check that execution strategy constraints are satisfied
    pub fn check_strategy_constraints(group: &DownloadGroup) -> GroupResult<()> {
        match group.strategy {
            crate::download_groups::ExecutionStrategy::Sequential => {
                // Only one member should be downloading at a time
                let downloading_count = group
                    .members
                    .values()
                    .filter(|m| m.state == GroupState::Downloading)
                    .count();

                if downloading_count > 1 {
                    return Err(GroupError::StrategyViolation {
                        group_id: group.id.clone(),
                        strategy: "Sequential".to_string(),
                        violation: format!(
                            "{} members are downloading simultaneously; only 1 allowed",
                            downloading_count
                        ),
                    });
                }
            }
            crate::download_groups::ExecutionStrategy::Parallel => {
                // All ready members should be downloading
                // (This is a softer constraint to avoid false positives)
            }
            crate::download_groups::ExecutionStrategy::Hybrid => {
                // Depends on specifics, no hard constraint here
            }
        }

        Ok(())
    }

    /// Detect potential deadlock conditions
    pub fn check_for_deadlocks(group: &DownloadGroup) -> GroupResult<()> {
        // A member is stuck if:
        // 1. Its state is Pending (not started)
        // 2. All its dependencies are also pending (will never complete)
        // 3. It's been pending for "a long time" (we don't track time here, so we just detect the logical deadlock)

        for (member_id, member) in &group.members {
            if member.state == GroupState::Pending {
                let deps_all_pending = member.dependencies.iter().all(|dep_id| {
                    group
                        .members
                        .get(dep_id)
                        .map(|m| m.state == GroupState::Pending)
                        .unwrap_or(false)
                });

                if deps_all_pending && !member.dependencies.is_empty() {
                    return Err(GroupError::Deadlock {
                        member_id: member_id.clone(),
                        stuck_reason: "All dependencies are still pending; will never start"
                            .to_string(),
                    });
                }
            }
        }

        Ok(())
    }

    /// Attempt to recover a group from an error state
    pub fn attempt_recovery(group: &mut DownloadGroup) -> GroupResult<String> {
        // Validate to see what's wrong
        match Self::validate_group(group) {
            Ok(()) => return Ok("Group is already valid".to_string()),
            Err(e) => {
                // Attempt targeted recovery based on error type
                match e {
                    GroupError::InvalidProgress { member_id, .. } => {
                        // Clamp progress to valid range
                        if let Some(member) = group.members.get_mut(&member_id) {
                            member.progress_percent = member.progress_percent.max(0.0).min(100.0);
                        }
                        Ok(format!("Clamped progress for member '{}'", member_id))
                    }
                    GroupError::InconsistentState { .. } => {
                        // Reset group state based on member states
                        let has_error =
                            group.members.values().any(|m| m.state == GroupState::Error);
                        let has_downloading = group
                            .members
                            .values()
                            .any(|m| m.state == GroupState::Downloading);
                        let all_completed = group
                            .members
                            .values()
                            .all(|m| m.state == GroupState::Completed);

                        if all_completed {
                            group.state = GroupState::Completed;
                        } else if has_error {
                            group.state = GroupState::Error;
                        } else if has_downloading {
                            group.state = GroupState::Downloading;
                        } else {
                            group.state = GroupState::Pending;
                        }

                        Ok("Reset group state based on member states".to_string())
                    }
                    _ => Err(e), // Other errors can't be auto-recovered
                }
            }
        }
    }
}

/// Event reporting for group errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupErrorEvent {
    pub group_id: String,
    pub error: String,
    pub timestamp_ms: u64,
    pub recovery_suggested: bool,
}

impl GroupErrorEvent {
    pub fn from_error(group_id: String, error: &GroupError) -> Self {
        let recovery_suggested = matches!(
            error,
            GroupError::InvalidProgress { .. } | GroupError::InconsistentState { .. }
        );

        Self {
            group_id,
            error: error.to_string(),
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            recovery_suggested,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_self_reference_detection() {
        let mut group = DownloadGroup::new("self-ref test");
        let member = GroupMember::new("http://example.com/file.zip".to_string(), vec![]);
        let id = member.id.clone();

        let bad_member =
            GroupMember::new("http://example.com/file.zip".to_string(), vec![id.clone()]);
        group.members.insert(id, bad_member);

        let result = GroupValidator::check_circular_dependencies(&group);
        assert!(result.is_err());
    }

    #[test]
    fn test_progress_validation() {
        let mut group = DownloadGroup::new("progress test");
        let mut member = GroupMember::new("http://example.com/file.zip".to_string(), vec![]);
        member.progress_percent = 150.0; // Invalid
        group.members.insert(member.id.clone(), member);

        let result = GroupValidator::check_progress_validity(&group);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_dependency_detection() {
        let mut group = DownloadGroup::new("invalid dep test");
        let member = GroupMember::new(
            "http://example.com/file.zip".to_string(),
            vec!["nonexistent-id".to_string()],
        );
        group.members.insert(member.id.clone(), member);

        let result = GroupValidator::check_valid_dependencies(&group);
        assert!(result.is_err());
    }

    #[test]
    fn test_valid_group_passes() {
        let mut group = DownloadGroup::new("valid test");
        let member = GroupMember::new("http://example.com/file.zip".to_string(), vec![]);
        group.members.insert(member.id.clone(), member);

        let result = GroupValidator::validate_group(&group);
        assert!(result.is_ok());
    }
}
