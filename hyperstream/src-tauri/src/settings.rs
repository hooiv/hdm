use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::collections::{HashMap, HashSet};

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

    // ChatOps (Telegram)
    #[serde(default)]
    pub telegram_bot_token: Option<String>,
    #[serde(default)]
    pub telegram_chat_id: Option<String>,
    #[serde(default)]
    pub chatops_enabled: bool,
    
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
    
    // Archive Extraction
    #[serde(default)]
    pub auto_extract_archives: bool,
    #[serde(default)]
    pub cleanup_archives_after_extract: bool,
    
    // P2P File Sharing
    #[serde(default)]
    pub p2p_enabled: bool,
    #[serde(default)]
    pub p2p_upload_limit_kbps: Option<u64>,
    
    // Webhooks
    #[serde(default)]
    pub webhooks: Option<Vec<crate::webhooks::WebhookConfig>>,

    // Custom Sound Files (Z1)
    #[serde(default)]
    pub custom_sound_start: Option<String>,
    #[serde(default)]
    pub custom_sound_complete: Option<String>,
    #[serde(default)]
    pub custom_sound_error: Option<String>,

    // Metadata Scrubber
    #[serde(default)]
    pub auto_scrub_metadata: bool,

    // VPN Auto-Connect
    #[serde(default)]
    pub vpn_auto_connect: bool,
    #[serde(default)]
    pub vpn_connection_name: Option<String>,

    // MQTT Smart Home
    #[serde(default)]
    pub mqtt_enabled: bool,
    #[serde(default)]
    pub mqtt_broker_url: String,
    #[serde(default)]
    pub mqtt_topic: String,

    // Smart Sleep (Power Management)
    #[serde(default)]
    pub prevent_sleep_during_download: bool,
    #[serde(default)]
    pub pause_on_low_battery: bool,

    // Torrent queue management
    #[serde(default = "default_torrent_max_active_downloads")]
    pub torrent_max_active_downloads: u32,
    #[serde(default = "default_true")]
    pub torrent_auto_manage_queue: bool,

    // Torrent seeding policy
    #[serde(default = "default_true")]
    pub torrent_auto_stop_seeding: bool,
    #[serde(default = "default_torrent_seed_ratio_limit")]
    pub torrent_seed_ratio_limit: f64,
    #[serde(default = "default_torrent_seed_time_limit_mins")]
    pub torrent_seed_time_limit_mins: u32,
    #[serde(default)]
    pub torrent_priority_overrides: HashMap<String, String>,
    #[serde(default)]
    pub torrent_pinned_hashes: HashSet<String>,

    // Download Queue Management
    #[serde(default = "default_max_concurrent_downloads")]
    pub max_concurrent_downloads: u32,

    // Network Recovery
    #[serde(default = "default_true")]
    pub auto_resume_on_reconnect: bool,

    // Crash Recovery
    #[serde(default = "default_true")]
    pub auto_resume_after_crash: bool,

    // Auto-sort downloads into category folders
    #[serde(default = "default_true")]
    pub auto_sort_downloads: bool,

    // Scan downloads with system antivirus after completion
    #[serde(default)]
    pub scan_after_download: bool,

    // Integrity Verification
    #[serde(default)]
    pub auto_checksum_verify: bool,

    // Speed Profiles (time-based bandwidth scheduling)
    #[serde(default)]
    pub speed_profiles: Vec<SpeedProfile>,
    #[serde(default)]
    pub speed_profiles_enabled: bool,

    // Quiet Hours — defer new/scheduled downloads during specified window
    #[serde(default)]
    pub quiet_hours_enabled: bool,
    #[serde(default = "default_quiet_hours_start")]
    pub quiet_hours_start: u32, // 0-23, hour of day
    #[serde(default = "default_quiet_hours_end")]
    pub quiet_hours_end: u32,   // 0-23, hour of day
    /// "defer" = don't start new downloads, "throttle" = apply speed_limit during window
    #[serde(default = "default_quiet_hours_action")]
    pub quiet_hours_action: String,
    /// Speed limit KB/s when quiet hours action is "throttle" (0 = 50 KB/s minimum)
    #[serde(default = "default_quiet_hours_throttle_kbps")]
    pub quiet_hours_throttle_kbps: u64,
}

/// A time-based speed limit profile (e.g. "limit to 500 KB/s from 9:00 to 17:00")
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeedProfile {
    /// Human-readable name (e.g. "Work Hours")
    pub name: String,
    /// Start time as "HH:MM" (24h format)
    pub start_time: String,
    /// End time as "HH:MM" (24h format)
    pub end_time: String,
    /// Speed limit in KB/s (0 = unlimited)
    pub speed_limit_kbps: u64,
    /// Days of the week this profile applies (0=Mon, 6=Sun). Empty = every day.
    #[serde(default)]
    pub days: Vec<u8>,
}

fn default_max_concurrent_downloads() -> u32 {
    5
}

fn default_torrent_max_active_downloads() -> u32 {
    4
}

fn default_true() -> bool {
    true
}

fn default_torrent_seed_ratio_limit() -> f64 {
    1.5
}

fn default_torrent_seed_time_limit_mins() -> u32 {
    180
}

fn default_quiet_hours_start() -> u32 {
    23
}

fn default_quiet_hours_end() -> u32 {
    7
}

fn default_quiet_hours_action() -> String {
    "defer".to_string()
}

fn default_quiet_hours_throttle_kbps() -> u64 {
    50
}

impl Default for Settings {
    fn default() -> Self {
        // Get user's Downloads path cross-platform
        let download_dir = dirs::download_dir()
            .or_else(|| dirs::desktop_dir())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| {
                std::env::var("HOME")
                    .or_else(|_| std::env::var("USERPROFILE"))
                    .map(|p| format!("{}/Downloads", p))
                    .unwrap_or_else(|_| "Downloads".to_string())
            });
        
        Self {
            download_dir: download_dir,
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
            auto_extract_archives: false,
            cleanup_archives_after_extract: false,
            p2p_enabled: false,
            p2p_upload_limit_kbps: None,
            webhooks: None,
            // Custom Sound Files
            custom_sound_start: None,
            custom_sound_complete: None,
            custom_sound_error: None,
            // Metadata Scrubber
            auto_scrub_metadata: false,
            // ChatOps defaults
            telegram_bot_token: None,
            telegram_chat_id: None,
            chatops_enabled: false,
            // VPN Defaults
            vpn_auto_connect: false,
            vpn_connection_name: None,
            // MQTT Defaults
            mqtt_enabled: false,
            mqtt_broker_url: "mqtt://localhost:1883".to_string(),
            mqtt_topic: "hyperstream/downloads".to_string(),
            prevent_sleep_during_download: true,
            pause_on_low_battery: true,
            torrent_max_active_downloads: default_torrent_max_active_downloads(),
            torrent_auto_manage_queue: true,
            torrent_auto_stop_seeding: true,
            torrent_seed_ratio_limit: default_torrent_seed_ratio_limit(),
            torrent_seed_time_limit_mins: default_torrent_seed_time_limit_mins(),
            torrent_priority_overrides: HashMap::new(),
            torrent_pinned_hashes: HashSet::new(),
            max_concurrent_downloads: default_max_concurrent_downloads(),
            auto_resume_on_reconnect: true,
            auto_resume_after_crash: true,
            auto_sort_downloads: true,
            scan_after_download: false,
            auto_checksum_verify: false,
            speed_profiles: Vec::new(),
            speed_profiles_enabled: false,
            quiet_hours_enabled: false,
            quiet_hours_start: default_quiet_hours_start(),
            quiet_hours_end: default_quiet_hours_end(),
            quiet_hours_action: default_quiet_hours_action(),
            quiet_hours_throttle_kbps: default_quiet_hours_throttle_kbps(),
        }
    }
}

pub fn normalize_torrent_priority_label(priority: &str) -> Option<&'static str> {
    match priority.trim().to_ascii_lowercase().as_str() {
        "high" => Some("high"),
        "normal" => Some("normal"),
        "low" => Some("low"),
        _ => None,
    }
}

fn is_valid_info_hash(hash: &str) -> bool {
    hash.len() == 40 && hash.bytes().all(|b| b.is_ascii_hexdigit())
}

fn get_settings_path() -> PathBuf {
    if let Some(config_dir) = dirs::config_dir() {
        return config_dir.join("hyperstream").join("settings.json");
    }
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".hyperstream").join("settings.json")
}

pub fn load_settings() -> Settings {
    let path = get_settings_path();
    
    let mut settings = match fs::read_to_string(&path) {
        Ok(json) => match serde_json::from_str(&json) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("WARNING: Settings file corrupted, using defaults: {}", e);
                Settings::default()
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::default(),
        Err(e) => {
            eprintln!("WARNING: Could not read settings file, using defaults: {}", e);
            Settings::default()
        }
    };

    // Defensive clamping: protect against manually edited settings with invalid values
    if settings.segments == 0 || settings.segments > 64 {
        settings.segments = 8;
    }
    if settings.torrent_max_active_downloads > 64 {
        settings.torrent_max_active_downloads = default_torrent_max_active_downloads();
    }
    if !settings.torrent_seed_ratio_limit.is_finite()
        || settings.torrent_seed_ratio_limit < 0.0
        || settings.torrent_seed_ratio_limit > 20.0
    {
        settings.torrent_seed_ratio_limit = default_torrent_seed_ratio_limit();
    }
    if settings.torrent_seed_time_limit_mins > 10_080 {
        settings.torrent_seed_time_limit_mins = default_torrent_seed_time_limit_mins();
    }
    let mut normalized_priorities = HashMap::new();
    for (hash, priority) in std::mem::take(&mut settings.torrent_priority_overrides) {
        if !is_valid_info_hash(&hash) {
            continue;
        }
        let Some(normalized) = normalize_torrent_priority_label(&priority) else {
            continue;
        };
        if normalized == "normal" {
            continue;
        }
        normalized_priorities.insert(hash.to_ascii_lowercase(), normalized.to_string());
    }
    settings.torrent_priority_overrides = normalized_priorities;

    let mut normalized_pins = HashSet::new();
    for hash in std::mem::take(&mut settings.torrent_pinned_hashes) {
        if !is_valid_info_hash(&hash) {
            continue;
        }
        normalized_pins.insert(hash.to_ascii_lowercase());
    }
    settings.torrent_pinned_hashes = normalized_pins;

    settings
}

pub fn save_settings(settings: &Settings) -> Result<(), String> {
    // Validate critical numeric settings
    if settings.segments == 0 || settings.segments > 64 {
        return Err("Segments must be between 1 and 64".to_string());
    }
    if settings.min_threads > 0 && settings.max_threads > 0 && settings.min_threads > settings.max_threads {
        return Err("min_threads cannot exceed max_threads".to_string());
    }
    if settings.torrent_max_active_downloads > 64 {
        return Err("torrent_max_active_downloads must be between 0 and 64".to_string());
    }
    if !settings.torrent_seed_ratio_limit.is_finite()
        || settings.torrent_seed_ratio_limit < 0.0
        || settings.torrent_seed_ratio_limit > 20.0
    {
        return Err("torrent_seed_ratio_limit must be between 0.0 and 20.0".to_string());
    }
    if settings.torrent_seed_time_limit_mins > 10_080 {
        return Err("torrent_seed_time_limit_mins must be between 0 and 10080".to_string());
    }
    for (hash, priority) in &settings.torrent_priority_overrides {
        if !is_valid_info_hash(hash) {
            return Err(format!("Invalid torrent info hash key: {}", hash));
        }
        if normalize_torrent_priority_label(priority).is_none() {
            return Err(format!("Invalid torrent priority '{}' for {}", priority, hash));
        }
    }
    for hash in &settings.torrent_pinned_hashes {
        if !is_valid_info_hash(hash) {
            return Err(format!("Invalid pinned torrent info hash: {}", hash));
        }
    }
    // Validate category rule regexes
    for rule in &settings.category_rules {
        if let Err(e) = regex::Regex::new(&rule.pattern) {
            return Err(format!("Invalid regex in category rule '{}': {}", rule.name, e));
        }
    }

    let path = get_settings_path();
    
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    
    let json = serde_json::to_string_pretty(settings)
        .map_err(|e| e.to_string())?;
    
    // Write to temp file first, then rename for crash-safe atomicity
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, &json).map_err(|e| e.to_string())?;
    if let Err(_rename_err) = fs::rename(&tmp_path, &path) {
        // Rename can fail cross-device; fall back to direct write
        fs::write(&path, &json).map_err(|e| format!("Failed to write settings: {}", e))?;
        // Clean up orphaned temp file
        let _ = fs::remove_file(&tmp_path);
    }
    
    eprintln!("[settings] Saved to {:?}", path);
    Ok(())
}
