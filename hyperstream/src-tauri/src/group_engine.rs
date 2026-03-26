/// Download Group Engine Integration
///
/// Auto-starts downloads when dependencies are satisfied.
/// Emits events for real-time UI updates.
/// Handles download completion cascading.

use tauri::{AppHandle, Emitter};
use crate::download_groups::DownloadGroup;
use crate::group_scheduler::GLOBAL_GROUP_SCHEDULER;
use crate::download_groups::GroupState;
use crate::group_smart_queue::{Priority, SchedulingConstraint, SmartPriorityQueue};

const DEFAULT_GROUP_BANDWIDTH_BPS: u64 = 10_000_000;
const DEFAULT_MEMBER_SIZE_BYTES: u64 = 100_000_000;

fn derive_member_priority(group: &DownloadGroup, member_id: &str) -> Priority {
    let downstream_count = group
        .members
        .values()
        .filter(|m| m.dependencies.iter().any(|dep| dep == member_id))
        .count();

    match downstream_count {
        n if n >= 3 => Priority::Critical,
        2 => Priority::High,
        1 => Priority::Normal,
        _ => Priority::Low,
    }
}

pub fn prioritize_ready_members(
    group: &DownloadGroup,
    ready_members: Vec<String>,
    completed_members: Vec<String>,
    available_bandwidth: u64,
) -> Vec<String> {
    if ready_members.is_empty() {
        return Vec::new();
    }

    let mut queue = SmartPriorityQueue::new(available_bandwidth);
    let fair_share_speed = (available_bandwidth / ready_members.len() as u64).max(1);

    for completed in completed_members {
        queue.mark_completed(&completed);
    }

    for member_id in ready_members {
        if let Some(member) = group.members.get(&member_id) {
            queue.add_member(SchedulingConstraint {
                member_id: member_id.clone(),
                priority: derive_member_priority(group, &member_id),
                min_bandwidth: (fair_share_speed / 2).max(1),
                deadline_ms: 0,
                earliest_start_ms: 0,
                dependencies: member.dependencies.clone(),
                expected_size: DEFAULT_MEMBER_SIZE_BYTES,
                expected_speed: fair_share_speed,
            });
        }
    }

    let mut ordered = Vec::new();
    while let Some(member_id) = queue.pop() {
        ordered.push(member_id);
    }

    ordered
}

/// Check if any group members are ready to start and trigger their downloads
pub fn trigger_ready_downloads(app: &AppHandle) {
    let scheduler = match GLOBAL_GROUP_SCHEDULER.lock() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[GroupEngine] Failed to acquire scheduler lock: {}", e);
            return;
        }
    };

    // Collect data while holding lock, then emit events
    let mut events_to_emit = Vec::new();

    // Get all groups
    for group in scheduler.get_all_groups() {
        if group.state != GroupState::Downloading {
            continue;
        }

        // Get ready members (dependencies satisfied, state is Pending)
        let ready_members = scheduler.get_ready_members(&group.id);
        let completed_members = scheduler.get_completed_members(&group.id);
        let ordered_ready_members = prioritize_ready_members(
            group,
            ready_members,
            completed_members,
            DEFAULT_GROUP_BANDWIDTH_BPS,
        );

        for member_id in ordered_ready_members {
            if let Some(member) = group.members.get(&member_id) {
                events_to_emit.push((group.id.clone(), member_id.clone(), member.url.clone()));
            }
        }
    }
    
    // Drop the lock before emitting events
    drop(scheduler);

    // Now emit all events without holding the lock
    for (group_id, member_id, url) in events_to_emit {
        let _ = app.emit("group_member_ready", serde_json::json!({
            "group_id": group_id,
            "member_id": member_id,
            "url": url,
        }));
    }
}

/// Called when a download completes to check if any group members can now start
pub fn on_download_complete(app: &AppHandle, download_id: &str) {
    // First, find which group this download belongs to (hold lock briefly)
    let (group_id, member_id) = {
        let scheduler = match GLOBAL_GROUP_SCHEDULER.lock() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[GroupEngine] Failed to acquire scheduler lock: {}", e);
                return;
            }
        };

        let mut found = None;
        for group in scheduler.get_all_groups() {
            if group.members.contains_key(download_id) {
                found = Some((group.id.clone(), download_id.to_string()));
                break;
            }
        }
        
        match found {
            Some(f) => f,
            None => return, // Download not part of any group
        }
    };

    // Now update the group (re-acquire lock)
    {
        let mut scheduler = match GLOBAL_GROUP_SCHEDULER.lock() {
            Ok(s) => s,
            Err(_) => return,
        };
        
        let _ = scheduler.complete_member(&group_id, &member_id);
        
        // Check if group is complete and gather info for events
        let is_complete = scheduler.get_group(&group_id)
            .map(|g| g.is_complete())
            .unwrap_or(false);
        
        let group_name = scheduler.get_group(&group_id)
            .map(|g| g.name.clone())
            .unwrap_or_default();
        
        // Persist changes
        if let Some(group) = scheduler.get_group(&group_id) {
            let _ = crate::group_persistence::upsert_group(group);
        }
        
        // Drop lock before emitting
        drop(scheduler);
        
        if is_complete {
            let _ = app.emit("group_completed", serde_json::json!({
                "group_id": group_id,
                "name": group_name,
            }));
        }
    }
    
    // Trigger ready downloads
    trigger_ready_downloads(app);
}

/// Update member progress when download makes progress
pub fn update_member_progress(app: &AppHandle, download_id: &str, progress: f64) {
    // Find the group this download belongs to (hold lock briefly)
    let group_id = {
        let scheduler = match GLOBAL_GROUP_SCHEDULER.lock() {
            Ok(s) => s,
            Err(_) => return,
        };

        let mut found_group_id = None;
        for group in scheduler.get_all_groups() {
            if group.members.contains_key(download_id) {
                found_group_id = Some(group.id.clone());
                break;
            }
        }
        
        match found_group_id {
            Some(id) => id,
            None => return, // Not part of any group
        }
    };
    
    // Update progress (lock released)
    let _ = crate::commands::download_groups_cmds::update_member_progress(
        group_id.clone(),
        download_id.to_string(),
        progress,
    );
    
    // Emit progress event
    let _ = app.emit("group_progress", serde_json::json!({
        "group_id": group_id,
        "member_id": download_id,
        "progress": progress,
    }));
}

/// Handle download failure
pub fn on_download_failure(app: &AppHandle, download_id: &str, error_msg: &str) {
    // Find the group this download belongs to (hold lock briefly)
    let group_id = {
        let scheduler = match GLOBAL_GROUP_SCHEDULER.lock() {
            Ok(s) => s,
            Err(_) => return,
        };

        let mut found_group_id = None;
        for group in scheduler.get_all_groups() {
            if group.members.contains_key(download_id) {
                found_group_id = Some(group.id.clone());
                break;
            }
        }
        
        match found_group_id {
            Some(id) => id,
            None => return, // Not part of any group
        }
    };
    
    // Update failure state (re-acquire lock)
    {
        let mut scheduler = match GLOBAL_GROUP_SCHEDULER.lock() {
            Ok(s) => s,
            Err(_) => return,
        };
        
        let _ = scheduler.fail_member(&group_id, download_id, error_msg);
        
        // Persist changes
        if let Some(group) = scheduler.get_group(&group_id) {
            let _ = crate::group_persistence::upsert_group(group);
        }
    }
    
    // Emit failure event (lock released)
    let _ = app.emit("group_member_failed", serde_json::json!({
        "group_id": group_id,
        "member_id": download_id,
        "error": error_msg,
    }));
}

/// Initialize group engine on app startup
pub fn init_group_engine(app: &AppHandle) {
    // Load groups from disk
    match crate::commands::download_groups_cmds::restore_groups_from_disk() {
        Ok(count) => {
            eprintln!("[GroupEngine] Restored {} groups from disk", count);
        }
        Err(e) => {
            eprintln!("[GroupEngine] Failed to restore groups: {}", e);
        }
    }
    
    // Trigger any ready downloads
    trigger_ready_downloads(app);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::download_groups::DownloadGroup;

    #[test]
    fn test_group_engine_module() {
        // Just ensure module compiles
        assert!(true);
    }

    #[test]
    fn test_prioritize_ready_members_prefers_dependency_hub() {
        let mut group = DownloadGroup::new("Priority Test");
        let hub = group.add_member("https://example.com/hub.zip", None);
        let leaf = group.add_member("https://example.com/leaf.zip", None);
        let _downstream = group.add_member(
            "https://example.com/dependent.zip",
            Some(vec![hub.clone()]),
        );

        let ordered = prioritize_ready_members(
            &group,
            vec![hub.clone(), leaf.clone()],
            vec![],
            DEFAULT_GROUP_BANDWIDTH_BPS,
        );

        assert_eq!(ordered.first(), Some(&hub));
    }
}
