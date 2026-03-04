use std::path::PathBuf;
use std::fs;
use reqwest;
use sha2::{Sha256, Digest};

/// Install or update a plugin from a URL.
/// If `expected_sha256` is provided, verifies the downloaded content matches the hash.
pub async fn install_plugin_from_url(app_handle: &tauri::AppHandle, url: String, filename: Option<String>, expected_sha256: Option<String>) -> Result<String, String> {
    // Validate URL scheme — only allow https
    if !url.starts_with("https://") {
        return Err("Only HTTPS plugin URLs are allowed for security".to_string());
    }

    // 1. Fetch content
    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Download failed with status: {}", response.status()));
    }

    let content_bytes = response.bytes()
        .await
        .map_err(|e| format!("Failed to read content: {}", e))?;
    // Cap plugin size to 1 MB
    if content_bytes.len() > 1_000_000 {
        return Err(format!("Plugin too large: {} bytes (max 1 MB)", content_bytes.len()));
    }
    let content = String::from_utf8_lossy(&content_bytes).into_owned();

    // 2. Verify integrity if hash provided
    if let Some(expected) = &expected_sha256 {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        let actual = hex::encode(hasher.finalize());
        if actual != expected.to_lowercase() {
            return Err(format!(
                "Integrity check failed! Expected SHA-256: {}, got: {}. Plugin may be tampered.",
                expected, actual
            ));
        }
    }

    // 3. Basic content validation — must look like Lua code
    if content.contains("<script") || content.contains("<?php") || content.contains("#!/") {
        return Err("Plugin content doesn't appear to be valid Lua code".to_string());
    }

    // 2. Determine filename
    let final_filename = if let Some(name) = filename {
        if !name.ends_with(".lua") {
            format!("{}.lua", name)
        } else {
            name
        }
    } else {
        // Try to infer from URL
        url.split('/').last()
            .map(|s| if s.ends_with(".lua") { s.to_string() } else { format!("{}.lua", s) })
            .unwrap_or_else(|| "unknown_plugin.lua".to_string())
    };

    // 3. Save to plugins directory
    let plugins_dir = get_plugins_dir(app_handle);
    if !plugins_dir.exists() {
        fs::create_dir_all(&plugins_dir).map_err(|e| format!("Failed to create plugins dir: {}", e))?;
    }

    // Validate filename doesn't contain path traversal sequences
    if final_filename.contains("..") || final_filename.contains('/') || final_filename.contains('\\') {
        return Err("Invalid plugin filename: must not contain path separators or '..'".to_string());
    }

    // Reject Windows reserved device names that cause undefined behavior
    let stem = final_filename.split('.').next().unwrap_or("").to_uppercase();
    const RESERVED: &[&str] = &["CON","PRN","AUX","NUL","COM1","COM2","COM3","COM4",
        "COM5","COM6","COM7","COM8","COM9","LPT1","LPT2","LPT3","LPT4","LPT5","LPT6","LPT7","LPT8","LPT9"];
    if RESERVED.contains(&stem.as_str()) {
        return Err("Invalid plugin filename: Windows reserved device name".to_string());
    }

    let target_path = plugins_dir.join(&final_filename);
    // Double-check the resolved path stays within plugins_dir
    let canonical_plugins = dunce::canonicalize(&plugins_dir)
        .unwrap_or_else(|_| plugins_dir.clone());
    let canonical_target = canonical_plugins.join(&final_filename);
    if !canonical_target.starts_with(&canonical_plugins) {
        return Err("Plugin path escapes plugins directory".to_string());
    }

    fs::write(&target_path, content).map_err(|e| format!("Failed to write plugin file: {}", e))?;

    Ok(final_filename)
}

fn get_plugins_dir(app_handle: &tauri::AppHandle) -> PathBuf {
    use tauri::Manager;
    app_handle.path().app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("plugins")
}
