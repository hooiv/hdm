// crash_recovery.rs — Detect and recover downloads interrupted by app crash
//
// When the app crashes or is killed while downloads are active, those downloads
// remain in downloads.json with status "Downloading". On next startup this module
// detects them, validates partial files, transitions them to "Interrupted", and
// optionally auto-resumes them.

use crate::persistence::{self, SavedDownload};
use serde::Serialize;
use std::path::Path;
use tauri::Emitter;

/// A download that was recovered after a crash
#[derive(Debug, Clone, Serialize)]
pub struct RecoveredDownload {
    pub id: String,
    pub filename: String,
    pub url: String,
    pub path: String,
    pub downloaded_bytes: u64,
    pub total_size: u64,
    pub has_segments: bool,
    pub file_exists: bool,
    pub file_size_on_disk: u64,
    pub last_active: Option<String>,
}

/// A download whose partial file was lost or corrupted
#[derive(Debug, Clone, Serialize)]
pub struct CorruptedDownload {
    pub id: String,
    pub filename: String,
    pub reason: String,
}

/// Result of the crash recovery scan
#[derive(Debug, Clone, Serialize)]
pub struct RecoveryReport {
    pub recovered: Vec<RecoveredDownload>,
    pub corrupted: Vec<CorruptedDownload>,
}

/// Scan for downloads that were interrupted by a crash (status == "Downloading")
/// and transition them to "Interrupted" status so they can be resumed.
///
/// Also detects downloads whose partial file has gone missing and marks them as "Error".
pub fn scan_and_recover() -> Result<RecoveryReport, String> {
    let downloads = persistence::load_downloads()?;
    let mut recovered = Vec::new();
    let mut corrupted = Vec::new();
    let mut updated = downloads.clone();
    let mut changed = false;

    for dl in &downloads {
        if dl.status != "Downloading" {
            continue;
        }

        let file_path = Path::new(&dl.path);
        let file_exists = file_path.exists();
        let file_size = if file_exists {
            file_path.metadata().map(|m| m.len()).unwrap_or(0)
        } else {
            0
        };

        // If the file is gone but we had progress, mark as error
        if !file_exists && dl.downloaded_bytes > 0 {
            corrupted.push(CorruptedDownload {
                id: dl.id.clone(),
                filename: dl.filename.clone(),
                reason: "Partial file missing from disk".to_string(),
            });
            if let Some(entry) = updated.iter_mut().find(|d| d.id == dl.id) {
                entry.status = "Error".to_string();
                entry.error_message = Some("Partial file lost after crash".to_string());
                changed = true;
            }
            continue;
        }

        // If the file exists but is smaller than the saved segment cursors suggest,
        // the last disk flush may have been lost. Adjust downloaded_bytes to match
        // what's actually on disk (conservative recovery).
        let actual_bytes = if file_exists && dl.total_size > 0 {
            // For sparse files, file_size == total_size even if not all written.
            // Trust saved segment data over file metadata when segments exist.
            if dl.segments.is_some() {
                dl.downloaded_bytes
            } else {
                file_size.min(dl.downloaded_bytes)
            }
        } else {
            dl.downloaded_bytes
        };

        recovered.push(RecoveredDownload {
            id: dl.id.clone(),
            filename: dl.filename.clone(),
            url: dl.url.clone(),
            path: dl.path.clone(),
            downloaded_bytes: actual_bytes,
            total_size: dl.total_size,
            has_segments: dl.segments.is_some(),
            file_exists,
            file_size_on_disk: file_size,
            last_active: dl.last_active.clone(),
        });

        if let Some(entry) = updated.iter_mut().find(|d| d.id == dl.id) {
            entry.status = "Interrupted".to_string();
            entry.downloaded_bytes = actual_bytes;
            changed = true;
        }
    }

    if changed {
        persistence::save_downloads(&updated)?;
    }

    Ok(RecoveryReport { recovered, corrupted })
}

/// Auto-resume all interrupted downloads by emitting `auto_resume_download` events.
/// Only runs if the `auto_resume_after_crash` setting is enabled.
pub fn auto_resume_interrupted<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let settings = crate::settings::load_settings();
    if !settings.auto_resume_after_crash {
        return;
    }

    let downloads = match persistence::load_downloads() {
        Ok(d) => d,
        Err(_) => return,
    };

    let interrupted: Vec<_> = downloads
        .iter()
        .filter(|d| d.status == "Interrupted")
        .cloned()
        .collect();

    if interrupted.is_empty() {
        return;
    }

    println!(
        "[CrashRecovery] Auto-resuming {} interrupted downloads",
        interrupted.len()
    );

    for dl in &interrupted {
        let _ = app.emit(
            "auto_resume_download",
            serde_json::json!({
                "id": dl.id,
                "url": dl.url,
                "path": dl.path,
            }),
        );
    }
}

/// Run crash recovery on app startup.
///
/// 1. Scans for "Downloading" status entries and transitions them to "Interrupted"
/// 2. Emits a `crash_recovery` event so the frontend can display a notification
/// 3. If `auto_resume_after_crash` is enabled, emits resume events for each
pub fn recover_on_startup<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    match scan_and_recover() {
        Ok(report) => {
            let recovered_count = report.recovered.len();
            let corrupted_count = report.corrupted.len();

            if recovered_count > 0 || corrupted_count > 0 {
                println!(
                    "[CrashRecovery] Startup scan: {} interrupted, {} corrupted",
                    recovered_count, corrupted_count
                );

                let _ = app.emit(
                    "crash_recovery",
                    serde_json::json!({
                        "recovered_count": recovered_count,
                        "corrupted_count": corrupted_count,
                        "recovered": report.recovered,
                        "corrupted": report.corrupted,
                    }),
                );
            }

            auto_resume_interrupted(app);
        }
        Err(e) => {
            eprintln!("[CrashRecovery] Startup scan failed: {}", e);
        }
    }
}

/// Get all currently interrupted downloads (for UI display)
pub fn get_interrupted() -> Result<Vec<SavedDownload>, String> {
    let downloads = persistence::load_downloads()?;
    Ok(downloads
        .into_iter()
        .filter(|d| d.status == "Interrupted")
        .collect())
}

/// Resume a specific interrupted download by re-emitting the resume event
pub fn resume_one<R: tauri::Runtime>(app: &tauri::AppHandle<R>, id: &str) -> Result<(), String> {
    let downloads = persistence::load_downloads()?;
    let dl = downloads
        .iter()
        .find(|d| d.id == id)
        .ok_or_else(|| "Download not found".to_string())?;

    if dl.status != "Interrupted" && dl.status != "Paused" && dl.status != "Error" {
        return Err(format!("Download status is '{}', cannot resume", dl.status));
    }

    let _ = app.emit(
        "auto_resume_download",
        serde_json::json!({
            "id": dl.id,
            "url": dl.url,
            "path": dl.path,
        }),
    );

    Ok(())
}
