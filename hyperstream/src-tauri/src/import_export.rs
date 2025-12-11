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
    let json = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

pub fn load_export_from_file(path: &str) -> Result<ExportData, String> {
    let json = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}
