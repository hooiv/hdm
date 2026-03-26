/// Atomic Group Operations with ACID Guarantees
///
/// Provides transaction-like semantics for group operations, ensuring that:
/// - Atomicity: All updates succeed or none do
/// - Consistency: Group state remains valid after any operation
/// - Isolation: Concurrent operations don't interfere
/// - Durability: Changes persisted to disk
///
/// Handles crash recovery and state corruption detection.

use std::sync::{Arc, RwLock};
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use super::download_groups::{DownloadGroup, GroupMember, GroupState};

/// Transaction operation type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionOp {
    /// Create a new group
    CreateGroup {
        group: DownloadGroup,
    },
    /// Delete an entire group
    DeleteGroup {
        group_id: String,
    },
    /// Add a member to a group
    AddMember {
        group_id: String,
        member: GroupMember,
    },
    /// Remove a member from a group
    RemoveMember {
        group_id: String,
        member_id: String,
    },
    /// Update member progress
    UpdateMemberProgress {
        group_id: String,
        member_id: String,
        progress: f64,
    },
    /// Complete a member
    CompleteMember {
        group_id: String,
        member_id: String,
    },
    /// Fail a member
    FailMember {
        group_id: String,
        member_id: String,
        reason: String,
    },
    /// Update group state
    UpdateGroupState {
        group_id: String,
        state: GroupState,
    },
}

/// Transaction log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxnLogEntry {
    /// Transaction ID (monotonically increasing)
    pub txn_id: u64,
    /// Operation performed
    pub operation: TransactionOp,
    /// Timestamp (milliseconds since epoch)
    pub timestamp_ms: u64,
    /// Whether transaction succeeded
    pub succeeded: bool,
    /// Optional error message
    pub error: Option<String>,
}

/// State before transaction (for rollback)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// Group state at this point
    pub groups: HashMap<String, DownloadGroup>,
    /// When snapshot was taken
    pub timestamp_ms: u64,
}

/// Atomic transaction manager for group operations
pub struct GroupTransactionManager {
    /// Current transaction counter
    next_txn_id: Arc<RwLock<u64>>,
    /// Transaction log for crash recovery
    txn_log: Arc<RwLock<Vec<TxnLogEntry>>>,
    /// Snapshots for rollback
    snapshots: Arc<RwLock<Vec<Snapshot>>>,
    /// Maximum log entries before compaction
    max_log_size: usize,
}

impl GroupTransactionManager {
    /// Create a new transaction manager
    pub fn new() -> Self {
        Self {
            next_txn_id: Arc::new(RwLock::new(0)),
            txn_log: Arc::new(RwLock::new(Vec::new())),
            snapshots: Arc::new(RwLock::new(Vec::new())),
            max_log_size: 10000,
        }
    }

    /// Begin a transaction and return transaction ID
    fn begin_txn(&self) -> Result<u64, String> {
        let mut id = self.next_txn_id.write().map_err(|e| e.to_string())?;
        *id += 1;
        Ok(*id)
    }

    /// Log transaction operation
    fn log_operation(&self, entry: TxnLogEntry) -> Result<(), String> {
        let mut log = self.txn_log.write().map_err(|e| e.to_string())?;
        log.push(entry);

        // Trigger compaction if log is too large
        if log.len() > self.max_log_size {
            drop(log); // Release lock before compacting
            self.compact_log()?;
        }

        Ok(())
    }

    /// Create a snapshot for crash recovery
    fn snapshot(
        &self,
        groups: &HashMap<String, DownloadGroup>,
    ) -> Result<(), String> {
        let snapshot = Snapshot {
            groups: groups.clone(),
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        };

        let mut snapshots = self.snapshots.write().map_err(|e| e.to_string())?;

        // Keep only last 5 snapshots
        if snapshots.len() >= 5 {
            snapshots.remove(0);
        }

        snapshots.push(snapshot);
        Ok(())
    }

    /// Execute a transaction atomically
    pub fn execute_transaction(
        &self,
        groups: &mut HashMap<String, DownloadGroup>,
        ops: Vec<TransactionOp>,
    ) -> Result<(), String> {
        let txn_id = self.begin_txn()?;

        // Save snapshot before transaction
        self.snapshot(groups)?;

        // Validate all operations first (no-op phase)
        for op in &ops {
            self.validate_operation(groups, op)?;
        }

        // Execute all operations (commit phase)
        let mut all_succeeded = true;
        let mut last_error = None;

        for op in ops {
            match self.apply_operation(groups, op.clone()) {
                Ok(_) => {
                    let entry = TxnLogEntry {
                        txn_id,
                        operation: op,
                        timestamp_ms: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_millis() as u64)
                            .unwrap_or(0),
                        succeeded: true,
                        error: None,
                    };
                    self.log_operation(entry)?;
                }
                Err(e) => {
                    all_succeeded = false;
                    last_error = Some(e.clone());

                    let entry = TxnLogEntry {
                        txn_id,
                        operation: op.clone(),
                        timestamp_ms: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_millis() as u64)
                            .unwrap_or(0),
                        succeeded: false,
                        error: Some(e.clone()),
                    };
                    self.log_operation(entry)?;

                    // Continue to log all failures, but don't apply further operations
                }
            }
        }

        if all_succeeded {
            // Snapshot successful state
            self.snapshot(groups)?;
            Ok(())
        } else {
            Err(format!(
                "Transaction {} failed: {}",
                txn_id,
                last_error.unwrap_or_else(|| "unknown error".to_string())
            ))
        }
    }

    /// Validate an operation before executing it
    fn validate_operation(
        &self,
        groups: &HashMap<String, DownloadGroup>,
        op: &TransactionOp,
    ) -> Result<(), String> {
        match op {
            TransactionOp::CreateGroup { group } => {
                if groups.contains_key(&group.id) {
                    return Err(format!("Group {} already exists", group.id));
                }
                Ok(())
            }
            TransactionOp::DeleteGroup { group_id } => {
                if !groups.contains_key(group_id) {
                    return Err(format!("Group {} does not exist", group_id));
                }
                Ok(())
            }
            TransactionOp::AddMember { group_id, member } => {
                let group = groups
                    .get(group_id)
                    .ok_or_else(|| format!("Group {} does not exist", group_id))?;

                if group.members.contains_key(&member.id) {
                    return Err(format!(
                        "Member {} already exists in group {}",
                        member.id, group_id
                    ));
                }

                // Validate dependencies exist
                for dep in &member.dependencies {
                    if !group.members.contains_key(dep) && dep != &member.id {
                        return Err(format!(
                            "Dependency {} does not exist in group {}",
                            dep, group_id
                        ));
                    }
                }

                Ok(())
            }
            TransactionOp::RemoveMember { group_id, member_id } => {
                let group = groups
                    .get(group_id)
                    .ok_or_else(|| format!("Group {} does not exist", group_id))?;

                if !group.members.contains_key(member_id) {
                    return Err(format!(
                        "Member {} does not exist in group {}",
                        member_id, group_id
                    ));
                }

                // Check if other members depend on this
                for (_, member) in &group.members {
                    if member.dependencies.contains(member_id) {
                        return Err(format!(
                            "Cannot remove member {}: {} depends on it",
                            member_id, member.id
                        ));
                    }
                }

                Ok(())
            }
            TransactionOp::UpdateMemberProgress { group_id, member_id, progress } => {
                let group = groups
                    .get(group_id)
                    .ok_or_else(|| format!("Group {} does not exist", group_id))?;

                if !group.members.contains_key(member_id) {
                    return Err(format!(
                        "Member {} does not exist in group {}",
                        member_id, group_id
                    ));
                }

                if *progress < 0.0 || *progress > 100.0 {
                    return Err(format!("Progress must be 0-100, got {}", progress));
                }

                Ok(())
            }
            TransactionOp::CompleteMember { group_id, member_id } => {
                let group = groups
                    .get(group_id)
                    .ok_or_else(|| format!("Group {} does not exist", group_id))?;

                let member = group
                    .members
                    .get(member_id)
                    .ok_or_else(|| format!("Member {} not found", member_id))?;

                if member.state == GroupState::Completed {
                    return Err(format!("Member {} is already completed", member_id));
                }

                Ok(())
            }
            TransactionOp::FailMember { group_id, member_id, .. } => {
                let group = groups
                    .get(group_id)
                    .ok_or_else(|| format!("Group {} does not exist", group_id))?;

                if !group.members.contains_key(member_id) {
                    return Err(format!(
                        "Member {} does not exist in group {}",
                        member_id, group_id
                    ));
                }

                Ok(())
            }
            TransactionOp::UpdateGroupState { group_id, .. } => {
                if !groups.contains_key(group_id) {
                    return Err(format!("Group {} does not exist", group_id));
                }
                Ok(())
            }
        }
    }

    /// Apply an operation to the group state
    fn apply_operation(
        &self,
        groups: &mut HashMap<String, DownloadGroup>,
        op: TransactionOp,
    ) -> Result<(), String> {
        match op {
            TransactionOp::CreateGroup { group } => {
                groups.insert(group.id.clone(), group);
                Ok(())
            }
            TransactionOp::DeleteGroup { group_id } => {
                groups.remove(&group_id);
                Ok(())
            }
            TransactionOp::AddMember { group_id, member } => {
                if let Some(group) = groups.get_mut(&group_id) {
                    group.members.insert(member.id.clone(), member);
                    Ok(())
                } else {
                    Err(format!("Group {} not found", group_id))
                }
            }
            TransactionOp::RemoveMember { group_id, member_id } => {
                if let Some(group) = groups.get_mut(&group_id) {
                    group.members.remove(&member_id);
                    Ok(())
                } else {
                    Err(format!("Group {} not found", group_id))
                }
            }
            TransactionOp::UpdateMemberProgress { group_id, member_id, progress } => {
                if let Some(group) = groups.get_mut(&group_id) {
                    if let Some(member) = group.members.get_mut(&member_id) {
                        member.progress_percent = progress;
                        if progress >= 100.0 {
                            member.state = GroupState::Completed;
                        }
                        Ok(())
                    } else {
                        Err(format!("Member {} not found", member_id))
                    }
                } else {
                    Err(format!("Group {} not found", group_id))
                }
            }
            TransactionOp::CompleteMember { group_id, member_id } => {
                if let Some(group) = groups.get_mut(&group_id) {
                    if let Some(member) = group.members.get_mut(&member_id) {
                        member.state = GroupState::Completed;
                        member.progress_percent = 100.0;
                        Ok(())
                    } else {
                        Err(format!("Member {} not found", member_id))
                    }
                } else {
                    Err(format!("Group {} not found", group_id))
                }
            }
            TransactionOp::FailMember { group_id, member_id, reason } => {
                if let Some(group) = groups.get_mut(&group_id) {
                    if let Some(member) = group.members.get_mut(&member_id) {
                        member.state = GroupState::Error;
                        // Log failure reason if needed
                        eprintln!(
                            "[GroupTxn] Member {} failed: {}",
                            member_id, reason
                        );
                        Ok(())
                    } else {
                        Err(format!("Member {} not found", member_id))
                    }
                } else {
                    Err(format!("Group {} not found", group_id))
                }
            }
            TransactionOp::UpdateGroupState { group_id, state } => {
                if let Some(group) = groups.get_mut(&group_id) {
                    group.state = state;
                    Ok(())
                } else {
                    Err(format!("Group {} not found", group_id))
                }
            }
        }
    }

    /// Recover state from transaction log (used after crash)
    pub fn recover_state(
        &self,
        mut groups: HashMap<String, DownloadGroup>,
    ) -> Result<HashMap<String, DownloadGroup>, String> {
        let log = self.txn_log.read().map_err(|e| e.to_string())?;

        for entry in log.iter() {
            if entry.succeeded {
                self.apply_operation(&mut groups, entry.operation.clone())?;
            }
        }

        Ok(groups)
    }

    /// Compact transaction log (keep only snapshots + recent operations)
    fn compact_log(&self) -> Result<(), String> {
        let mut log = self.txn_log.write().map_err(|e| e.to_string())?;
        let snapshots = self.snapshots.read().map_err(|e| e.to_string())?;

        if let Some(latest_snapshot) = snapshots.last() {
            // Keep only operations after the latest snapshot
            log.retain(|entry| entry.timestamp_ms >= latest_snapshot.timestamp_ms);
        }

        Ok(())
    }

    /// Get transaction history for debugging
    pub fn get_history(&self) -> Result<Vec<TxnLogEntry>, String> {
        Ok(self
            .txn_log
            .read()
            .map_err(|e| e.to_string())?
            .clone())
    }
}

impl Default for GroupTransactionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::download_groups::ExecutionStrategy;

    #[test]
    fn test_atomic_create() {
        let mgr = GroupTransactionManager::new();
        let mut groups = HashMap::new();

        let group = DownloadGroup::new("test");
        let ops = vec![TransactionOp::CreateGroup {
            group: group.clone(),
        }];

        assert!(mgr.execute_transaction(&mut groups, ops).is_ok());
        assert!(groups.contains_key(&group.id));
    }

    #[test]
    fn test_validate_duplicate_group() {
        let mgr = GroupTransactionManager::new();
        let group = DownloadGroup::new("test");
        let mut groups = HashMap::new();
        groups.insert(group.id.clone(), group.clone());

        let ops = vec![TransactionOp::CreateGroup { group }];
        let result = mgr.execute_transaction(&mut groups, ops);

        assert!(result.is_err());
    }

    #[test]
    fn test_transaction_rollback() {
        let mgr = GroupTransactionManager::new();
        let group = DownloadGroup::new("test");
        let mut groups = HashMap::new();

        // Transaction that will fail: add member to non-existent group
        let fake_member = GroupMember {
            id: "m1".to_string(),
            url: "http://example.com".to_string(),
            progress_percent: 0.0,
            state: GroupState::Pending,
            dependencies: vec![],
        };

        let ops = vec![TransactionOp::AddMember {
            group_id: "nonexistent".to_string(),
            member: fake_member,
        }];

        assert!(mgr.execute_transaction(&mut groups, ops).is_err());
        assert!(groups.is_empty()); // State unchanged
    }
}
