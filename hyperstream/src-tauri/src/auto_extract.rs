use std::path::Path;
use std::process::Command;

/// Extract an archive file (.zip, .7z, .rar, .tar.gz, .tar.bz2) to a destination directory.
pub async fn extract_archive(archive_path: String, destination: Option<String>) -> Result<serde_json::Value, String> {
    let path = Path::new(&archive_path);
    if !path.exists() {
        return Err(format!("Archive not found: {}", archive_path));
    }

    let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
    let ext = filename.to_lowercase();

    // Determine destination
    let dest = if let Some(d) = destination {
        d
    } else {
        // Extract to a folder with the same name (without extension)
        let mut stem = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
        // Handle double extensions like .tar.gz, .tar.bz2, .tar.xz
        if stem.ends_with(".tar") {
            stem = stem.trim_end_matches(".tar").to_string();
        }
        let parent = path.parent().unwrap_or(Path::new("."));
        parent.join(&stem).to_string_lossy().to_string()
    };

    // Create destination directory
    std::fs::create_dir_all(&dest)
        .map_err(|e| format!("Failed to create directory {}: {}", dest, e))?;

    if ext.ends_with(".zip") || ext.ends_with(".jar") {
        // Use built-in zip crate
        extract_zip(&archive_path, &dest)?;
    } else if ext.ends_with(".tar.gz") || ext.ends_with(".tgz") {
        // Use tar + gzip via PowerShell
        extract_via_powershell(&archive_path, &dest, "tar")?;
    } else if ext.ends_with(".7z") {
        // Try 7z.exe
        extract_via_7zip(&archive_path, &dest)?;
    } else if ext.ends_with(".rar") {
        // Try unrar or 7z
        extract_via_7zip(&archive_path, &dest)?;
    } else if ext.ends_with(".tar.bz2") || ext.ends_with(".tar.xz") || ext.ends_with(".tar") {
        extract_via_powershell(&archive_path, &dest, "tar")?;
    } else {
        return Err(format!("Unsupported archive format: {}", filename));
    }

    // Count extracted files
    let file_count = count_files_recursive(&dest);

    Ok(serde_json::json!({
        "status": "extracted",
        "archive": archive_path,
        "destination": dest,
        "files_extracted": file_count,
    }))
}

fn extract_zip(archive_path: &str, dest: &str) -> Result<(), String> {
    let file = std::fs::File::open(archive_path)
        .map_err(|e| format!("Cannot open zip: {}", e))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("Invalid zip: {}", e))?;
    let dest_path = Path::new(dest);

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| format!("Zip entry error: {}", e))?;
        let outpath = dest_path.join(file.mangled_name());

        // Validate extracted path stays within destination (defense-in-depth beyond mangled_name)
        // Normalize path components without requiring filesystem existence
        let mut normalized = std::path::PathBuf::new();
        for component in outpath.components() {
            match component {
                std::path::Component::ParentDir => { normalized.pop(); },
                std::path::Component::CurDir => {},
                c => normalized.push(c.as_os_str()),
            }
        }
        if let Ok(canonical_dest) = dunce::canonicalize(dest_path) {
            if !normalized.starts_with(&canonical_dest) {
                return Err(format!("Zip entry escapes destination: {}", file.name()));
            }
        }

        if file.name().ends_with('/') {
            std::fs::create_dir_all(&outpath).ok();
        } else {
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            let mut outfile = std::fs::File::create(&outpath)
                .map_err(|e| format!("Cannot create {}: {}", outpath.display(), e))?;
            std::io::copy(&mut file, &mut outfile)
                .map_err(|e| format!("Write error: {}", e))?;
        }
    }
    Ok(())
}

fn extract_via_powershell(archive_path: &str, dest: &str, tool: &str) -> Result<(), String> {
    let result = if tool == "tar" {
        Command::new("tar")
            .args(["-xf", archive_path, "-C", dest])
            .output()
            .map_err(|e| format!("tar failed: {}", e))?
    } else {
        // Single-quote-escape paths for PowerShell -Command to prevent injection
        // via special characters ($, `, (), ;, etc.) in filenames
        let escaped_path = archive_path.replace('\'', "''");
        let escaped_dest = dest.replace('\'', "''");
        let script = format!(
            "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
            escaped_path, escaped_dest
        );
        Command::new("powershell")
            .args([
                "-NoProfile", "-Command",
                &script
            ])
            .output()
            .map_err(|e| format!("PowerShell extract failed: {}", e))?
    };

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(format!("Extraction failed: {}", stderr));
    }

    // Post-extraction: verify no files escaped the destination (tar/zip-slip prevention)
    let canonical_dest = dunce::canonicalize(dest)
        .unwrap_or_else(|_| std::path::PathBuf::from(dest));
    verify_extraction_boundaries(&canonical_dest)?;

    Ok(())
}

fn extract_via_7zip(archive_path: &str, dest: &str) -> Result<(), String> {
    // Try common 7-Zip locations
    let seven_zip_paths = vec![
        r"C:\Program Files\7-Zip\7z.exe",
        r"C:\Program Files (x86)\7-Zip\7z.exe",
        "7z",
    ];

    for exe in &seven_zip_paths {
        let result = Command::new(exe)
            .args(["x", archive_path, &format!("-o{}", dest), "-y"])
            .output();

        if let Ok(output) = result {
            if output.status.success() {
                // Post-extraction: verify no files escaped the destination (Zip-Slip for 7z/RAR)
                let canonical_dest = dunce::canonicalize(dest)
                    .unwrap_or_else(|_| std::path::PathBuf::from(dest));
                verify_extraction_boundaries(&canonical_dest)?;
                return Ok(());
            }
        }
    }

    Err("7-Zip not found. Install 7-Zip to extract .7z and .rar files.".to_string())
}

/// Post-extraction safety check: recursively verify all extracted files stay within dest_dir.
fn verify_extraction_boundaries(dest_dir: &std::path::Path) -> Result<(), String> {
    fn walk(dir: &std::path::Path, root: &std::path::Path) -> Result<(), String> {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let canonical = dunce::canonicalize(&path)
                    .unwrap_or_else(|_| path.clone());
                if !canonical.starts_with(root) {
                    let _ = std::fs::remove_file(&path);
                    return Err(format!("Path traversal detected in archive: {}", canonical.display()));
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

fn count_files_recursive(dir: &str) -> usize {
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.path().is_file() {
                count += 1;
            } else if entry.path().is_dir() {
                count += count_files_recursive(&entry.path().to_string_lossy());
            }
        }
    }
    count
}
