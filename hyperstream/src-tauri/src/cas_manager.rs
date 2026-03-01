use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use lazy_static::lazy_static;

#[derive(Serialize, Deserialize, Default)]
struct CasData {
    // Maps ETag or MD5 hash to an absolute file path
    entries: HashMap<String, String>,
}

lazy_static! {
    static ref CAS_MUTEX: Mutex<CasData> = Mutex::new(load_cas_data());
}

fn get_cas_path() -> PathBuf {
    let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("hyperstream");
    path.push("cas_db.json");
    path
}

fn load_cas_data() -> CasData {
    if let Ok(content) = fs::read_to_string(get_cas_path()) {
        if let Ok(data) = serde_json::from_str(&content) {
            return data;
        }
    }
    CasData::default()
}

fn save_cas_data(data: &CasData) {
    if let Some(parent) = get_cas_path().parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(content) = serde_json::to_string_pretty(data) {
        let _ = fs::write(get_cas_path(), content);
    }
}

/// Check if we already have a file matching this ETag or MD5.
/// Returns the absolute path of the local file if a match exists.
pub fn check_cas(etag: Option<&str>, md5: Option<&str>) -> Option<String> {
    let data = CAS_MUTEX.lock().unwrap();
    
    if let Some(e) = etag {
        if let Some(path) = data.entries.get(e) {
            if Path::new(path).exists() {
                return Some(path.clone());
            }
        }
    }
    
    if let Some(m) = md5 {
        if let Some(path) = data.entries.get(m) {
            if Path::new(path).exists() {
                return Some(path.clone());
            }
        }
    }
    
    None
}

/// Register a successfully downloaded file with its ETag and MD5.
pub fn register_cas(etag: Option<&str>, md5: Option<&str>, path: &str) {
    if etag.is_none() && md5.is_none() {
        return; // Nothing to index
    }
    
    let mut data = CAS_MUTEX.lock().unwrap();
    let mut changed = false;
    
    if let Some(e) = etag {
        data.entries.insert(e.to_string(), path.to_string());
        changed = true;
    }
    
    if let Some(m) = md5 {
        data.entries.insert(m.to_string(), path.to_string());
        changed = true;
    }
    
    if changed {
        save_cas_data(&data);
    }
}
