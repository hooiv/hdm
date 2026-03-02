use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::AppHandle;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LedgerEvent {
    pub timestamp: u64,
    pub aggregate_id: String,
    pub event_type: String,
    pub payload: serde_json::Value,
}

pub struct SharedLog {
    log_file: PathBuf,
    lock: Mutex<()>,
}

impl SharedLog {
    pub fn new(app: &AppHandle) -> Self {
        let app_dir = app.path().app_data_dir().unwrap_or_else(|_| PathBuf::from("."));
        std::fs::create_dir_all(&app_dir).unwrap_or_default();
        let log_file = app_dir.join("hyperstream_events.log");
        
        Self {
            log_file,
            lock: Mutex::new(()),
        }
    }

    pub fn append(&self, event: LedgerEvent) -> Result<(), String> {
        let _guard = self.lock.lock().map_err(|e| e.to_string())?;
        
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_file)
            .map_err(|e| format!("Failed to open log: {}", e))?;
            
        let serialized = serde_json::to_string(&event)
            .map_err(|e| format!("Serialization error: {}", e))?;
            
        writeln!(file, "{}", serialized).map_err(|e| e.to_string())?;
        
        Ok(())
    }
}
