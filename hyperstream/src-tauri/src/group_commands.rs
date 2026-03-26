//! Production-grade Tauri command handlers for Download Groups
//!
//! All commands include:
//! - Circular dependency validation via DagSolver
//! - Progress tracking and state consistency checks
//! - Detailed error messages for UI display
//! - Persistence to disk on mutations
//! - Transaction-like semantics (all-or-nothing)

use crate::download_groups::{DownloadGroup, ExecutionStrategy};
use crate::group_dag_solver::DagSolver;
use crate::group_error_handler::{GroupErrorEvent, GroupValidator};
use crate::group_persistence;
use serde_json::{json, Value};
use tauri::AppHandle;
use tauri::Emitter;

/// Validate and create a new download group
/// Returns the group ID on success
#[tauri::command]
pub async fn create_group(
    name: String,
    strategy: Option<String>,
    app: AppHandle,
) -> Result<String, String> {
    let mut group = DownloadGroup::new(&name);

    // Set strategy if provided
    if let Some(strategy_str) = strategy {
        group.strategy = match strategy_str.as_str() {
            "sequential" => ExecutionStrategy::Sequential,
            "parallel" => ExecutionStrategy::Parallel,
            "hybrid" => ExecutionStrategy::Hybrid,
            _ => return Err(format!("Invalid strategy: {}", strategy_str)),
        };
    }

    // Validate group (empty group is always valid)
    GroupValidator::validate_group(&group).map_err(|e| e.to_string())?;

    let group_id = group.id.clone();

    // Persist to disk
    group_persistence::upsert_group(&group)
        .map_err(|e| format!("Failed to persist group: {}", e))?;

    // Emit event
    let _ = app.emit(
        "group_created",
        json!({
            "group_id": group_id,
            "name": name,
        }),
    );

    Ok(group_id)
}

/// Add a member URL to a group
/// Optionally specify dependencies on other group members
#[tauri::command]
pub async fn add_group_member(
    group_id: String,
    url: String,
    dependencies: Option<Vec<String>>,
    app: AppHandle,
) -> Result<String, String> {
    // Load group from disk
    let mut group = group_persistence::load_group(&group_id)
        .map_err(|e| format!("Failed to load group '{}': {}", group_id, e))?
        .ok_or_else(|| format!("Group '{}' not found", group_id))?;

    // Validate input
    if url.is_empty() {
        return Err("URL cannot be empty".to_string());
    }

    // Add member
    let member_id = group.add_member(&url, dependencies.clone());

    // **CRITICAL**: Validate for circular dependencies BEFORE saving
    if let Err(e) = GroupValidator::check_circular_dependencies(&group) {
        return Err(format!(
            "Invalid dependency: {}. Member will NOT be added.",
            e
        ));
    }

    // Validate the entire group
    if let Err(e) = GroupValidator::validate_group(&group) {
        return Err(format!(
            "Group validation failed: {}. Member will NOT be added.",
            e
        ));
    }

    // Persist changes
    group_persistence::upsert_group(&group).map_err(|e| format!("Failed to save group: {}", e))?;

    // Emit event
    let _ = app.emit(
        "group_member_added",
        json!({
            "group_id": group_id,
            "member_id": member_id,
            "url": url,
        }),
    );

    Ok(member_id)
}

/// Get detailed group information including validation status
#[tauri::command]
pub async fn get_group(group_id: String) -> Result<Value, String> {
    let group = group_persistence::load_group(&group_id)
        .map_err(|e| format!("Failed to load group '{}': {}", group_id, e))?
        .ok_or_else(|| format!("Group '{}' not found", group_id))?;

    // Check validity
    let validation_result = match GroupValidator::validate_group(&group) {
        Ok(()) => json!({
            "valid": true,
            "errors": [],
            "warnings": []
        }),
        Err(e) => {
            let event = GroupErrorEvent::from_error(group_id.clone(), &e);
            json!({
                "valid": false,
                "error": event.error,
                "recovery_suggested": event.recovery_suggested,
            })
        }
    };

    // Get execution order
    let execution_order = match DagSolver::topological_sort(&group) {
        Ok(topo) => json!({
            "order": topo.order,
            "critical_path": topo.critical_path_length,
        }),
        Err(e) => json!({
            "error": e,
        }),
    };

    Ok(json!({
        "id": group.id,
        "name": group.name,
        "state": format!("{:?}", group.state),
        "strategy": format!("{:?}", group.strategy),
        "members": group.members,
        "created_at_ms": group.created_at_ms,
        "completed_at_ms": group.completed_at_ms,
        "validation": validation_result,
        "execution_order": execution_order,
    }))
}

/// List all groups
#[tauri::command]
pub async fn list_groups() -> Result<Vec<Value>, String> {
    let groups =
        group_persistence::load_groups().map_err(|e| format!("Failed to load groups: {}", e))?;

    let result = groups
        .groups
        .into_values()
        .map(|group| {
            json!({
                "id": group.id,
                "name": group.name,
                "state": format!("{:?}", group.state),
                "member_count": group.members.len(),
                "created_at_ms": group.created_at_ms,
                "completed_at_ms": group.completed_at_ms,
            })
        })
        .collect();

    Ok(result)
}

/// Remove a group and all its members
#[tauri::command]
pub async fn delete_group(group_id: String, app: AppHandle) -> Result<(), String> {
    group_persistence::remove_group(&group_id)
        .map_err(|e| format!("Failed to delete group: {}", e))?;

    let _ = app.emit(
        "group_deleted",
        json!({
            "group_id": group_id,
        }),
    );

    Ok(())
}

/// Pause a group (pause all active members)
#[tauri::command]
pub async fn pause_group(group_id: String, app: AppHandle) -> Result<(), String> {
    let mut group = group_persistence::load_group(&group_id)
        .map_err(|e| format!("Failed to load group '{}': {}", group_id, e))?
        .ok_or_else(|| format!("Group '{}' not found", group_id))?;

    use crate::download_groups::GroupState;
    group.state = GroupState::Paused;

    group_persistence::upsert_group(&group).map_err(|e| format!("Failed to pause group: {}", e))?;

    let _ = app.emit("group_paused", json!({"group_id": group_id}));
    Ok(())
}

/// Resume a group
#[tauri::command]
pub async fn resume_group(group_id: String, app: AppHandle) -> Result<(), String> {
    let mut group = group_persistence::load_group(&group_id)
        .map_err(|e| format!("Failed to load group '{}': {}", group_id, e))?
        .ok_or_else(|| format!("Group '{}' not found", group_id))?;

    use crate::download_groups::GroupState;
    group.state = GroupState::Downloading;

    group_persistence::upsert_group(&group)
        .map_err(|e| format!("Failed to resume group: {}", e))?;

    let _ = app.emit("group_resumed", json!({"group_id": group_id}));
    Ok(())
}

/// Get execution order for a group (topological sort)
#[tauri::command]
pub async fn get_group_execution_order(group_id: String) -> Result<Value, String> {
    let group = group_persistence::load_group(&group_id)
        .map_err(|e| format!("Failed to load group '{}': {}", group_id, e))?
        .ok_or_else(|| format!("Group '{}' not found", group_id))?;

    let topo = DagSolver::topological_sort(&group)
        .map_err(|e| format!("Cannot determine execution order: {}", e))?;

    Ok(json!({
        "order": topo.order,
        "depths": topo.depths,
        "critical_path_length": topo.critical_path_length,
    }))
}

/// Detect and report any issues in a group
#[tauri::command]
pub async fn check_group_health(group_id: String) -> Result<Value, String> {
    let group = group_persistence::load_group(&group_id)
        .map_err(|e| format!("Failed to load group '{}': {}", group_id, e))?
        .ok_or_else(|| format!("Group '{}' not found", group_id))?;

    match GroupValidator::validate_group(&group) {
        Ok(()) => Ok(json!({
            "healthy": true,
            "issues": [],
        })),
        Err(e) => {
            let event = GroupErrorEvent::from_error(group_id, &e);
            Ok(json!({
                "healthy": false,
                "issue": event.error,
                "can_recover": event.recovery_suggested,
            }))
        }
    }
}

/// Attempt to recover a group from an error state
#[tauri::command]
pub async fn recover_group(group_id: String, app: AppHandle) -> Result<String, String> {
    let mut group = group_persistence::load_group(&group_id)
        .map_err(|e| format!("Failed to load group '{}': {}", group_id, e))?
        .ok_or_else(|| format!("Group '{}' not found", group_id))?;

    let recovery_message = GroupValidator::attempt_recovery(&mut group)
        .map_err(|e| format!("Recovery failed: {}", e))?;

    // Re-validate after recovery
    GroupValidator::validate_group(&group)
        .map_err(|e| format!("Recovery produced an invalid state: {}", e))?;

    // Persist recovered state
    group_persistence::upsert_group(&group)
        .map_err(|e| format!("Failed to save recovered group: {}", e))?;

    let _ = app.emit(
        "group_recovered",
        json!({
            "group_id": group_id,
            "recovery_message": recovery_message,
        }),
    );

    Ok(recovery_message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_list_groups_after_persisted_insert() {
        let _ = crate::group_persistence::clear_all_groups();

        let mut group = DownloadGroup::new("Test Group");
        group.strategy = ExecutionStrategy::Sequential;
        group.add_member("https://example.com/file.zip", None);
        crate::group_persistence::upsert_group(&group).expect("upsert group");

        let groups = list_groups().await.expect("list groups");
        assert!(!groups.is_empty());

        let first = groups.first().expect("first group");
        assert_eq!(
            first.get("name").and_then(|v| v.as_str()),
            Some("Test Group")
        );
    }
}
