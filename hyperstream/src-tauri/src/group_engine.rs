/// Download Group Engine Integration
///
/// Auto-starts downloads when dependencies are satisfied.
/// Emits events for real-time UI updates.
/// Handles download completion cascading.

use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};
use crate::group_scheduler::GLOBAL_GROUP_SCHEDULER;
use crate::download_groups::GroupState;

/// Check if any group members are ready to start and trigger their downloads
pub fn trigger_ready_downloads(app: &AppHandle) {
    let mut scheduler = match GLOBAL_GROUP_SCHEDULER.lock() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[GroupEngine] Failed to acquire scheduler lock: {}", e);
            return;
        }
    };

    // Get all groups
    let group_ids: Vec<String> = scheduler
        .get_all_groups()
        .iter()
        .map(|g| g.id.clone())
        .collect();

    for group_id in group_ids {
        // Check if group is currently running
        if let Some(group) = scheduler.get_group(&group_id) {
            if group.state != GroupState::Downloading {
                continue;
            }

            // Get ready members (dependencies satisfied, state is Pending)
            let ready_members = scheduler.get_ready_members(&group_id);

            for member_id in ready_members {
                if let Some(member) = group.members.get(&member_id) {
                    let url = member.url.clone();
                    
                    // Emit event to frontend to start download
                    let _ = app.emit("group_member_ready", serde_json::json!({
                        "group_id": group_id,
                        "member_id": member_id,
                        "url": url,
                    }));

                    // TODO: Automatically invoke start_download here
                    // For now, the frontend will handle this event
                }
            }
        }
    }
}

/// Called when a download completes to check if any group members can now start
pub fn on_download_complete(app: &AppHandle, download_id: &str) {
    // Find which group(s) this download belongs to
    let scheduler = match GLOBAL_GROUP_SCHEDULER.lock() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[GroupEngine] Failed to acquire scheduler lock: {}", e);
            return;
        }
    };

    for group in scheduler.get_all_groups() {
        // Check if this download_id matches any member
        for (member_id, member) in &group.members {
            // Match by URL or ID (we'll need to track this mapping)
            if member_id == download_id {
                // Found it! Mark as complete
                drop(scheduler); // Release lock before calling other functions
                
                if let Ok(mut scheduler) = GLOBAL_GROUP_SCHEDULER.lock() {
                    let _ = scheduler.complete_member(&group.id, member_id);
                    
                    // Check if group is complete
                    if let Some(group) = scheduler.get_group(&group.id) {
                        if group.is_complete() {
                            let _ = app.emit("group_completed", serde_json::json!({
                                "group_id": group.id,
                                "name": group.name,
                            }));
                        }
                    }
                    
                    // Persist changes
                    if let Some(group) = scheduler.get_group(&group.id) {
                        let _ = crate::group_persistence::upsert_group(group);
                    }
                }
                
                // Trigger ready downloads
                trigger_ready_downloads(app);
                return;
            }
        }
    }
}

/// Update member progress when download makes progress
pub fn update_member_progress(app: &AppHandle, download_id: &str, progress: f64) {
    let mut scheduler = match GLOBAL_GROUP_SCHEDULER.lock() {
        Ok(s) => s,
        Err(_) => return,
    };

    for group in scheduler.get_all_groups() {
        for (member_id, _member) in &group.members {
            if member_id == download_id {
                let group_id = group.id.clone();
                drop(scheduler); // Release lock
                
                // Update progress
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
                
                return;
            }
        }
    }
}

/// Handle download failure
pub fn on_download_failure(app: &AppHandle, download_id: &str, error_msg: &str) {
    let mut scheduler = match GLOBAL_GROUP_SCHEDULER.lock() {
        Ok(s) => s,
        Err(_) => return,
    };

    for group in scheduler.get_all_groups() {
        for (member_id, _) in &group.members {
            if member_id == download_id {
                let group_id = group.id.clone();
                drop(scheduler); // Release lock
                
                if let Ok(mut scheduler) = GLOBAL_GROUP_SCHEDULER.lock() {
                    let _ = scheduler.fail_member(&group_id, member_id, error_msg);
                    
                    // Emit failure event
                    let _ = app.emit("group_member_failed", serde_json::json!({
                        "group_id": group_id,
                        "member_id": member_id,
                        "error": error_msg,
                    }));
                    
                    // Persist changes
                    if let Some(group) = scheduler.get_group(&group_id) {
                        let _ = crate::group_persistence::upsert_group(group);
                    }
                }
                
                return;
            }
        }
    }
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

    #[test]
    fn test_group_engine_module() {
        // Just ensure module compiles
        assert!(true);
    }
}
