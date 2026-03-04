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
        let cas_path = get_cas_path();
        let tmp_path = cas_path.with_extension("json.tmp");
        if let Err(e) = fs::write(&tmp_path, &content) {
            eprintln!("[CAS] Failed to write temp file {:?}: {}", tmp_path, e);
            return;
        }
        if let Err(e) = fs::rename(&tmp_path, &cas_path) {
            eprintln!("[CAS] rename failed ({}), falling back to copy", e);
            if let Err(e2) = fs::copy(&tmp_path, &cas_path) {
                eprintln!("[CAS] copy fallback also failed: {}", e2);
            }
            let _ = fs::remove_file(&tmp_path);
        }
    }
}

/// Check if we already have a file matching this ETag or MD5.
/// Returns the absolute path of the local file if a match exists.
pub fn check_cas(etag: Option<&str>, md5: Option<&str>) -> Option<String> {
    let data = CAS_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    
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
    
    let mut data = CAS_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    // Cap at 10,000 entries to prevent unbounded memory growth.
    // When exceeding, remove oldest entries (arbitrary eviction since HashMap is unordered).
    const MAX_CAS_ENTRIES: usize = 10_000;
    if data.entries.len() >= MAX_CAS_ENTRIES {
        let keys_to_remove: Vec<String> = data.entries.keys()
            .take(data.entries.len() - MAX_CAS_ENTRIES + 2)
            .cloned()
            .collect();
        for key in keys_to_remove {
            data.entries.remove(&key);
        }
    }

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
