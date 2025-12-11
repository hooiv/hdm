use mlua::{Lua, Result, Table, Function, Value};
use serde::{Serialize, Deserialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use rquest::Client;
use regex::Regex;
use tauri::{AppHandle, Emitter, Manager};
use crate::AppState;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PluginMetadata {
    pub name: String,
    pub version: String,
    pub domains: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExtractionResult {
    pub url: String,
    pub cookies: Option<String>,
    pub headers: Option<std::collections::HashMap<String, String>>,
    pub filename: Option<String>,
}

pub struct LuaPluginHost {
    lua: Arc<Mutex<Lua>>,
    client: Client,
    app: AppHandle,
}

impl LuaPluginHost {
    pub fn new(client: Client, app: AppHandle) -> Self {
        let lua = Lua::new();
        Self {
            lua: Arc::new(Mutex::new(lua)),
            client,
            app,
        }
    }

    pub async fn init(&self) -> Result<()> {
        let lua = self.lua.lock().await;
        
        // Register 'host' table with exposed functions
        let globals = lua.globals();
        let host = lua.create_table()?;

        // host.http_get(url, headers)
        let client = self.client.clone();
        host.set("http_get", lua.create_async_function(move |_, (url, headers): (String, Option<std::collections::HashMap<String, String>>)| {
            let client = client.clone();
            async move {
                let mut req = client.get(&url);
                if let Some(h) = headers {
                    for (k, v) in h {
                        req = req.header(&k, &v);
                    }
                }
                
                match req.send().await {
                    Ok(resp) => {
                        match resp.text().await {
                            Ok(text) => Ok(text),
                            Err(e) => Err(mlua::Error::RuntimeError(format!("Failed to read body: {}", e)))
                        }
                    },
                    Err(e) => Err(mlua::Error::RuntimeError(format!("Request failed: {}", e)))
                }
            }
        })?)?;

        // host.log(msg)
        host.set("log", lua.create_function(|_, msg: String| {
            println!("LUA [Plugin]: {}", msg);
            Ok(())
        })?)?;

        // host.toast(msg, type)
        let app_handle = self.app.clone();
        host.set("toast", lua.create_function(move |_, (msg, type_str): (String, Option<String>)| {
            let type_s = type_str.unwrap_or_else(|| "info".to_string());
            // Emit event to frontend for toast
            let _ = app_handle.emit("plugin_toast", serde_json::json!({
                "message": msg,
                "type": type_s
            }));
            Ok(())
        })?)?;

        // host.regex_find(pattern, text)
        host.set("regex_find", lua.create_function(|_, (pattern, text): (String, String)| {
            if let Ok(re) = Regex::new(&pattern) {
                if let Some(caps) = re.captures(&text) {
                    if let Some(m) = caps.get(1) {
                        return Ok(Some(m.as_str().to_string()));
                    }
                }
            }
            Ok(None::<String>)
        })?)?;

        // host.downloads namespace
        let downloads = lua.create_table()?;
        let app_dl = self.app.clone();
        
        // host.downloads.add(url, filename)
        downloads.set("add", lua.create_async_function(move |_, (url, filename): (String, String)| {
            let app = app_dl.clone();
            async move {
                let state: tauri::State<AppState> = app.state();
                // Settings loaded inside start_download_impl
                let id = uuid::Uuid::new_v4().to_string();
                
                // Trigger backend add via reusable impl
                let _ = crate::start_download_impl(&app, state.inner(), id.clone(), url, filename, None).await;
                
                // Emit toast
                let _ = app.emit("toast", "Plugin triggered download");
                
                Ok(id)
            }
        })?)?;
        host.set("downloads", downloads)?;

        // host.fs namespace (Sandboxed to 'plugins_data')
        let fs = lua.create_table()?;
        // Resolve data path: CWD/plugins_data (safe-ish)
        let data_dir = std::env::current_dir().unwrap_or_default().join("plugins_data");
        if !data_dir.exists() {
            let _ = std::fs::create_dir_all(&data_dir);
        }


        // host.fs.write(subpath, content)
        let dd_write = data_dir.clone();
        fs.set("write", lua.create_function(move |_, (subpath, content): (String, String)| {
             let target = dd_write.join(&subpath);
             // Sandbox check
             if !target.starts_with(&dd_write) {
                 return Err(mlua::Error::RuntimeError("Path traversal detected".to_string()));
             }
             if let Some(p) = target.parent() {
                 let _ = std::fs::create_dir_all(p);
             }
             std::fs::write(target, content).map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
             Ok(())
        })?)?;

        // host.fs.exists(subpath)
        let dd_exists = data_dir.clone();
        fs.set("exists", lua.create_function(move |_, subpath: String| {
             let target = dd_exists.join(&subpath);
             if !target.starts_with(&dd_exists) {
                 return Ok(false);
             }
             Ok(target.exists())
        })?)?;

        // host.fs.read(subpath)
        let dd_read = data_dir.clone();
        fs.set("read", lua.create_function(move |_, subpath: String| {
             let target = dd_read.join(&subpath);
             if !target.starts_with(&dd_read) {
                 return Err(mlua::Error::RuntimeError("Path traversal detected".to_string()));
             }
             match std::fs::read_to_string(target) {
                 Ok(s) => Ok(Some(s)),
                 Err(_) => Ok(None)
             }
        })?)?;
        
        host.set("fs", fs)?;

        globals.set("host", host)?;

        Ok(())
    }

    pub async fn load_script(&self, script: &str) -> Result<()> {
        let lua = self.lua.lock().await;
        lua.load(script).exec()?;
        Ok(())
    }

    pub async fn extract_stream(&self, page_url: &str) -> Result<Option<ExtractionResult>> {
        let lua = self.lua.lock().await;
        let globals = lua.globals();
        
        if let Ok(extract_fn) = globals.get::<_, Function>("extract_stream") {
            let result: Value = extract_fn.call(page_url)?;
            
            if let Value::Table(t) = result {
                // Manually extract fields or use serde if configured
                // Using manual extraction for safety
                let url: String = t.get("url")?;
                let cookies: Option<String> = t.get("cookies").ok();
                let filename: Option<String> = t.get("filename").ok();
                let headers: Option<std::collections::HashMap<String, String>> = t.get("headers").ok();

                return Ok(Some(ExtractionResult {
                    url,
                    cookies,
                    headers,
                    filename,
                }));
            }
        }
        
        Ok(None)
    }

    /// Get plugin metadata by reading from a Lua table (uses Table type explicitly)
    pub async fn get_plugin_metadata(&self) -> Result<Option<PluginMetadata>> {
        let lua = self.lua.lock().await;
        let globals = lua.globals();
        
        // Try to get the 'plugin' table that should be defined in the plugin script
        if let Ok(plugin_table) = globals.get::<_, Table>("plugin") {
            let name: String = plugin_table.get("name").unwrap_or_else(|_| "Unknown".to_string());
            let version: String = plugin_table.get("version").unwrap_or_else(|_| "1.0".to_string());
            
            // Get domains as a Lua table and convert to Vec<String>
            let mut domains = Vec::new();
            if let Ok(domains_table) = plugin_table.get::<_, Table>("domains") {
                for pair in domains_table.pairs::<i32, String>() {
                    if let Ok((_, domain)) = pair {
                        domains.push(domain);
                    }
                }
            }
            
            return Ok(Some(PluginMetadata { name, version, domains }));
        }
        
        Ok(None)
    }

    /// Create a configuration table for the plugin (uses Table type explicitly)
    pub async fn set_config(&self, config: std::collections::HashMap<String, String>) -> Result<()> {
        let lua = self.lua.lock().await;
        let globals = lua.globals();
        
        // Create a new Lua table for config
        let config_table: Table = lua.create_table()?;
        
        for (key, value) in config {
            config_table.set(key, value)?;
        }
        
        // Set as global 'config' table
        globals.set("config", config_table)?;
        
        Ok(())
    }
}
