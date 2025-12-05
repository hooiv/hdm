use serde::{Serialize, Deserialize};
use std::path::Path;
use std::fs;

/// Exported download data format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportedDownload {
    pub url: String,
    pub filename: String,
    pub save_path: String,
    pub category: Option<String>,
    pub total_size: u64,
    pub downloaded_bytes: u64,
    pub status: String,
    pub added_at: String,
}

/// Export format for HyperStream data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HyperStreamExport {
    pub version: String,
    pub export_date: String,
    pub downloads: Vec<ExportedDownload>,
}

impl HyperStreamExport {
    pub fn new() -> Self {
        Self {
            version: "1.0".to_string(),
            export_date: chrono::Local::now().to_rfc3339(),
            downloads: Vec::new(),
        }
    }

    /// Export to JSON file
    pub fn to_json_file(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize: {}", e))?;
        fs::write(path, json).map_err(|e| format!("Failed to write file: {}", e))?;
        Ok(())
    }

    /// Import from JSON file
    pub fn from_json_file(path: &Path) -> Result<Self, String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read file: {}", e))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse: {}", e))
    }

    /// Export to CSV format
    pub fn to_csv_file(&self, path: &Path) -> Result<(), String> {
        let mut csv = String::from("url,filename,save_path,category,total_size,downloaded_bytes,status,added_at\n");
        
        for d in &self.downloads {
            csv.push_str(&format!(
                "{},{},{},{},{},{},{},{}\n",
                escape_csv(&d.url),
                escape_csv(&d.filename),
                escape_csv(&d.save_path),
                escape_csv(&d.category.as_deref().unwrap_or("")),
                d.total_size,
                d.downloaded_bytes,
                escape_csv(&d.status),
                escape_csv(&d.added_at),
            ));
        }

        fs::write(path, csv).map_err(|e| format!("Failed to write CSV: {}", e))?;
        Ok(())
    }
}

/// Import from IDM export file
pub fn import_from_idm(path: &Path) -> Result<Vec<ExportedDownload>, String> {
    // IDM exports in a custom format - this is a best-effort parser
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read IDM export: {}", e))?;
    
    let mut downloads = Vec::new();
    
    // IDM export format parsing (simplified)
    for line in content.lines() {
        if line.starts_with("http://") || line.starts_with("https://") {
            let url = line.trim().to_string();
            let filename = url.split('/').last().unwrap_or("download").to_string();
            
            downloads.push(ExportedDownload {
                url,
                filename,
                save_path: String::new(),
                category: None,
                total_size: 0,
                downloaded_bytes: 0,
                status: "Pending".to_string(),
                added_at: chrono::Local::now().to_rfc3339(),
            });
        }
    }
    
    Ok(downloads)
}

/// Import from FDM (Free Download Manager) export
pub fn import_from_fdm(path: &Path) -> Result<Vec<ExportedDownload>, String> {
    // FDM exports as SQLite - we'd need rusqlite for full support
    // For now, support their txt export format
    import_from_idm(path) // Same basic format
}

fn escape_csv(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_create() {
        let export = HyperStreamExport::new();
        assert_eq!(export.version, "1.0");
        assert!(export.downloads.is_empty());
    }

    #[test]
    fn test_csv_escape() {
        assert_eq!(escape_csv("hello"), "hello");
        assert_eq!(escape_csv("hello,world"), "\"hello,world\"");
        assert_eq!(escape_csv("say \"hi\""), "\"say \"\"hi\"\"\"");
    }
}
