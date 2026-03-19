use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::time;
use tauri::Emitter;

static NETWORK_MONITOR_RUNNING: AtomicBool = AtomicBool::new(false);

/// Network status emitted to the frontend.
#[derive(Clone, serde::Serialize)]
pub struct NetworkStatus {
    pub online: bool,
    pub timestamp: i64,
}

/// Probe a set of reliable endpoints to determine if the network is up.
/// Uses HEAD requests with a short timeout for efficiency.
async fn check_network_connectivity() -> bool {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .no_proxy()
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    // Try multiple endpoints — we only need one to succeed.
    // Using well-known connectivity-check endpoints that return fast.
    let endpoints = [
        "http://www.gstatic.com/generate_204",
        "http://connectivitycheck.platform.hicloud.com/generate_204",
        "http://clients3.google.com/generate_204",
    ];

    for endpoint in &endpoints {
        match client.head(*endpoint).send().await {
            Ok(resp) => {
                if resp.status().is_success() || resp.status().as_u16() == 204 {
                    return true;
                }
            }
            Err(_) => continue,
        }
    }
    false
}

/// Start the network monitor background task.
/// This periodically checks connectivity and:
/// - Emits `network_status_changed` events on transitions
/// - When network comes back online after being offline, emits
///   `network_recovered` so the frontend can auto-resume paused downloads.
///
/// Must be called once during app setup.
pub fn start_network_monitor<R: tauri::Runtime + 'static>(app_handle: tauri::AppHandle<R>) {
    if NETWORK_MONITOR_RUNNING.swap(true, Ordering::SeqCst) {
        return; // already running
    }

    tauri::async_runtime::spawn(async move {
        let mut was_online = true; // assume online at startup
        let mut consecutive_failures: u32 = 0;

        loop {
            time::sleep(Duration::from_secs(10)).await;

            let is_online = check_network_connectivity().await;

            if is_online {
                if !was_online {
                    // NETWORK RECOVERED — transition from offline → online
                    println!("[NetworkMonitor] Network recovered after {} checks offline", consecutive_failures);

                    let _ = app_handle.emit("network_status_changed", NetworkStatus {
                        online: true,
                        timestamp: chrono::Utc::now().timestamp(),
                    });

                    // Emit recovery event — frontend should auto-resume downloads
                    let _ = app_handle.emit("network_recovered", serde_json::json!({
                        "timestamp": chrono::Utc::now().timestamp(),
                        "offline_duration_secs": consecutive_failures * 10,
                    }));

                    // Directly trigger auto-resume of errored/paused-by-network downloads
                    auto_resume_downloads(&app_handle).await;
                }
                consecutive_failures = 0;
                was_online = true;
            } else {
                consecutive_failures += 1;
                if was_online && consecutive_failures >= 2 {
                    // Require 2 consecutive failures before declaring offline
                    // (avoids false positives from a single dropped packet)
                    println!("[NetworkMonitor] Network appears offline");
                    let _ = app_handle.emit("network_status_changed", NetworkStatus {
                        online: false,
                        timestamp: chrono::Utc::now().timestamp(),
                    });
                    was_online = false;
                }
            }
        }
    });
}

/// When the network recovers, find downloads that errored or were paused
/// due to network issues and restart them automatically.
async fn auto_resume_downloads<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let settings = crate::settings::load_settings();
    if !settings.auto_resume_on_reconnect {
        return;
    }

    let downloads = match crate::persistence::load_downloads() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[NetworkMonitor] Failed to load downloads for auto-resume: {}", e);
            return;
        }
    };

    let resumable: Vec<_> = downloads.iter()
        .filter(|d| d.status == "Error" || d.status == "NetworkError")
        .filter(|d| d.downloaded_bytes > 0 && d.downloaded_bytes < d.total_size)
        .cloned()
        .collect();

    if resumable.is_empty() {
        return;
    }

    println!("[NetworkMonitor] Auto-resuming {} downloads after network recovery", resumable.len());

    for dl in resumable {
        let _ = app.emit("auto_resume_download", serde_json::json!({
            "id": dl.id,
            "url": dl.url,
            "path": dl.path,
        }));
    }
}
