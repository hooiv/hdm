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

        // Rotate log if it exceeds 50 MB to prevent unbounded disk growth
        const MAX_LOG_SIZE: u64 = 50 * 1024 * 1024;
        const MAX_ROTATED_LOGS: usize = 5;
        if let Ok(metadata) = std::fs::metadata(&self.log_file) {
            if metadata.len() > MAX_LOG_SIZE {
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default().as_secs();
                let rotated = self.log_file.with_extension(format!("log.{}", timestamp));
                let _ = std::fs::rename(&self.log_file, &rotated);

                // Prune oldest rotated logs beyond MAX_ROTATED_LOGS
                if let Some(parent) = self.log_file.parent() {
                    if let Some(stem) = self.log_file.file_stem().and_then(|s| s.to_str()) {
                        let mut rotated_files: Vec<_> = std::fs::read_dir(parent)
                            .into_iter()
                            .flatten()
                            .flatten()
                            .filter(|e| {
                                e.file_name().to_string_lossy().starts_with(stem)
                                    && e.path() != self.log_file
                            })
                            .collect();
                        if rotated_files.len() > MAX_ROTATED_LOGS {
                            rotated_files.sort_by_key(|e| e.file_name());
                            for old in &rotated_files[..rotated_files.len() - MAX_ROTATED_LOGS] {
                                let _ = std::fs::remove_file(old.path());
                            }
                        }
                    }
                }
            }
        }

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
