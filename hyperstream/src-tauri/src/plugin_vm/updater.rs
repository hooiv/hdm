use std::path::PathBuf;
use std::fs;
use reqwest;

/// Install or update a plugin from a URL
pub async fn install_plugin_from_url(app_handle: &tauri::AppHandle, url: String, filename: Option<String>) -> Result<String, String> {
    println!("DEBUG: Installing plugin from {}", url);

    // 1. Fetch content
    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Download failed with status: {}", response.status()));
    }

    let content = response.text()
        .await
        .map_err(|e| format!("Failed to read content: {}", e))?;

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

    let target_path = plugins_dir.join(&final_filename);
    fs::write(&target_path, content).map_err(|e| format!("Failed to write plugin file: {}", e))?;

    println!("DEBUG: Plugin installed to {:?}", target_path);
    Ok(final_filename)
}

fn get_plugins_dir(app_handle: &tauri::AppHandle) -> PathBuf {
    use tauri::Manager;
    app_handle.path().app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("plugins")
}
