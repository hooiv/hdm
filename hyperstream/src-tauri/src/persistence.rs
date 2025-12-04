use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Represents a saved download that can be resumed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedDownload {
    pub id: String,
    pub url: String,
    pub path: String,
    pub filename: String,
    pub total_size: u64,
    pub downloaded_bytes: u64,
    pub status: String, // "Paused", "Error", "Done"
}

/// Get the path to the downloads.json file
fn get_storage_path() -> PathBuf {
    // Use a simple path in user's home directory for now
    let home = std::env::var("USERPROFILE").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".hyperstream").join("downloads.json")
}

/// Save downloads to disk
pub fn save_downloads(downloads: &[SavedDownload]) -> Result<(), String> {
    let path = get_storage_path();
    
    // Create directory if it doesn't exist
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create directory: {}", e))?;
    }
    
    let json = serde_json::to_string_pretty(downloads)
        .map_err(|e| format!("Failed to serialize: {}", e))?;
    
    fs::write(&path, json).map_err(|e| format!("Failed to write file: {}", e))?;
    
    println!("DEBUG: Saved {} downloads to {:?}", downloads.len(), path);
    Ok(())
}

/// Load downloads from disk
pub fn load_downloads() -> Result<Vec<SavedDownload>, String> {
    let path = get_storage_path();
    
    if !path.exists() {
        println!("DEBUG: No saved downloads file found at {:?}", path);
        return Ok(Vec::new());
    }
    
    let json = fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read file: {}", e))?;
    
    let downloads: Vec<SavedDownload> = serde_json::from_str(&json)
        .map_err(|e| format!("Failed to deserialize: {}", e))?;
    
    println!("DEBUG: Loaded {} downloads from {:?}", downloads.len(), path);
    Ok(downloads)
}

/// Add or update a download in the saved list
pub fn upsert_download(download: SavedDownload) -> Result<(), String> {
    let mut downloads = load_downloads().unwrap_or_default();
    
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
    let mut downloads = load_downloads().unwrap_or_default();
    downloads.retain(|d| d.id != id);
    save_downloads(&downloads)
}
