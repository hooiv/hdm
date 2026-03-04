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
            
            // Single RAR file
            return Some(ArchiveInfo {
                path: path.to_string(),
                archive_type: ArchiveType::Rar,
                is_multi_part: false,
                part_number: None,
                base_name: lowercase.clone(),
            });
        }
        
        // Old-style multi-part RAR (.r00, .r01, ...) — must be checked OUTSIDE .rar guard
        let r_regex = Regex::new(r"\.r(\d{2,})$").ok()?;
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
                base_name: lowercase.clone(),
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
        // Canonicalize dest_dir before extraction to validate post-extraction
        let canonical_dest = dunce::canonicalize(dest_dir)
            .unwrap_or_else(|_| std::path::PathBuf::from(dest_dir));

        // Try unrar first, then fall back to WinRAR
        let dest_with_slash = format!("{}\\", dest_dir);
        let commands: Vec<(&str, Vec<&str>)> = vec![
            ("unrar", vec!["x", "-o+", "-y", archive_path, &dest_with_slash]),
            ("C:\\Program Files\\WinRAR\\UnRAR.exe", vec!["x", "-o+", "-y", archive_path, &dest_with_slash]),
        ];
        
        for (cmd, args) in commands {
            match Command::new(cmd).args(&args).output() {
                Ok(output) => {
                    if output.status.success() {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        println!("✅ Extraction successful:\n{}", stdout);

                        // Post-extraction: verify no files escaped dest_dir (Zip-Slip for RAR)
                        if let Err(e) = Self::verify_no_path_escape(&canonical_dest) {
                            return Err(format!("Path traversal detected in archive: {}", e));
                        }

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
        // On Windows, use PowerShell Expand-Archive with safe argument passing
        #[cfg(target_os = "windows")]
        {
            // Use -LiteralPath to avoid command injection via string interpolation
            let ps_script = format!(
                "Expand-Archive -LiteralPath $args[0] -DestinationPath $args[1] -Force"
            );

            match Command::new("powershell")
                .args(&["-NoProfile", "-Command", &ps_script, archive_path, dest_dir])
                .output()
            {
                Ok(output) => {
                    if output.status.success() {
                        // Post-extraction safety check for zip-slip
                        let canonical_dest = dunce::canonicalize(dest_dir)
                            .map_err(|e| format!("Cannot resolve dest dir: {}", e))?;
                        if let Err(e) = Self::verify_no_path_escape(&canonical_dest) {
                            return Err(format!("ZIP extraction aborted — path traversal detected: {}", e));
                        }
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
                        // Post-extraction safety check for zip-slip
                        let canonical_dest = dunce::canonicalize(dest_dir)
                            .map_err(|e| format!("Cannot resolve dest dir: {}", e))?;
                        if let Err(e) = Self::verify_no_path_escape(&canonical_dest) {
                            return Err(format!("ZIP extraction aborted — path traversal detected: {}", e));
                        }
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

    /// Post-extraction safety check: Recursively verify all files inside dest_dir 
    /// have canonical paths that start with dest_dir (no path traversal / Zip-Slip).
    fn verify_no_path_escape(dest_dir: &std::path::Path) -> Result<(), String> {
        fn walk(dir: &std::path::Path, root: &std::path::Path) -> Result<(), String> {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let canonical = dunce::canonicalize(&path)
                        .unwrap_or_else(|_| path.clone());
                    if !canonical.starts_with(root) {
                        // Remove the offending file/symlink immediately
                        let _ = std::fs::remove_file(&path);
                        return Err(format!("Escaped path detected: {}", canonical.display()));
                    }
                    if path.is_dir() {
                        walk(&path, root)?;
                    }
                }
            }
            Ok(())
        }
        walk(dest_dir, dest_dir)
    }
}
