use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::AppHandle;
use crate::plugin_vm::lua_host::{LuaPluginHost, PluginMetadata};
use crate::search::{self, SearchResult};

pub struct PluginManager {
    app: AppHandle,
    plugins: Arc<Mutex<HashMap<String, Arc<LuaPluginHost>>>>,
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

    pub fn get_plugins_dir(&self) -> std::path::PathBuf {
        crate::plugin_vm::get_plugins_dir(&self.app)
    }

    pub async fn load_plugins(&self) -> Result<(), String> {
        let dir = self.get_plugins_dir();
        if !dir.exists() {
            tokio::fs::create_dir_all(&dir).await.map_err(|e| e.to_string())?;
        }

        // Temporary maps to populate
        let mut new_plugins = HashMap::new();
        let mut new_meta_cache = HashMap::new();

        let entries = std::fs::read_dir(&dir).map_err(|e| e.to_string())?;
        
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
                    let script = tokio::fs::read_to_string(&path).await.unwrap_or_default();
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

                    new_plugins.insert(filename, Arc::new(host));
                }
            }
        }
        
        // Critical Section: Swap
        {
            let mut plugins = self.plugins.lock().unwrap_or_else(|e| e.into_inner());
            let mut meta_cache = self.metadata_cache.lock().unwrap_or_else(|e| e.into_inner());
            *plugins = new_plugins;
            *meta_cache = new_meta_cache;
            println!("Loaded {} plugins", plugins.len());
        }
        
        Ok(())
    }

    pub fn get_plugins_list(&self) -> Vec<PluginMetadata> {
        let cache = self.metadata_cache.lock().unwrap_or_else(|e| e.into_inner());
        let mut plugins: Vec<_> = cache.values().cloned().collect();
        plugins.sort_by(|left, right| left.name.to_ascii_lowercase().cmp(&right.name.to_ascii_lowercase()));
        plugins
    }

    pub async fn search(&self, query: &str) -> Result<Vec<SearchResult>, String> {
        let query = search::sanitize_query(query)?;

        let providers: Vec<(String, String, Arc<LuaPluginHost>)> = {
            let plugins = self.plugins.lock().unwrap_or_else(|e| e.into_inner());
            let metadata_cache = self.metadata_cache.lock().unwrap_or_else(|e| e.into_inner());

            plugins
                .iter()
                .map(|(filename, host)| {
                    let engine_name = metadata_cache
                        .get(filename)
                        .map(|meta| meta.name.trim())
                        .filter(|name| !name.is_empty())
                        .unwrap_or(filename.as_str())
                        .to_string();

                    (filename.clone(), engine_name, Arc::clone(host))
                })
                .collect()
        };

        let mut aggregated = Vec::new();

        for (filename, engine_name, host) in providers {
            match host.search(&query, &engine_name).await {
                Ok(Some(mut results)) => aggregated.append(&mut results),
                Ok(None) => {}
                Err(err) => eprintln!("Search provider {} failed: {}", filename, err),
            }
        }

        Ok(search::finalize_results(aggregated))
    }
}
