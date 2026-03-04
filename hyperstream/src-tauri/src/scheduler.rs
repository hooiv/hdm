use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Emitter;
use chrono::{DateTime, Local, Timelike};

lazy_static::lazy_static! {
    static ref SCHEDULED_DOWNLOADS: Mutex<HashMap<String, ScheduledDownload>> = Mutex::new(HashMap::new());
    /// Set to true to signal the scheduler thread to stop.
    static ref SCHEDULER_STOP: AtomicBool = AtomicBool::new(false);
}

/// Tracks whether the scheduler thread is already running to prevent duplicates.
static SCHEDULER_RUNNING: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ScheduledDownload {
    pub id: String,
    pub url: String,
    pub filename: String,
    pub scheduled_time: String, // ISO 8601 format
    pub status: String, // "pending", "started", "completed", "cancelled"
}

pub fn add_scheduled_download(download: ScheduledDownload) {
    let mut scheduled = SCHEDULED_DOWNLOADS.lock().unwrap_or_else(|e| e.into_inner());
    scheduled.insert(download.id.clone(), download);
}

pub fn remove_scheduled_download(id: &str) {
    let mut scheduled = SCHEDULED_DOWNLOADS.lock().unwrap_or_else(|e| e.into_inner());
    scheduled.remove(id);
}

pub fn force_start_download(id: &str) -> Option<ScheduledDownload> {
    let mut scheduled = SCHEDULED_DOWNLOADS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(download) = scheduled.get_mut(id) {
        if download.status == "pending" {
            download.status = "started".to_string();
            return Some(download.clone());
        }
    }
    None
}

pub fn get_scheduled_downloads() -> Vec<ScheduledDownload> {
    let scheduled = SCHEDULED_DOWNLOADS.lock().unwrap_or_else(|e| e.into_inner());
    scheduled.values().cloned().collect()
}

pub fn check_scheduled_downloads<R: tauri::Runtime>(app_handle: &tauri::AppHandle<R>) {
    let now = Local::now();
    let mut to_start = Vec::new();
    
    {
        let mut scheduled = SCHEDULED_DOWNLOADS.lock().unwrap_or_else(|e| e.into_inner());
        
        for (_id, download) in scheduled.iter_mut() {
            if download.status != "pending" {
                continue;
            }
            
            if let Ok(scheduled_time) = DateTime::parse_from_rfc3339(&download.scheduled_time) {
                let scheduled_local = scheduled_time.with_timezone(&Local);
                
                if now >= scheduled_local {
                    download.status = "started".to_string();
                    to_start.push(download.clone());
                }
            }
        }
    }
    
    // Emit events for downloads that should start
    for download in to_start {
        let _ = app_handle.emit("scheduled_download_start", serde_json::json!({
            "id": download.id,
            "url": download.url,
            "filename": download.filename
        }));
    }
}

pub fn start_scheduler<R: tauri::Runtime + 'static>(app_handle: tauri::AppHandle<R>) {
    // Prevent multiple scheduler threads from being spawned
    if SCHEDULER_RUNNING.swap(true, Ordering::SeqCst) {
        println!("Scheduler thread already running, skipping duplicate spawn.");
        return;
    }
    SCHEDULER_STOP.store(false, Ordering::SeqCst);
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(30)); // Check every 30 seconds
            if SCHEDULER_STOP.load(Ordering::SeqCst) {
                println!("Scheduler thread stopping.");
                break;
            }
            check_scheduled_downloads(&app_handle);
        }
        SCHEDULER_RUNNING.store(false, Ordering::SeqCst);
    });
}

/// Stop the scheduler thread gracefully.
#[allow(dead_code)]
pub fn stop_scheduler() {
    SCHEDULER_STOP.store(true, Ordering::SeqCst);
}

/// Check if current time is within quiet hours (using Timelike trait)
/// Quiet hours: 11 PM to 7 AM by default
pub fn is_quiet_hours() -> bool {
    let now = Local::now();
    let hour = now.hour(); // Uses Timelike trait
    hour >= 23 || hour < 7
}

/// Get the next optimal download time (outside quiet hours)
pub fn get_next_download_time() -> DateTime<Local> {
    let now = Local::now();
    let hour = now.hour();
    
    // If within quiet hours, schedule for 7 AM
    if hour >= 23 || hour < 7 {
        let next_day = if hour >= 23 {
            now + chrono::Duration::days(1)
        } else {
            now
        };
        // Set to 7 AM
        next_day
            .with_hour(7)
            .and_then(|t| t.with_minute(0))
            .and_then(|t| t.with_second(0))
            .unwrap_or(now)
    } else {
        now
    }
}

/// Get formatted time info for scheduling UI
#[derive(Clone, Serialize)]
pub struct TimeInfo {
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub is_quiet_hours: bool,
}

pub fn get_current_time_info() -> TimeInfo {
    let now = Local::now();
    TimeInfo {
        hour: now.hour(),
        minute: now.minute(),
        second: now.second(),
        is_quiet_hours: is_quiet_hours(),
    }
}
