use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryRule {
    pub name: String,
    pub pattern: String,
    pub path: String, // Relative to download_dir or absolute
}

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
    /// Regex-based Category Rules
    #[serde(default)]
    pub category_rules: Vec<CategoryRule>,
    /// Enable DPI evasion (random padding)
    #[serde(default)]
    pub dpi_evasion: bool,
    /// Enable JA3/TLS fingerprint simulation
    #[serde(default)]
    pub ja3_enabled: bool,
    /// Enable Tor Network (All traffic via Tor)
    #[serde(default)]
    pub use_tor: bool,
    /// Minimum adaptive threads
    #[serde(default)]
    pub min_threads: u32,
    /// Maximum adaptive threads
    #[serde(default)]
    pub max_threads: u32,
    
    // Proxy Settings
    #[serde(default)]
    pub proxy_enabled: bool,
    #[serde(default)]
    pub proxy_type: String, // "http", "socks5"
    #[serde(default)]
    pub proxy_host: String,
    #[serde(default)]
    pub proxy_port: u16,
    #[serde(default)]
    pub proxy_username: Option<String>,
    #[serde(default)]
    pub proxy_password: Option<String>,

    // Cloud Settings
    #[serde(default)]
    pub cloud_enabled: bool,
    #[serde(default)]
    pub cloud_endpoint: Option<String>,
    #[serde(default)]
    pub cloud_bucket: Option<String>,
    #[serde(default)]
    pub cloud_region: Option<String>,
    #[serde(default)]
    pub cloud_access_key: Option<String>,
    #[serde(default)]
    pub cloud_secret_key: Option<String>,

    // Team Sync
    #[serde(default)]
    pub last_sync_host: Option<String>,
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
            category_rules: vec![
                CategoryRule { name: "Images".to_string(), pattern: r"(?i)\.(jpg|jpeg|png|gif|webp)$".to_string(), path: "Images".to_string() },
                CategoryRule { name: "Documents".to_string(), pattern: r"(?i)\.(pdf|doc|docx|txt)$".to_string(), path: "Documents".to_string() },
                CategoryRule { name: "Music".to_string(), pattern: r"(?i)\.(mp3|wav|flac|m4a|aac)$".to_string(), path: "Music".to_string() },
                CategoryRule { name: "Video".to_string(), pattern: r"(?i)\.(mp4|mkv|avi|mov|wmv|webm)$".to_string(), path: "Video".to_string() },
                CategoryRule { name: "Archives".to_string(), pattern: r"(?i)\.(zip|rar|7z|tar|gz|iso)$".to_string(), path: "Archives".to_string() },
                CategoryRule { name: "Programs".to_string(), pattern: r"(?i)\.(exe|msi|dmg|pkg)$".to_string(), path: "Programs".to_string() },
            ],
            dpi_evasion: false,
            ja3_enabled: false,
            use_tor: false,
            min_threads: 2,
            max_threads: 16,
            // Proxy Defaults
            proxy_enabled: false,
            proxy_type: "http".to_string(),
            proxy_host: "127.0.0.1".to_string(),
            proxy_port: 8080,
            proxy_username: None,
            proxy_password: None,
            // Cloud Defaults
            cloud_enabled: false,
            cloud_endpoint: None,
            cloud_bucket: None,
            cloud_region: Some("us-east-1".to_string()),
            cloud_access_key: None,

            cloud_secret_key: None,
            last_sync_host: None,
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
