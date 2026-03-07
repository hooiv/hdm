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
    /// Tracks whether we are currently applying the quiet hours throttle to restore the user limit later.
    static ref QUIET_THROTTLE_ACTIVE: AtomicBool = AtomicBool::new(false);
}

/// Tracks whether the scheduler thread is already running to prevent duplicates.
static SCHEDULER_RUNNING: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ScheduledDownload {
    pub id: String,
    pub url: String,
    pub filename: String,
    pub scheduled_time: String, // ISO 8601 format
    pub stop_time: Option<String>,
    pub end_action: Option<String>,
    pub status: String, // "pending", "started", "completed", "cancelled", "stopped"
}

fn get_store_path() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
        .join("hyperstream")
        .join("scheduled.json")
}

fn save_to_disk(scheduled: &HashMap<String, ScheduledDownload>) {
    let pending: Vec<_> = scheduled.values().filter(|d| d.status == "pending" || d.status == "started").collect();
    if let Ok(data) = serde_json::to_string_pretty(&pending) {
        let path = get_store_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, data);
    }
}

fn load_from_disk() {
    let path = get_store_path();
    if let Ok(data) = std::fs::read_to_string(&path) {
        if let Ok(items) = serde_json::from_str::<Vec<ScheduledDownload>>(&data) {
            let mut scheduled = SCHEDULED_DOWNLOADS.lock().unwrap_or_else(|e| e.into_inner());
            for mut item in items {
                if item.status == "pending" || item.status == "started" {
                    // After restart, "started" items are orphaned (no running download).
                    // Treat as pending so they can be re-triggered on next scheduler tick.
                    if item.status == "started" {
                        item.status = "pending".to_string();
                    }
                    scheduled.insert(item.id.clone(), item);
                }
            }
        }
    }
}

pub fn add_scheduled_download(download: ScheduledDownload) {
    let mut scheduled = SCHEDULED_DOWNLOADS.lock().unwrap_or_else(|e| e.into_inner());
    scheduled.insert(download.id.clone(), download);
    save_to_disk(&scheduled);
}

pub fn remove_scheduled_download(id: &str) {
    let mut scheduled = SCHEDULED_DOWNLOADS.lock().unwrap_or_else(|e| e.into_inner());
    scheduled.remove(id);
    save_to_disk(&scheduled);
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

pub fn handle_download_complete(id: &str) {
    let end_action = {
        let mut scheduled = SCHEDULED_DOWNLOADS.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(download) = scheduled.get_mut(id) {
            download.status = "completed".to_string();
            download.end_action.clone()
        } else {
            None
        }
    };
    
    if let Some(action) = end_action {
        match action.to_lowercase().as_str() {
            "exit" => {
                println!("[Scheduler] End action triggered: exit");
                std::process::exit(0);
            },
            "sleep" => {
                println!("[Scheduler] End action triggered: sleep");
                #[cfg(target_os = "windows")]
                {
                    let _ = std::process::Command::new("rundll32.exe")
                        .args(["powrprof.dll,SetSuspendState", "0,1,0"])
                        .spawn();
                }
            },
            "shutdown" => {
                println!("[Scheduler] End action triggered: shutdown");
                #[cfg(target_os = "windows")]
                {
                    let _ = std::process::Command::new("shutdown")
                        .args(["/s", "/t", "0"])
                        .spawn();
                }
            },
            _ => {}
        }
    }
}

pub fn check_scheduled_downloads<R: tauri::Runtime>(app_handle: &tauri::AppHandle<R>) {
    let now = Local::now();
    let quiet = is_quiet_hours();
    let settings = crate::settings::load_settings();
    let defer_mode = settings.quiet_hours_action != "throttle";
    let mut to_start = Vec::new();
    
    {
        let mut scheduled = SCHEDULED_DOWNLOADS.lock().unwrap_or_else(|e| e.into_inner());
        
        for (_id, download) in scheduled.iter_mut() {
            if download.status == "started" {
                if let Some(stop_time_str) = &download.stop_time {
                    if let Ok(stop_time) = DateTime::parse_from_rfc3339(stop_time_str) {
                        let stop_local = stop_time.with_timezone(&Local);
                        if now >= stop_local {
                            println!("[Scheduler] Stop time reached for {}", download.id);
                            download.status = "stopped".to_string();
                            
                            let dl_id = download.id.clone();
                            let app_clone = app_handle.clone();
                            tokio::spawn(async move {
                                use tauri::Manager;
                                let state = app_clone.state::<crate::core_state::AppState>();
                                let _ = crate::pause_download(dl_id, state).await;
                            });
                        }
                    }
                }
                continue;
            }

            if download.status != "pending" {
                continue;
            }
            
            if let Ok(scheduled_time) = DateTime::parse_from_rfc3339(&download.scheduled_time) {
                let scheduled_local = scheduled_time.with_timezone(&Local);
                
                if now >= scheduled_local {
                    if quiet && defer_mode {
                        // Quiet hours with defer — skip starting, will retry next loop
                        continue;
                    }
                    download.status = "started".to_string();
                    to_start.push(download.clone());
                }
            }
        }
    }
    
    // Emit events for downloads that should start, then remove them from the map
    for download in &to_start {
        let _ = app_handle.emit("scheduled_download_start", serde_json::json!({
            "id": download.id,
            "url": download.url,
            "filename": download.filename
        }));
    }

    // If quiet hours are active with defer mode and there are deferred items, notify frontend
    if quiet && defer_mode {
        let scheduled = SCHEDULED_DOWNLOADS.lock().unwrap_or_else(|e| e.into_inner());
        let deferred_count = scheduled.values()
            .filter(|d| d.status == "pending")
            .filter(|d| {
                d.scheduled_time.parse::<DateTime<chrono::FixedOffset>>()
                    .map(|t| now >= t.with_timezone(&Local))
                    .unwrap_or(false)
            })
            .count();
        if deferred_count > 0 {
            let next = get_next_download_time();
            let _ = app_handle.emit("quiet_hours_deferred", serde_json::json!({
                "deferred_count": deferred_count,
                "resume_at": next.to_rfc3339()
            }));
        }
    }

    // Apply quiet hours throttle if in throttle mode
    if quiet && !defer_mode && settings.quiet_hours_throttle_kbps > 0 {
        if !QUIET_THROTTLE_ACTIVE.swap(true, Ordering::SeqCst) {
            // Transitioning into quiet-hours throttle
            crate::speed_limiter::GLOBAL_LIMITER.set_limit(settings.quiet_hours_throttle_kbps * 1024);
        }
    } else if QUIET_THROTTLE_ACTIVE.swap(false, Ordering::SeqCst) {
        // Transitioning out of quiet hours — restore user's configured speed limit
        crate::speed_limiter::GLOBAL_LIMITER.set_limit(settings.speed_limit_kbps * 1024);
    }

    // Purge non-pending entries to prevent unbounded accumulation of dead entries
    {
        let mut scheduled = SCHEDULED_DOWNLOADS.lock().unwrap_or_else(|e| e.into_inner());
        scheduled.retain(|_, d| d.status == "pending");
        save_to_disk(&scheduled);
    }
}

pub fn start_scheduler<R: tauri::Runtime + 'static>(app_handle: tauri::AppHandle<R>) {
    // Prevent multiple scheduler threads from being spawned
    if SCHEDULER_RUNNING.swap(true, Ordering::SeqCst) {
        println!("Scheduler thread already running, skipping duplicate spawn.");
        return;
    }
    SCHEDULER_STOP.store(false, Ordering::SeqCst);

    // Restore scheduled downloads from disk before entering the loop
    load_from_disk();

    std::thread::spawn(move || {
        // Guard ensures SCHEDULER_RUNNING is reset even if the thread panics
        struct SchedulerGuard;
        impl Drop for SchedulerGuard {
            fn drop(&mut self) {
                SCHEDULER_RUNNING.store(false, Ordering::SeqCst);
            }
        }
        let _guard = SchedulerGuard;

        // Check immediately on startup, then every 30 seconds
        check_scheduled_downloads(&app_handle);

        loop {
            std::thread::sleep(std::time::Duration::from_secs(30));
            if SCHEDULER_STOP.load(Ordering::SeqCst) {
                println!("Scheduler thread stopping.");
                break;
            }
            check_scheduled_downloads(&app_handle);
        }
    });
}

/// Stop the scheduler thread gracefully.
#[allow(dead_code)]
pub fn stop_scheduler() {
    SCHEDULER_STOP.store(true, Ordering::SeqCst);
}

/// Check if current time is within the configured quiet hours window.
/// Returns false when quiet hours are disabled in settings.
pub fn is_quiet_hours() -> bool {
    let settings = crate::settings::load_settings();
    if !settings.quiet_hours_enabled {
        return false;
    }
    let now = Local::now();
    let hour = now.hour();
    let start = settings.quiet_hours_start;
    let end = settings.quiet_hours_end;
    if start == end {
        return false; // zero-length window
    }
    if start < end {
        // e.g. 9..17 — simple range
        hour >= start && hour < end
    } else {
        // wraps midnight, e.g. 23..7
        hour >= start || hour < end
    }
}

/// Get the next time outside quiet hours, respecting configured window.
pub fn get_next_download_time() -> DateTime<Local> {
    let settings = crate::settings::load_settings();
    let now = Local::now();
    if !settings.quiet_hours_enabled || !is_quiet_hours() {
        return now;
    }
    let end = settings.quiet_hours_end;
    let hour = now.hour();
    let need_next_day = if settings.quiet_hours_start < settings.quiet_hours_end {
        false // same-day window, end is later today
    } else {
        // wraps midnight: if hour >= start we need to go to next day's end
        hour >= settings.quiet_hours_start
    };
    let base = if need_next_day {
        now + chrono::Duration::days(1)
    } else {
        now
    };
    base.with_hour(end)
        .and_then(|t| t.with_minute(0))
        .and_then(|t| t.with_second(0))
        .unwrap_or(now)
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
