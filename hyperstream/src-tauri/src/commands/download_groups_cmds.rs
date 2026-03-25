/// Download Groups Commands — Expose download groups management to frontend
///
/// Provides Tauri command handlers for download group management:
/// - create_download_group: Create new group with name
/// - add_member_to_group: Add member URL to group
/// - add_group_dependency: Add dependency between members
/// - get_group_details: Fetch full group state
/// - start_group_download: Begin group downloads
/// - pause_group_download: Pause all group downloads
/// - get_next_group_member: Get next member ready for download
/// - update_member_progress: Update download progress
/// - complete_group_member: Mark member as completed
/// - list_all_groups: Get all active groups
///
/// These commands bridge the gap between the GroupManager React component
/// and the underlying group_scheduler module.

use tauri::command;
use serde::{Serialize, Deserialize};
use lazy_static::lazy_static;
use std::sync::Mutex;
use crate::download_groups::{DownloadGroup, GroupMember, GroupState, ExecutionStrategy};
use crate::group_scheduler::{GroupScheduler, ExecutionState};

/// Response DTO for a member
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberResponse {
    pub id: String,
    pub url: String,
    pub progress_percent: f64,
    pub state: String,
    pub dependencies_count: usize,
    pub dependencies: Vec<String>,
}

/// Response DTO for a group
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupResponse {
    pub id: String,
    pub name: String,
    pub state: String,
    pub strategy: String,
    pub members: Vec<MemberResponse>,
    pub overall_progress: f64,
    pub completed_count: usize,
    pub total_count: usize,
    pub created_at_ms: u64,
    pub completed_at_ms: u64,
}

/// Response DTO for scheduler state summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerStateResponse {
    pub total_groups: usize,
    pub active_groups: usize,
    pub completed_groups: usize,
    pub total_members: usize,
    pub completed_members: usize,
    pub overall_progress: f64,
}

/// Global group scheduler instance
lazy_static! {
    static ref GLOBAL_GROUP_SCHEDULER: Mutex<GroupScheduler> = Mutex::new(GroupScheduler::new());
}

/// Helper: Convert GroupMember to MemberResponse
fn member_to_response(id: String, member: &GroupMember) -> MemberResponse {
    MemberResponse {
        id,
        url: member.url.clone(),
        progress_percent: member.progress_percent,
        state: format!("{:?}", member.state),
        dependencies_count: member.dependencies.len(),
        dependencies: member.dependencies.clone(),
    }
}

/// Helper: Convert DownloadGroup to GroupResponse
fn group_to_response(group: &DownloadGroup) -> GroupResponse {
    let members: Vec<MemberResponse> = group
        .members
        .iter()
        .map(|(id, member)| member_to_response(id.clone(), member))
        .collect();

    GroupResponse {
        id: group.id.clone(),
        name: group.name.clone(),
        state: format!("{:?}", group.state),
        strategy: format!("{:?}", group.strategy),
        members: members.clone(),
        overall_progress: group.overall_progress(),
        completed_count: group.completed_count(),
        total_count: group.member_count(),
        created_at_ms: group.created_at_ms,
        completed_at_ms: group.completed_at_ms,
    }
}

/// Create a new download group with the given name
#[command]
pub fn create_download_group(name: String) -> Result<GroupResponse, String> {
    let group = DownloadGroup::new(&name);
    let group_response = group_to_response(&group);

    let mut scheduler = GLOBAL_GROUP_SCHEDULER
        .lock()
        .map_err(|e| format!("Failed to acquire scheduler lock: {}", e))?;

    scheduler
        .schedule_group(group.clone())
        .map_err(|e| format!("Failed to schedule group: {}", e))?;

    // Persist to disk
    crate::group_persistence::upsert_group(&group)
        .map_err(|e| format!("Failed to persist group: {}", e))?;

    Ok(group_response)
}

/// Add a member URL to an existing group with optional dependencies
#[command]
pub fn add_member_to_group(
    group_id: String,
    url: String,
    dependencies: Option<Vec<String>>,
) -> Result<String, String> {
    let mut scheduler = GLOBAL_GROUP_SCHEDULER
        .lock()
        .map_err(|e| format!("Failed to acquire scheduler lock: {}", e))?;

    let group = scheduler
        .get_group_mut(&group_id)
        .ok_or_else(|| format!("Group {} not found", group_id))?;

    let member_id = group.add_member(&url, dependencies);

    // Persist changes
    crate::group_persistence::upsert_group(group)
        .map_err(|e| format!("Failed to persist group: {}", e))?;

    Ok(member_id)
}

/// Add a dependency relationship between two group members
#[command]
pub fn add_group_dependency(
    group_id: String,
    dependent_id: String,
    prerequisite_id: String,
) -> Result<(), String> {
    let mut scheduler = GLOBAL_GROUP_SCHEDULER
        .lock()
        .map_err(|e| format!("Failed to acquire scheduler lock: {}", e))?;

    let group = scheduler
        .get_group_mut(&group_id)
        .ok_or_else(|| format!("Group {} not found", group_id))?;

    // Validate that both members exist
    if !group.members.contains_key(&dependent_id) {
        return Err(format!(
            "Dependent member {} not found in group {}",
            dependent_id, group_id
        ));
    }

    if !group.members.contains_key(&prerequisite_id) {
        return Err(format!(
            "Prerequisite member {} not found in group {}",
            prerequisite_id, group_id
        ));
    }

    group.add_dependency(dependent_id, prerequisite_id);

    // Persist changes
    crate::group_persistence::upsert_group(group)
        .map_err(|e| format!("Failed to persist group: {}", e))?;

    Ok(())
}

/// Get full group details including all members and progress
#[command]
pub fn get_group_details(group_id: String) -> Result<GroupResponse, String> {
    let scheduler = GLOBAL_GROUP_SCHEDULER
        .lock()
        .map_err(|e| format!("Failed to acquire scheduler lock: {}", e))?;

    let group = scheduler
        .get_group(&group_id)
        .ok_or_else(|| format!("Group {} not found", group_id))?;

    Ok(group_to_response(group))
}

/// Start downloading all members in a group
#[command]
pub fn start_group_download(group_id: String) -> Result<(), String> {
    let mut scheduler = GLOBAL_GROUP_SCHEDULER
        .lock()
        .map_err(|e| format!("Failed to acquire scheduler lock: {}", e))?;

    scheduler.start_group(&group_id)?;

    Ok(())
}

/// Pause all active downloads in a group
#[command]
pub fn pause_group_download(group_id: String) -> Result<(), String> {
    let mut scheduler = GLOBAL_GROUP_SCHEDULER
        .lock()
        .map_err(|e| format!("Failed to acquire scheduler lock: {}", e))?;

    scheduler.pause_group(&group_id)?;

    Ok(())
}

/// Get the next group member ready for download based on dependencies
#[command]
pub fn get_next_group_member(group_id: String) -> Result<Option<MemberResponse>, String> {
    let scheduler = GLOBAL_GROUP_SCHEDULER
        .lock()
        .map_err(|e| format!("Failed to acquire scheduler lock: {}", e))?;

    let group = scheduler
        .get_group(&group_id)
        .ok_or_else(|| format!("Group {} not found", group_id))?;

    if let Some(member_id) = scheduler.get_next_member(&group_id) {
        if let Some(member) = group.members.get(&member_id) {
            return Ok(Some(member_to_response(member_id, member)));
        }
    }

    Ok(None)
}

/// Update member progress (0-100), auto-completes at 100%
#[command]
pub fn update_member_progress(
    group_id: String,
    member_id: String,
    progress_percent: f64,
) -> Result<(), String> {
    let mut scheduler = GLOBAL_GROUP_SCHEDULER
        .lock()
        .map_err(|e| format!("Failed to acquire scheduler lock: {}", e))?;

    let group = scheduler
        .get_group_mut(&group_id)
        .ok_or_else(|| format!("Group {} not found", group_id))?;

    if !group.members.contains_key(&member_id) {
        return Err(format!(
            "Member {} not found in group {}",
            member_id, group_id
        ));
    }

    // Clamp progress between 0 and 100
    let clamped_progress = progress_percent.clamp(0.0, 100.0);
    group.update_member_progress(&member_id, clamped_progress);

    // Persist progress updates periodically (every 10%)
    if clamped_progress % 10.0 < 1.0 || clamped_progress >= 100.0 {
        crate::group_persistence::upsert_group(group)
            .map_err(|e| format!("Failed to persist group: {}", e))?;
    }

    Ok(())
}

/// Mark a group member as completed
#[command]
pub fn complete_group_member(
    group_id: String,
    member_id: String,
) -> Result<(), String> {
    let mut scheduler = GLOBAL_GROUP_SCHEDULER
        .lock()
        .map_err(|e| format!("Failed to acquire scheduler lock: {}", e))?;

    scheduler.complete_member(&group_id, &member_id)?;

    Ok(())
}

/// List all active groups
#[command]
pub fn list_all_groups() -> Result<Vec<GroupResponse>, String> {
    let scheduler = GLOBAL_GROUP_SCHEDULER
        .lock()
        .map_err(|e| format!("Failed to acquire scheduler lock: {}", e))?;

    // Iterate through all groups and convert to responses
    let groups: Vec<GroupResponse> = scheduler
        .get_all_groups()
        .iter()
        .map(|group| group_to_response(group))
        .collect();

    Ok(groups)
}

/// Delete a download group
#[command]
pub fn delete_download_group(group_id: String) -> Result<(), String> {
    let mut scheduler = GLOBAL_GROUP_SCHEDULER
        .lock()
        .map_err(|e| format!("Failed to acquire scheduler lock: {}", e))?;

    // Remove from scheduler
    scheduler.remove_group(&group_id)?;

    // Remove from persistence
    crate::group_persistence::remove_group(&group_id)
        .map_err(|e| format!("Failed to remove group from disk: {}", e))?;

    Ok(())
}

/// Load groups from disk on startup
#[command]
pub fn restore_groups_from_disk() -> Result<usize, String> {
    let persisted = crate::group_persistence::load_groups()
        .map_err(|e| format!("Failed to load groups: {}", e))?;

    let mut scheduler = GLOBAL_GROUP_SCHEDULER
        .lock()
        .map_err(|e| format!("Failed to acquire scheduler lock: {}", e))?;

    let mut count = 0;
    for (_id, group) in persisted.groups {
        if let Ok(()) = scheduler.schedule_group(group) {
            count += 1;
        }
    }

    Ok(count)
}

/// Save current groups to disk (manual save)
#[command]
pub fn save_groups_to_disk() -> Result<usize, String> {
    let scheduler = GLOBAL_GROUP_SCHEDULER
        .lock()
        .map_err(|e| format!("Failed to acquire scheduler lock: {}", e))?;

    let groups: std::collections::HashMap<String, DownloadGroup> = scheduler
        .get_all_groups()
        .iter()
        .map(|g| (g.id.clone(), (*g).clone()))
        .collect();

    let count = groups.len();

    crate::group_persistence::save_groups(&groups)
        .map_err(|e| format!("Failed to save groups: {}", e))?;

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_download_group() {
        let response = create_download_group("Test Group".to_string());
        assert!(response.is_ok());
        let group = response.unwrap();
        assert_eq!(group.name, "Test Group");
        assert_eq!(group.state, "Pending");
        assert_eq!(group.total_count, 0);
    }

    #[test]
    fn test_add_member_to_group() {
        let group_result = create_download_group("Test Group 2".to_string());
        assert!(group_result.is_ok());
        let group = group_result.unwrap();

        let member_result = add_member_to_group(
            group.id.clone(),
            "https://example.com/file.zip".to_string(),
            None,
        );
        assert!(member_result.is_ok());
        let member_id = member_result.unwrap();
        assert!(!member_id.is_empty());
    }

    #[test]
    fn test_add_member_to_nonexistent_group() {
        let result = add_member_to_group(
            "nonexistent".to_string(),
            "https://example.com/file.zip".to_string(),
            None,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_get_group_details() {
        let group_result = create_download_group("Test Group 3".to_string());
        assert!(group_result.is_ok());
        let group = group_result.unwrap();

        let details = get_group_details(group.id.clone());
        assert!(details.is_ok());
        let group_details = details.unwrap();
        assert_eq!(group_details.id, group.id);
        assert_eq!(group_details.name, "Test Group 3");
    }

    #[test]
    fn test_start_group_download() {
        let group_result = create_download_group("Test Group 4".to_string());
        assert!(group_result.is_ok());
        let group = group_result.unwrap();

        let member_result = add_member_to_group(
            group.id.clone(),
            "https://example.com/file1.zip".to_string(),
            None,
        );
        assert!(member_result.is_ok());

        let start_result = start_group_download(group.id.clone());
        assert!(start_result.is_ok());

        let details = get_group_details(group.id);
        assert!(details.is_ok());
        let group_details = details.unwrap();
        assert_eq!(group_details.state, "Downloading");
    }

    #[test]
    fn test_pause_group_download() {
        let group_result = create_download_group("Test Group 5".to_string());
        assert!(group_result.is_ok());
        let group = group_result.unwrap();

        let member_result = add_member_to_group(
            group.id.clone(),
            "https://example.com/file2.zip".to_string(),
            None,
        );
        assert!(member_result.is_ok());

        let start_result = start_group_download(group.id.clone());
        assert!(start_result.is_ok());

        let pause_result = pause_group_download(group.id.clone());
        assert!(pause_result.is_ok());

        let details = get_group_details(group.id);
        assert!(details.is_ok());
        let group_details = details.unwrap();
        assert_eq!(group_details.state, "Paused");
    }

    #[test]
    fn test_update_member_progress() {
        let group_result = create_download_group("Test Group 6".to_string());
        assert!(group_result.is_ok());
        let group = group_result.unwrap();

        let member_result = add_member_to_group(
            group.id.clone(),
            "https://example.com/file3.zip".to_string(),
            None,
        );
        assert!(member_result.is_ok());
        let member_id = member_result.unwrap();

        let update_result = update_member_progress(group.id.clone(), member_id.clone(), 50.0);
        assert!(update_result.is_ok());

        let details = get_group_details(group.id);
        assert!(details.is_ok());
        let group_details = details.unwrap();
        assert!(group_details.members.len() > 0);
        assert_eq!(group_details.members[0].progress_percent, 50.0);
    }

    #[test]
    fn test_complete_group_member() {
        let group_result = create_download_group("Test Group 7".to_string());
        assert!(group_result.is_ok());
        let group = group_result.unwrap();

        let member_result = add_member_to_group(
            group.id.clone(),
            "https://example.com/file4.zip".to_string(),
            None,
        );
        assert!(member_result.is_ok());
        let member_id = member_result.unwrap();

        let complete_result = complete_group_member(group.id.clone(), member_id.clone());
        assert!(complete_result.is_ok());

        let details = get_group_details(group.id);
        assert!(details.is_ok());
        let group_details = details.unwrap();
        assert!(group_details.members.len() > 0);
        assert_eq!(group_details.members[0].state, "Completed");
    }

    #[test]
    fn test_list_all_groups() {
        let group1_result = create_download_group("Group A".to_string());
        assert!(group1_result.is_ok());

        let group2_result = create_download_group("Group B".to_string());
        assert!(group2_result.is_ok());

        let list_result = list_all_groups();
        assert!(list_result.is_ok());
        let groups = list_result.unwrap();
        assert!(groups.len() >= 2);
    }
}
