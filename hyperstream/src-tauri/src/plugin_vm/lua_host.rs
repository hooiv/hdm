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
        // Restrict Lua stdlib to safe subset — no os/io/debug/package modules
        let lua = Lua::new_with(mlua::StdLib::TABLE | mlua::StdLib::STRING | mlua::StdLib::MATH | mlua::StdLib::COROUTINE, mlua::LuaOptions::default()).unwrap_or_else(|_| Lua::new());
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
                // SSRF protection: block requests to private/loopback addresses
                crate::api_replay::validate_url_not_private(&url)
                    .map_err(|e| mlua::Error::RuntimeError(e))?;

                let mut req = client.get(&url);
                if let Some(h) = headers {
                    for (k, v) in h {
                        req = req.header(&k, &v);
                    }
                }
                
                match req.send().await {
                    Ok(resp) => {
                        // Check Content-Length header first to reject obviously oversized responses
                        if let Some(cl) = resp.content_length() {
                            if cl > 10 * 1024 * 1024 {
                                return Err(mlua::Error::RuntimeError(
                                    format!("Response too large: {} bytes (max 10 MB)", cl)
                                ));
                            }
                        }
                        // Stream body with size cap to prevent OOM from chunked/unbounded responses
                        let mut body = Vec::new();
                        let mut stream = resp.bytes_stream();
                        use futures_util::StreamExt;
                        while let Some(chunk) = stream.next().await {
                            let chunk = chunk.map_err(|e| mlua::Error::RuntimeError(format!("Read error: {}", e)))?;
                            body.extend_from_slice(&chunk);
                            if body.len() > 10 * 1024 * 1024 {
                                return Err(mlua::Error::RuntimeError(
                                    format!("Response exceeded 10 MB limit at {} bytes", body.len())
                                ));
                            }
                        }
                        Ok(String::from_utf8_lossy(&body).into_owned())
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
            // Limit pattern length to prevent excessive compile time
            if pattern.len() > 1024 {
                return Err(mlua::Error::RuntimeError("Regex pattern too long (max 1024 chars)".to_string()));
            }
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
                let _ = crate::start_download_impl(&app, state.inner(), id.clone(), url, filename, None, None).await;
                
                // Emit toast
                let _ = app.emit("toast", "Plugin triggered download");
                
                Ok(id)
            }
        })?)?;

        // host.downloads.add_download(url, name) — named download variant (Y2)
        let app_dl_named = self.app.clone();
        downloads.set("add_download", lua.create_async_function(move |_, (url, name): (String, String)| {
            let app = app_dl_named.clone();
            async move {
                let state: tauri::State<AppState> = app.state();
                let id = uuid::Uuid::new_v4().to_string();
                
                // Use the provided name as the filename/display name
                let _ = crate::start_download_impl(&app, state.inner(), id.clone(), url, name.clone(), None, None).await;
                
                // Emit toast with the custom name
                let _ = app.emit("toast", format!("Plugin download: {}", name));
                
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
             // Reject obvious traversal attempts before joining
             let sub = std::path::Path::new(&subpath);
             for comp in sub.components() {
                 match comp {
                     std::path::Component::Normal(_) => {},
                     _ => return Err(mlua::Error::RuntimeError("Path traversal detected".to_string())),
                 }
             }
             let target = dd_write.join(&subpath);
             // Canonicalize both paths and verify containment
             let canon_base = dunce::canonicalize(&dd_write).unwrap_or_else(|_| dd_write.clone());
             if let Some(p) = target.parent() {
                 let _ = std::fs::create_dir_all(p);
             }
             let canon_target = dunce::canonicalize(&target).unwrap_or_else(|_| target.clone());
             if !canon_target.starts_with(&canon_base) {
                 return Err(mlua::Error::RuntimeError("Path traversal detected".to_string()));
             }
             std::fs::write(&canon_target, content).map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
             Ok(())
        })?)?;

        // host.fs.exists(subpath)
        let dd_exists = data_dir.clone();
        fs.set("exists", lua.create_function(move |_, subpath: String| {
             let sub = std::path::Path::new(&subpath);
             for comp in sub.components() {
                 match comp {
                     std::path::Component::Normal(_) => {},
                     _ => return Ok(false),
                 }
             }
             let target = dd_exists.join(&subpath);
             let canon_base = dunce::canonicalize(&dd_exists).unwrap_or_else(|_| dd_exists.clone());
             let canon_target = dunce::canonicalize(&target).unwrap_or_else(|_| target);
             if !canon_target.starts_with(&canon_base) {
                 return Ok(false);
             }
             Ok(canon_target.exists())
        })?)?;

        // host.fs.read(subpath)
        let dd_read = data_dir.clone();
        fs.set("read", lua.create_function(move |_, subpath: String| {
             let sub = std::path::Path::new(&subpath);
             for comp in sub.components() {
                 match comp {
                     std::path::Component::Normal(_) => {},
                     _ => return Err(mlua::Error::RuntimeError("Path traversal detected".to_string())),
                 }
             }
             let target = dd_read.join(&subpath);
             let canon_base = dunce::canonicalize(&dd_read).unwrap_or_else(|_| dd_read.clone());
             let canon_target = dunce::canonicalize(&target).unwrap_or_else(|_| target);
             if !canon_target.starts_with(&canon_base) {
                 return Err(mlua::Error::RuntimeError("Path traversal detected".to_string()));
             }
             match std::fs::read_to_string(canon_target) {
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
