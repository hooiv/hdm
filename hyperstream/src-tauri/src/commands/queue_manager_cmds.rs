/// Queue Manager Commands — Expose queue_manager module to frontend
///
/// Provides Tauri command handlers for queue management:
/// - get_queue_status: Current queue state and counts
/// - get_queue_groups: Available download groups
/// - get_queue_items: Detailed list of queued downloads
/// - remove_from_queue: Remove specific download from queue
/// - set_queue_priority: Change download priority
/// - move_queue_item_to_front: Move download to front of queue
/// - clear_download_queue: Clear all queued downloads
/// - pause_queue / resume_queue: Pause/resume queue processing
/// - set_max_concurrent_downloads: Set max parallel downloads
///
/// These commands bridge the gap between the QueueManager React component
/// and the underlying queue_manager module.

use tauri::command;
use serde::{Serialize, Deserialize};
use crate::queue_manager::{self, DOWNLOAD_QUEUE, DownloadPriority};

/// Queue download - matches the struct in queue_manager.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedDownload {
    pub id: String,
    pub url: String,
    pub path: String,
    pub filename: String,
    pub priority: String,
    pub added_at: u64,
    pub custom_headers: Option<std::collections::HashMap<String, String>>,
    pub expected_checksum: Option<String>,
    pub retry_count: u32,
    pub max_retries: u32,
    pub retry_delay_ms: u32,
    pub depends_on: Vec<String>,
    pub custom_segments: Option<u32>,
    pub group: Option<String>,
}

/// Current queue status snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStatus {
    pub max_concurrent: u32,
    pub active_count: u32,
    pub queued_count: u32,
    pub queued_items: Vec<QueuedDownload>,
    pub active_ids: Vec<String>,
    pub paused: bool,
    pub blocked_ids: Vec<String>,
}

/// Get current queue status including all pending downloads
#[command]
pub fn get_queue_status() -> Result<QueueStatus, String> {
    let queue = DOWNLOAD_QUEUE
        .lock()
        .map_err(|e| format!("Failed to acquire queue lock: {}", e))?;
    
    let queued_items: Vec<QueuedDownload> = queue
        .queue
        .iter()
        .map(|dl| QueuedDownload {
            id: dl.id.clone(),
            url: dl.url.clone(),
            path: dl.path.clone(),
            filename: dl.path.split('\\').last().unwrap_or("Unknown").to_string(),
            priority: format!("{:?}", dl.priority),
            added_at: dl.added_at,
            custom_headers: dl.custom_headers.clone(),
            expected_checksum: dl.expected_checksum.clone(),
            retry_count: dl.retry_count,
            max_retries: dl.max_retries,
            retry_delay_ms: dl.retry_delay_ms,
            depends_on: dl.depends_on.clone(),
            custom_segments: dl.custom_segments,
            group: dl.group.clone(),
        })
        .collect();
    
    Ok(QueueStatus {
        max_concurrent: queue.max_concurrent,
        active_count: queue.active_set.len() as u32,
        queued_count: queue.queue.len() as u32,
        queued_items,
        active_ids: queue.active_set.iter().cloned().collect(),
        paused: queue.paused,
        blocked_ids: queue.blocked_ids(),
    })
}

/// Get all available download groups
#[command]
pub fn get_queue_groups() -> Result<Vec<String>, String> {
    let queue = DOWNLOAD_QUEUE
        .lock()
        .map_err(|e| format!("Failed to acquire queue lock: {}", e))?;
    
    Ok(queue.groups())
}

/// Get detailed list of all queued downloads (same as get_queue_status queued_items)
#[command]
pub fn get_queue_items() -> Result<Vec<QueuedDownload>, String> {
    let queue = DOWNLOAD_QUEUE
        .lock()
        .map_err(|e| format!("Failed to acquire queue lock: {}", e))?;
    
    let items: Vec<QueuedDownload> = queue
        .queue
        .iter()
        .map(|dl| QueuedDownload {
            id: dl.id.clone(),
            url: dl.url.clone(),
            path: dl.path.clone(),
            filename: dl.path.split('\\').last().unwrap_or("Unknown").to_string(),
            priority: format!("{:?}", dl.priority),
            added_at: dl.added_at,
            custom_headers: dl.custom_headers.clone(),
            expected_checksum: dl.expected_checksum.clone(),
            retry_count: dl.retry_count,
            max_retries: dl.max_retries,
            retry_delay_ms: dl.retry_delay_ms,
            depends_on: dl.depends_on.clone(),
            custom_segments: dl.custom_segments,
            group: dl.group.clone(),
        })
        .collect();
    
    Ok(items)
}

/// Remove a download from the queue
#[command]
pub fn remove_from_queue(id: String) -> Result<(), String> {
    let mut queue = DOWNLOAD_QUEUE
        .lock()
        .map_err(|e| format!("Failed to acquire queue lock: {}", e))?;
    
    queue.queue.retain(|dl| dl.id != id);
    
    Ok(())
}

/// Set the priority of a queued download
#[command]
pub fn set_queue_priority(id: String, priority: String) -> Result<(), String> {
    let mut queue = DOWNLOAD_QUEUE
        .lock()
        .map_err(|e| format!("Failed to acquire queue lock: {}", e))?;
    
    let new_priority = match priority.to_lowercase().as_str() {
        "high" => DownloadPriority::High,
        "normal" => DownloadPriority::Normal,
        "low" => DownloadPriority::Low,
        _ => return Err(format!("Invalid priority: {}", priority)),
    };
    
    if let Some(dl) = queue.queue.iter_mut().find(|d| d.id == id) {
        dl.priority = new_priority;
        Ok(())
    } else {
        Err(format!("Download {} not found in queue", id))
    }
}

/// Move a queued download to the front of the queue
#[command]
pub fn move_queue_item_to_front(id: String) -> Result<(), String> {
    let mut queue = DOWNLOAD_QUEUE
        .lock()
        .map_err(|e| format!("Failed to acquire queue lock: {}", e))?;
    
    if let Some(pos) = queue.queue.iter().position(|dl| dl.id == id) {
        let item = queue.queue.remove(pos);
        queue.queue.push_front(item);
        Ok(())
    } else {
        Err(format!("Download {} not found in queue", id))
    }
}

/// Move a queued download towards the front (increase priority in order)
#[command]
pub fn move_queue_item_up(id: String) -> Result<(), String> {
    let mut queue = DOWNLOAD_QUEUE
        .lock()
        .map_err(|e| format!("Failed to acquire queue lock: {}", e))?;
    
    if let Some(pos) = queue.queue.iter().position(|dl| dl.id == id) {
        if pos > 0 {
            let item = queue.queue.remove(pos);
            queue.queue.insert(pos - 1, item);
        }
        Ok(())
    } else {
        Err(format!("Download {} not found in queue", id))
    }
}

/// Clear all downloads from the queue (doesn't affect active downloads)
#[command]
pub fn clear_download_queue() -> Result<(), String> {
    let mut queue = DOWNLOAD_QUEUE
        .lock()
        .map_err(|e| format!("Failed to acquire queue lock: {}", e))?;
    
    queue.queue.clear();
    
    Ok(())
}

/// Pause the queue (prevents new downloads from starting)
#[command]
pub fn pause_queue() -> Result<(), String> {
    let mut queue = DOWNLOAD_QUEUE
        .lock()
        .map_err(|e| format!("Failed to acquire queue lock: {}", e))?;
    
    queue.pause();
    
    Ok(())
}

/// Resume the queue (allows queued downloads to start)
#[command]
pub fn resume_queue() -> Result<(), String> {
    let mut queue = DOWNLOAD_QUEUE
        .lock()
        .map_err(|e| format!("Failed to acquire queue lock: {}", e))?;
    
    queue.resume();
    
    Ok(())
}

/// Get whether the queue is currently paused
#[command]
pub fn is_queue_paused() -> Result<bool, String> {
    let queue = DOWNLOAD_QUEUE
        .lock()
        .map_err(|e| format!("Failed to acquire queue lock: {}", e))?;
    
    Ok(queue.is_paused())
}

/// Set the maximum number of concurrent downloads
#[command]
pub fn set_max_concurrent_downloads(max: u32) -> Result<(), String> {
    if max == 0 || max > 64 {
        return Err(format!("Invalid max concurrent: {} (must be 1-64)", max));
    }
    
    let mut queue = DOWNLOAD_QUEUE
        .lock()
        .map_err(|e| format!("Failed to acquire queue lock: {}", e))?;
    
    queue.set_max_concurrent(max);
    
    Ok(())
}

/// Get the current maximum concurrent downloads setting
#[command]
pub fn get_max_concurrent_downloads() -> Result<u32, String> {
    let queue = DOWNLOAD_QUEUE
        .lock()
        .map_err(|e| format!("Failed to acquire queue lock: {}", e))?;
    
    Ok(queue.max_concurrent)
}

/// Queue statistics for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStats {
    pub total_queued: usize,
    pub total_active: usize,
    pub total_blocked: usize,
    pub max_concurrent: u32,
    pub paused: bool,
    pub groups_count: usize,
}

/// Get queue statistics
#[command]
pub fn get_queue_stats() -> Result<QueueStats, String> {
    let queue = DOWNLOAD_QUEUE
        .lock()
        .map_err(|e| format!("Failed to acquire queue lock: {}", e))?;
    
    Ok(QueueStats {
        total_queued: queue.queue.len(),
        total_active: queue.active_set.len(),
        total_blocked: queue.blocked_ids().len(),
        max_concurrent: queue.max_concurrent,
        paused: queue.is_paused(),
        groups_count: queue.groups().len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_queue_status_serialization() {
        let status = QueueStatus {
            max_concurrent: 4,
            active_count: 2,
            queued_count: 5,
            queued_items: vec![],
            active_ids: vec!["id1".to_string(), "id2".to_string()],
            paused: false,
            blocked_ids: vec![],
        };
        
        let json = serde_json::to_string(&status).expect("Should serialize");
        serde_json::from_str::<QueueStatus>(&json).expect("Should deserialize");
    }
}
