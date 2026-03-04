use serde::{Serialize, Deserialize};
use crate::settings::Settings;
use crate::persistence::SavedDownload;
use std::fs;
// use std::path::Path;

#[derive(Serialize, Deserialize)]
pub struct ExportData {
    pub version: String,
    pub timestamp: u64,
    pub settings: Settings,
    pub downloads: Vec<SavedDownload>,
    // Could add schedules, regex rules (part of settings), etc.
}

pub fn save_export_to_file(data: &ExportData, path: &str) -> Result<(), String> {
    // Validate export path is not inside system directories
    let export_path = std::path::Path::new(path);
    let canon = dunce::canonicalize(export_path.parent().unwrap_or(export_path))
        .map_err(|e| format!("Cannot resolve export path: {}", e))?;
    // Block writes to system-critical directories
    let canon_str = canon.to_string_lossy().to_lowercase();
    if canon_str.contains("\\windows\\system32") || canon_str.contains("/etc/") || canon_str.contains("/usr/") {
        return Err("Cannot export to system directories".to_string());
    }

    let json = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

pub fn load_export_from_file(path: &str) -> Result<ExportData, String> {
    // Cap import file size to 50 MB to prevent OOM
    let metadata = fs::metadata(path).map_err(|e| e.to_string())?;
    if metadata.len() > 50 * 1024 * 1024 {
        return Err(format!("Import file too large: {} bytes (max 50 MB)", metadata.len()));
    }
    let json = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}
