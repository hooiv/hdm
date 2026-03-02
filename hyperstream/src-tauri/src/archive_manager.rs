use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;
use regex::Regex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveInfo {
    pub path: String,
    pub archive_type: ArchiveType,
    pub is_multi_part: bool,
    pub part_number: Option<u32>,
    pub base_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ArchiveType {
    Rar,
    Zip,
    SevenZip,
    Unknown,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveGroup {
    pub id: String,
    pub base_name: String,
    pub parts: Vec<String>,
    pub total_parts: u32,
    pub archive_type: ArchiveType,
    pub is_complete: bool,
}

pub struct ArchiveManager;

impl ArchiveManager {
    /// Detect if a file is part of a multi-part archive
    pub fn detect_archive(path: &str) -> Option<ArchiveInfo> {
        let file_path = Path::new(path);
        let filename = file_path.file_name()?.to_str()?;
        let lowercase = filename.to_lowercase();

        // RAR patterns
        if lowercase.ends_with(".rar") {
            // Check for .partXX.rar pattern
            let part_regex = Regex::new(r"\.part(\d+)\.rar$").ok()?;
            if let Some(caps) = part_regex.captures(&lowercase) {
                let part_num = caps.get(1)?.as_str().parse::<u32>().ok()?;
                let base = part_regex.replace(&lowercase, "").to_string();
                return Some(ArchiveInfo {
                    path: path.to_string(),
                    archive_type: ArchiveType::Rar,
                    is_multi_part: true,
                    part_number: Some(part_num),
                    base_name: base,
                });
            }
            
            // Check for .rXX pattern
            let r_regex = Regex::new(r"\.r(\d+)$").ok()?;
            if lowercase.ends_with(".r00") || lowercase.ends_with(".r01") {
                if let Some(caps) = r_regex.captures(&lowercase) {
                    let part_num = caps.get(1)?.as_str().parse::<u32>().ok()?;
                    let base = r_regex.replace(&lowercase, ".rar").to_string();
                    return Some(ArchiveInfo {
                        path: path.to_string(),
                        archive_type: ArchiveType::Rar,
                        is_multi_part: true,
                        part_number: Some(part_num + 1),
                        base_name: base,
                    });
                }
            }
            
            // Single RAR file
            return Some(ArchiveInfo {
                path: path.to_string(),
                archive_type: ArchiveType::Rar,
                is_multi_part: false,
                part_number: None,
                base_name: filename.to_string(),
            });
        }
        
        // ZIP split patterns (.zip.001, .z01)
        if lowercase.contains(".zip.") {
            let zip_regex = Regex::new(r"\.zip\.(\d+)$").ok()?;
            if let Some(caps) = zip_regex.captures(&lowercase) {
                let part_num = caps.get(1)?.as_str().parse::<u32>().ok()?;
                let base = zip_regex.replace(&lowercase, ".zip").to_string();
                return Some(ArchiveInfo {
                    path: path.to_string(),
                    archive_type: ArchiveType::Zip,
                    is_multi_part: true,
                    part_number: Some(part_num),
                    base_name: base,
                });
            }
        }
        
        if lowercase.ends_with(".z01") || lowercase.ends_with(".z02") {
            let z_regex = Regex::new(r"\.z(\d+)$").ok()?;
            if let Some(caps) = z_regex.captures(&lowercase) {
                let part_num = caps.get(1)?.as_str().parse::<u32>().ok()?;
                let base = z_regex.replace(&lowercase, ".zip").to_string();
                return Some(ArchiveInfo {
                    path: path.to_string(),
                    archive_type: ArchiveType::Zip,
                    is_multi_part: true,
                    part_number: Some(part_num),
                    base_name: base,
                });
            }
        }
        
        // Single ZIP file
        if lowercase.ends_with(".zip") {
            return Some(ArchiveInfo {
                path: path.to_string(),
                archive_type: ArchiveType::Zip,
                is_multi_part: false,
                part_number: None,
                base_name: filename.to_string(),
            });
        }
        
        None
    }

    /// Extract archive to destination directory
    pub fn extract_archive(archive_path: &str, dest_dir: &str) -> Result<String, String> {
        let path = Path::new(archive_path);
        
        if !path.exists() {
            return Err(format!("Archive not found: {}", archive_path));
        }
        
        let archive_info = Self::detect_archive(archive_path)
            .ok_or_else(|| "Not a recognized archive format".to_string())?;
        
        match archive_info.archive_type {
            ArchiveType::Rar => Self::extract_rar(archive_path, dest_dir),
            ArchiveType::Zip => Self::extract_zip(archive_path, dest_dir),
            _ => Err("Unsupported archive type".to_string()),
        }
    }

    /// Extract RAR archive using system unrar command
    fn extract_rar(archive_path: &str, dest_dir: &str) -> Result<String, String> {
        // Try unrar first, then fall back to WinRAR
        let commands = vec![
            ("unrar", vec!["x", "-o+", "-y", archive_path, dest_dir]),
            ("C:\\Program Files\\WinRAR\\UnRAR.exe", vec!["x", "-o+", "-y", archive_path, dest_dir]),
        ];
        
        for (cmd, args) in commands {
            match Command::new(cmd).args(&args).output() {
                Ok(output) => {
                    if output.status.success() {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        println!("✅ Extraction successful:\n{}", stdout);
                        return Ok(format!("Extracted to: {}", dest_dir));
                    } else {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        eprintln!("⚠️  Extraction failed: {}", stderr);
                    }
                }
                Err(_) => continue, // Try next command
            }
        }
        
        Err("unrar command not found. Please install WinRAR or unrar.".to_string())
    }

    /// Extract ZIP archive using system commands
    fn extract_zip(archive_path: &str, dest_dir: &str) -> Result<String, String> {
        // On Windows, use PowerShell Expand-Archive
        #[cfg(target_os = "windows")]
        {
            let ps_cmd = format!(
                "Expand-Archive -Path '{}' -DestinationPath '{}' -Force",
                archive_path, dest_dir
            );
            
            match Command::new("powershell")
                .args(&["-Command", &ps_cmd])
                .output()
            {
                Ok(output) => {
                    if output.status.success() {
                        return Ok(format!("Extracted to: {}", dest_dir));
                    } else {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        return Err(format!("ZIP extraction failed: {}", stderr));
                    }
                }
                Err(e) => {
                    return Err(format!("Failed to run PowerShell: {}", e));
                }
            }
        }
        
        #[cfg(not(target_os = "windows"))]
        {
            match Command::new("unzip")
                .args(&["-o", archive_path, "-d", dest_dir])
                .output()
            {
                Ok(output) => {
                    if output.status.success() {
                        Ok(format!("Extracted to: {}", dest_dir))
                    } else {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        Err(format!("unzip failed: {}", stderr))
                    }
                }
                Err(e) => Err(format!("unzip command not found: {}", e)),
            }
        }
    }

    /// Delete archive files (for cleanup after extraction)
    pub fn cleanup_archive(archive_path: &str) -> Result<(), String> {
        let path = Path::new(archive_path);
        
        // If multi-part, find and delete all parts
        if let Some(archive_info) = Self::detect_archive(archive_path) {
            if archive_info.is_multi_part {
                let parent = path.parent().ok_or("Invalid path")?;
                let base = &archive_info.base_name;
                
                // Find all related parts
                if let Ok(entries) = std::fs::read_dir(parent) {
                    for entry in entries.flatten() {
                        let entry_path = entry.path();
                        if let Some(detected) = Self::detect_archive(entry_path.to_str().unwrap_or("")) {
                            if detected.base_name == *base {
                                if let Err(e) = std::fs::remove_file(&entry_path) {
                                    eprintln!("Failed to delete {}: {}", entry_path.display(), e);
                                } else {
                                    println!("🗑️  Deleted: {}", entry_path.display());
                                }
                            }
                        }
                    }
                }
            } else {
                // Single file archive
                std::fs::remove_file(path)
                    .map_err(|e| format!("Failed to delete archive: {}", e))?;
                println!("🗑️  Deleted: {}", archive_path);
            }
        }
        
        Ok(())
    }

    /// Check if unrar is available on the system
    pub fn check_unrar_available() -> bool {
        Command::new("unrar").arg("-?").output().is_ok() ||
        Command::new("C:\\Program Files\\WinRAR\\UnRAR.exe").arg("-?").output().is_ok()
    }
}
