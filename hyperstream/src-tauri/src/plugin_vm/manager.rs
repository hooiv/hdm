use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use tauri::AppHandle;
use crate::plugin_vm::lua_host::{LuaPluginHost, PluginMetadata};

pub struct PluginManager {
    app: AppHandle,
    plugins: Arc<Mutex<HashMap<String, LuaPluginHost>>>,
    metadata_cache: Arc<Mutex<HashMap<String, PluginMetadata>>>,
}

impl PluginManager {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            plugins: Arc::new(Mutex::new(HashMap::new())),
            metadata_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn get_plugins_dir(&self) -> PathBuf {
        // Use local 'plugins' folder for development ease, or app_data
        // For now, let's look in "plugins" relative to CWD
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join("plugins")
    }

    pub async fn load_plugins(&self) -> Result<(), String> {
        let dir = self.get_plugins_dir();
        if !dir.exists() {
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        }

        // Temporary maps to populate
        let mut new_plugins = HashMap::new();
        let mut new_meta_cache = HashMap::new();

        let entries = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
        
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("lua") {
                    let filename = match path.file_stem() {
                        Some(stem) => stem.to_string_lossy().to_string(),
                        None => continue, // Skip files with no stem
                    };
                    
                    // Create Host
                    let client = rquest::Client::new();
                    let host = LuaPluginHost::new(client, self.app.clone());
                    
                    // Init (Async)
                    if let Err(e) = host.init().await {
                        println!("Failed to init plugin {}: {}", filename, e);
                        continue;
                    }

                    // Load Script (Async)
                    let script = std::fs::read_to_string(&path).unwrap_or_default();
                    if let Err(e) = host.load_script(&script).await {
                        println!("Failed to load script {}: {}", filename, e);
                        continue;
                    }

                    // Get Metadata (Async)
                    if let Ok(Some(meta)) = host.get_plugin_metadata().await {
                        new_meta_cache.insert(filename.clone(), meta);
                    } else {
                        new_meta_cache.insert(filename.clone(), PluginMetadata {
                            name: filename.clone(),
                            version: "0.0.1".to_string(),
                            domains: vec![],
                        });
                    }

                    new_plugins.insert(filename, host);
                }
            }
        }
        
        // Critical Section: Swap
        {
            let mut plugins = self.plugins.lock().unwrap();
            let mut meta_cache = self.metadata_cache.lock().unwrap();
            *plugins = new_plugins;
            *meta_cache = new_meta_cache;
            println!("Loaded {} plugins", plugins.len());
        }
        
        Ok(())
    }

    pub fn get_plugins_list(&self) -> Vec<PluginMetadata> {
        let cache = self.metadata_cache.lock().unwrap();
        cache.values().cloned().collect()
    }
}
