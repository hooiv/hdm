use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Download directory path
    pub download_dir: String,
    /// Number of concurrent segments per download
    pub segments: u32,
    /// Speed limit in KB/s (0 = unlimited)
    pub speed_limit_kbps: u64,
    /// Enable clipboard monitoring
    pub clipboard_monitor: bool,
    /// Auto-start downloads from browser extension
    pub auto_start_extension: bool,
    /// Auto-sort downloads into category folders
    #[serde(default)]
    pub use_category_folders: bool,
}

impl Default for Settings {
    fn default() -> Self {
        // Get user's Desktop path
        let desktop = std::env::var("USERPROFILE")
            .map(|p| format!("{}\\Desktop", p))
            .unwrap_or_else(|_| "C:\\Downloads".to_string());
        
        Self {
            download_dir: desktop,
            segments: 8,
            speed_limit_kbps: 0, // Unlimited
            clipboard_monitor: false,
            auto_start_extension: true,
            use_category_folders: true,
        }
    }
}

fn get_settings_path() -> PathBuf {
    let home = std::env::var("USERPROFILE").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".hyperstream").join("settings.json")
}

pub fn load_settings() -> Settings {
    let path = get_settings_path();
    
    if !path.exists() {
        return Settings::default();
    }
    
    match fs::read_to_string(&path) {
        Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

pub fn save_settings(settings: &Settings) -> Result<(), String> {
    let path = get_settings_path();
    
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    
    let json = serde_json::to_string_pretty(settings)
        .map_err(|e| e.to_string())?;
    
    fs::write(&path, json).map_err(|e| e.to_string())?;
    
    println!("DEBUG: Settings saved to {:?}", path);
    Ok(())
}
