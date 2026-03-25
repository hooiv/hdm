// auto_recovery.rs — Automatic recovery and self-healing system
//
// Implements intelligent recovery procedures, automatic retry strategies,
// and self-healing mechanisms for download failures

use crate::resilience::{ClassifiedError, ErrorCategory, ResilienceEngine, DownloadHealth, HealthStatus};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use std::time::SystemTime;
use std::collections::HashMap;

/// Recovery action to perform
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryAction {
    pub action_id: String,
    pub download_id: String,
    pub action_type: RecoveryActionType,
    pub priority: u32,
    pub created_at: u64,
    pub executed_at: Option<u64>,
    pub status: RecoveryActionStatus,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RecoveryActionType {
    /// Resume the download
    Resume,
    /// Retry with new connection details
    Retry,
    /// Switch to alternative URL/mirror
    SwitchMirror,
    /// Reduce concurrent segments
    ReduceSegments,
    /// Enable proxy
    EnableProxy,
    /// Remove corrupted segments and resume
    CleanupAndResume,
    /// Switch network interface
    SwitchNetwork,
    /// Enable smaller chunk size
    ReduceChunkSize,
    /// Wait and retry
    BackoffRetry,
    /// Report for manual intervention
    ManualIntervention,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RecoveryActionStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Skipped,
}

/// Recovery plan for a failed download
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryPlan {
    pub plan_id: String,
    pub download_id: String,
    pub error_category: String,
    pub actions: Vec<RecoveryAction>,
    pub current_step: usize,
    pub created_at: u64,
    pub completed_at: Option<u64>,
    pub success: bool,
}

/// Automatic recovery engine
pub struct AutoRecoveryEngine {
    planned_recoveries: Arc<RwLock<HashMap<String, RecoveryPlan>>>,
    recovery_history: Arc<RwLock<Vec<RecoveryPlan>>>,
    resilience_engine: Arc<ResilienceEngine>,
}

impl AutoRecoveryEngine {
    pub fn new(resilience_engine: Arc<ResilienceEngine>) -> Self {
        Self {
            planned_recoveries: Arc::new(RwLock::new(HashMap::new())),
            recovery_history: Arc::new(RwLock::new(Vec::new())),
            resilience_engine,
        }
    }

    /// Generate recovery plan for a failed download
    pub fn generate_plan(
        &self,
        download_id: &str,
        error: &ClassifiedError,
        file_size: u64,
        _total_size: u64,
    ) -> RecoveryPlan {
        let mut actions = Vec::new();

        // Start with backoff retry (most errors respond to this)
        if error.category.is_retryable() {
            actions.push(self.create_action(
                download_id,
                RecoveryActionType::BackoffRetry,
                1,
                &error.message,
            ));
        }

        // Add category-specific recovery actions
        match error.category {
            ErrorCategory::RateLimited => {
                actions.push(self.create_action(
                    download_id,
                    RecoveryActionType::ReduceSegments,
                    2,
                    "Rate limited - reduce concurrency",
                ));
                actions.push(self.create_action(
                    download_id,
                    RecoveryActionType::BackoffRetry,
                    3,
                    "Wait longer before retry",
                ));
            }
            ErrorCategory::ServerError => {
                actions.push(self.create_action(
                    download_id,
                    RecoveryActionType::SwitchMirror,
                    2,
                    "Try alternative mirror/URL",
                ));
                actions.push(self.create_action(
                    download_id,
                    RecoveryActionType::Resume,
                    3,
                    "Resume after server recovery",
                ));
            }
            ErrorCategory::NetworkUnreachable => {
                actions.push(self.create_action(
                    download_id,
                    RecoveryActionType::SwitchNetwork,
                    2,
                    "Try different network",
                ));
                if file_size > 0 {
                    actions.push(self.create_action(
                        download_id,
                        RecoveryActionType::ReduceChunkSize,
                        3,
                        "Use smaller chunks for reliability",
                    ));
                }
                actions.push(self.create_action(
                    download_id,
                    RecoveryActionType::Resume,
                    4,
                    "Resume download",
                ));
            }
            ErrorCategory::TlsError => {
                actions.push(self.create_action(
                    download_id,
                    RecoveryActionType::EnableProxy,
                    2,
                    "Use proxy to bypass TLS issues",
                ));
                actions.push(self.create_action(
                    download_id,
                    RecoveryActionType::SwitchNetwork,
                    3,
                    "Try different network",
                ));
            }
            ErrorCategory::DiskError => {
                actions.push(self.create_action(
                    download_id,
                    RecoveryActionType::ManualIntervention,
                    1,
                    "Manual intervention required - check disk space",
                ));
            }
            ErrorCategory::CorruptedData => {
                actions.push(self.create_action(
                    download_id,
                    RecoveryActionType::CleanupAndResume,
                    1,
                    "Remove corrupted data and resume",
                ));
            }
            _ => {
                actions.push(self.create_action(
                    download_id,
                    RecoveryActionType::Resume,
                    1,
                    "Resume download",
                ));
            }
        }

        // If all else fails, mark for manual review
        if actions.len() < 2 {
            actions.push(self.create_action(
                download_id,
                RecoveryActionType::ManualIntervention,
                100, // Low priority
                "Scheduled for manual review",
            ));
        }

        let actions_count = actions.len() as u32;
        let plan = RecoveryPlan {
            plan_id: format!("plan-{}-{}", download_id, current_timestamp_ms()),
            download_id: download_id.to_string(),
            error_category: format!("{:?}", error.category),
            actions: actions.into_iter().map(|mut a| {
                a.priority = actions_count;
                a
            }).collect(),
            current_step: 0,
            created_at: current_timestamp_ms(),
            completed_at: None,
            success: false,
        };

        self.planned_recoveries
            .write()
            .unwrap()
            .insert(download_id.to_string(), plan.clone());

        plan
    }

    fn create_action(
        &self,
        download_id: &str,
        action_type: RecoveryActionType,
        priority: u32,
        metadata_msg: &str,
    ) -> RecoveryAction {
        RecoveryAction {
            action_id: format!("action-{}-{}", download_id, current_timestamp_ms()),
            download_id: download_id.to_string(),
            action_type,
            priority,
            created_at: current_timestamp_ms(),
            executed_at: None,
            status: RecoveryActionStatus::Pending,
            metadata: {
                let mut m = HashMap::new();
                m.insert("reason".to_string(), metadata_msg.to_string());
                m
            },
        }
    }

    /// Get next recovery action to execute for a download
    pub fn get_next_action(&self, download_id: &str) -> Option<RecoveryAction> {
        let plans = self.planned_recoveries.read().unwrap();
        plans.get(download_id).and_then(|plan| {
            plan.actions
                .iter()
                .find(|a| a.status == RecoveryActionStatus::Pending)
                .cloned()
        })
    }

    /// Mark action as executed
    pub fn mark_action_executed(
        &self,
        download_id: &str,
        action_id: &str,
        success: bool,
    ) -> Option<()> {
        let mut plans = self.planned_recoveries.write().unwrap();
        if let Some(plan) = plans.get_mut(download_id) {
            if let Some(action) = plan.actions.iter_mut().find(|a| a.action_id == action_id) {
                action.executed_at = Some(current_timestamp_ms());
                action.status = if success {
                    RecoveryActionStatus::Completed
                } else {
                    RecoveryActionStatus::Failed
                };

                // If successful, mark plan as complete
                if success {
                    plan.completed_at = Some(current_timestamp_ms());
                    plan.success = true;
                    self.resilience_engine.record_success(download_id);
                    self.recovery_history
                        .write()
                        .unwrap()
                        .push(plan.clone());
                    return Some(());
                }

                // Move to next step
                plan.current_step += 1;

                // If no more actions, mark as requiring manual intervention
                if plan.current_step >= plan.actions.len() {
                    plan.completed_at = Some(current_timestamp_ms());
                    self.recovery_history
                        .write()
                        .unwrap()
                        .push(plan.clone());
                }

                return Some(());
            }
        }
        None
    }

    /// Get recovery statistics
    pub fn get_stats(&self) -> RecoveryStats {
        let history = self.recovery_history.read().unwrap();
        let successful = history.iter().filter(|p| p.success).count();
        let failed = history.len() - successful;

        let error_categories: HashMap<String, u32> = history.iter().fold(
            HashMap::new(),
            |mut acc, plan| {
                *acc.entry(plan.error_category.clone()).or_insert(0) += 1;
                acc
            },
        );

        RecoveryStats {
            total_recovery_plans: history.len(),
            successful_recoveries: successful,
            failed_recoveries: failed,
            success_rate: if history.len() > 0 {
                (successful as f32 / history.len() as f32) * 100.0
            } else {
                0.0
            },
            error_categories,
        }
    }

    /// Get all pending recovery actions
    pub fn get_pending_actions(&self) -> Vec<RecoveryAction> {
        self.planned_recoveries
            .read()
            .unwrap()
            .values()
            .flat_map(|plan| {
                plan.actions
                    .iter()
                    .filter(|a| a.status == RecoveryActionStatus::Pending)
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryStats {
    pub total_recovery_plans: usize,
    pub successful_recoveries: usize,
    pub failed_recoveries: usize,
    pub success_rate: f32,
    pub error_categories: HashMap<String, u32>,
}

fn current_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recovery_plan_generation() {
        let resilience = Arc::new(ResilienceEngine::new());
        let recovery = AutoRecoveryEngine::new(resilience);

        let error = ClassifiedError::new(ErrorCategory::RateLimited, "Too many requests");
        let plan = recovery.generate_plan("dl1", &error, 100, 1000);

        assert_eq!(plan.download_id, "dl1");
        assert!(!plan.actions.is_empty());
        assert!(plan.actions[0].action_type == RecoveryActionType::BackoffRetry);
    }

    #[test]
    fn test_action_tracking() {
        let resilience = Arc::new(ResilienceEngine::new());
        let recovery = AutoRecoveryEngine::new(resilience);

        let error = ClassifiedError::new(ErrorCategory::NetworkUnreachable, "Timeout");
        let plan = recovery.generate_plan("dl1", &error, 50, 500);

        let first_action = recovery.get_next_action("dl1").unwrap();
        assert_eq!(first_action.status, RecoveryActionStatus::Pending);

        recovery.mark_action_executed("dl1", &first_action.action_id, true);

        let stats = recovery.get_stats();
        assert_eq!(stats.successful_recoveries, 1);
    }
}
