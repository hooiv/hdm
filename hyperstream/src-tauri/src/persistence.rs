use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::downloader::structures::Segment;

/// Serialize all persistence read-modify-write operations to prevent data races
static PERSISTENCE_LOCK: std::sync::LazyLock<Mutex<()>> = std::sync::LazyLock::new(|| Mutex::new(()));

/// Represents a saved download that can be resumed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedDownload {
    pub id: String,
    pub url: String,
    pub path: String,
    pub filename: String,
    pub total_size: u64,
    pub downloaded_bytes: u64,
    pub status: String, // "Paused", "Error", "Done", "Downloading", "Interrupted"
    pub segments: Option<Vec<Segment>>, // Saved state of dynamic segments
    /// ISO 8601 timestamp of last activity (for crash staleness detection)
    #[serde(default)]
    pub last_active: Option<String>,
    /// Human-readable error message (when status is "Error")
    #[serde(default)]
    pub error_message: Option<String>,
    /// Optional checksum expectation preserved across pause/resume and completion.
    #[serde(default)]
    pub expected_checksum: Option<String>,
}

/// Persistent health and cooldown state for a mirror host.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MirrorHostHealth {
    #[serde(default)]
    pub success_count: u32,
    #[serde(default)]
    pub failure_count: u32,
    #[serde(default)]
    pub consecutive_failures: u32,
    #[serde(default)]
    pub quarantine_count: u32,
    #[serde(default)]
    pub cooldown_until: Option<String>,
    #[serde(default)]
    pub last_success_at: Option<String>,
    #[serde(default)]
    pub last_failure_at: Option<String>,
    #[serde(default)]
    pub last_error: Option<String>,
}

/// Get the path to the downloads.json file
fn get_storage_path() -> PathBuf {
    if let Some(config_dir) = dirs::config_dir() {
        return config_dir.join("hyperstream").join("downloads.json");
    }
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".hyperstream").join("downloads.json")
}

#[allow(dead_code)]
fn get_mirror_host_health_path() -> PathBuf {
    if let Some(config_dir) = dirs::config_dir() {
        return config_dir.join("hyperstream").join("mirror-host-health.json");
    }
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".hyperstream")
        .join("mirror-host-health.json")
}

fn write_json_atomically<T: Serialize + ?Sized>(path: &PathBuf, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    let json = serde_json::to_string_pretty(value)
        .map_err(|e| format!("Failed to serialize: {}", e))?;

    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, &json).map_err(|e| format!("Failed to write temp file: {}", e))?;

    // Rotate backups before overwriting: keep up to 3 generations
    // .bak3 ← .bak2 ← .bak1 ← current
    if path.exists() {
        rotate_backups(path);
    }

    if let Err(_rename_err) = fs::rename(&tmp_path, path) {
        fs::write(path, &json).map_err(|e| format!("Failed to write file: {}", e))?;
        let _ = fs::remove_file(&tmp_path);
    }

    Ok(())
}

/// Rotate backup files: .bak3 ← .bak2 ← .bak1 ← current
fn rotate_backups(path: &PathBuf) {
    let bak1 = path.with_extension("json.bak1");
    let bak2 = path.with_extension("json.bak2");
    let bak3 = path.with_extension("json.bak3");

    let _ = fs::remove_file(&bak3);
    if bak2.exists() { let _ = fs::rename(&bak2, &bak3); }
    if bak1.exists() { let _ = fs::rename(&bak1, &bak2); }
    // Copy (not move) current to bak1 so the primary file stays intact for the rename
    let _ = fs::copy(path, &bak1);
}

/// Save downloads to disk atomically (write to temp file, then rename)
pub fn save_downloads(downloads: &[SavedDownload]) -> Result<(), String> {
    let path = get_storage_path();

    write_json_atomically(&path, downloads)
}

/// Load downloads from disk. On corruption, automatically tries backup files.
pub fn load_downloads() -> Result<Vec<SavedDownload>, String> {
    let path = get_storage_path();
    
    match fs::read_to_string(&path) {
        Ok(json) => {
            match serde_json::from_str::<Vec<SavedDownload>>(&json) {
                Ok(downloads) => Ok(downloads),
                Err(e) => {
                    eprintln!("WARNING: downloads.json corrupted ({}), attempting backup recovery", e);
                    recover_from_backups(&path)
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => {
            eprintln!("WARNING: Could not read downloads.json ({}), attempting backup recovery", e);
            recover_from_backups(&path)
        }
    }
}

/// Try to load from backup files (.bak1, .bak2, .bak3) in order.
/// If a backup is valid, restore it as the primary file.
fn recover_from_backups(path: &PathBuf) -> Result<Vec<SavedDownload>, String> {
    for suffix in &["json.bak1", "json.bak2", "json.bak3"] {
        let backup = path.with_extension(suffix);
        if !backup.exists() {
            continue;
        }
        match fs::read_to_string(&backup) {
            Ok(json) => {
                match serde_json::from_str::<Vec<SavedDownload>>(&json) {
                    Ok(downloads) => {
                        eprintln!("RECOVERED: Restored {} downloads from {}", downloads.len(), suffix);
                        // Restore the backup as the primary file
                        let _ = fs::copy(&backup, path);
                        return Ok(downloads);
                    }
                    Err(_) => continue,
                }
            }
            Err(_) => continue,
        }
    }
    // All backups failed — start fresh rather than losing the app
    eprintln!("WARNING: All backup files corrupted or missing. Starting with empty download list.");
    Ok(Vec::new())
}

#[allow(dead_code)]
pub fn load_mirror_host_health() -> Result<HashMap<String, MirrorHostHealth>, String> {
    let path = get_mirror_host_health_path();

    match fs::read_to_string(&path) {
        Ok(json) => {
            let health: HashMap<String, MirrorHostHealth> = serde_json::from_str(&json)
                .map_err(|e| format!("Failed to deserialize: {}", e))?;
            Ok(health)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(HashMap::new()),
        Err(e) => Err(format!("Failed to read file: {}", e)),
    }
}

#[allow(dead_code)]
pub fn save_mirror_host_health(health: &HashMap<String, MirrorHostHealth>) -> Result<(), String> {
    let _lock = PERSISTENCE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = get_mirror_host_health_path();
    write_json_atomically(&path, health)
}

/// Add or update a download in the saved list
pub fn upsert_download(download: SavedDownload) -> Result<(), String> {
    let _lock = PERSISTENCE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut downloads = load_downloads().map_err(|e| {
        format!("Cannot upsert download: failed to load existing data: {}", e)
    })?;

    // Find and update, or insert new
    if let Some(existing) = downloads.iter_mut().find(|d| d.id == download.id) {
        *existing = download;
    } else {
        downloads.push(download);
    }

    save_downloads(&downloads)
}

/// Remove a download from the saved list
pub fn remove_download(id: &str) -> Result<(), String> {
    let _lock = PERSISTENCE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut downloads = load_downloads().map_err(|e| {
        format!("Cannot remove download: failed to load existing data: {}", e)
    })?;
    downloads.retain(|d| d.id != id);
    save_downloads(&downloads)
}
/// Move a download up or down in the list
pub fn move_download(id: &str, direction: &str) -> Result<(), String> {
    let _lock = PERSISTENCE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut downloads = match load_downloads() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("WARNING: Could not load downloads for move: {}", e);
            return Err(format!("Could not load download list: {}", e));
        }
    };
    
    if let Some(index) = downloads.iter().position(|d| d.id == id) {
        if direction == "up" && index > 0 {
            downloads.swap(index, index - 1);
        } else if direction == "down" && index < downloads.len() - 1 {
            downloads.swap(index, index + 1);
        } else {
            return Ok(()); // No move possible/needed
        }
        save_downloads(&downloads)
    } else {
        Err("Download not found".to_string())
    }
}
