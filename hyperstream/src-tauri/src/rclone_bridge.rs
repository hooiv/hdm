use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
pub struct RcloneRemote {
    pub name: String,
    pub remote_type: String,
}

/// List configured rclone remotes.
pub fn rclone_list_remotes() -> Result<Vec<RcloneRemote>, String> {
    let output = std::process::Command::new("rclone")
        .args(["listremotes", "--long"])
        .output()
        .map_err(|e| format!("rclone not found. Install rclone first. Error: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("rclone error: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut remotes = Vec::new();

    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            remotes.push(RcloneRemote {
                name: parts[0].trim_end_matches(':').to_string(),
                remote_type: parts[1].to_string(),
            });
        } else if !line.trim().is_empty() {
            remotes.push(RcloneRemote {
                name: line.trim().trim_end_matches(':').to_string(),
                remote_type: "unknown".to_string(),
            });
        }
    }

    Ok(remotes)
}

/// Validate that a path uses remote:path format (not local filesystem).
fn validate_remote_path(path: &str) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.starts_with(':') || trimmed.starts_with('/') || trimmed.starts_with('\\')
        || (trimmed.len() >= 2 && trimmed.as_bytes()[1] == b':') // drive letter like C:
        || !trimmed.contains(':')
    {
        return Err(format!("Invalid remote path '{}'. Must use 'remote:path' format (e.g. 'gdrive:backup/'). Local filesystem paths are not allowed.", trimmed));
    }
    Ok(())
}

/// Transfer files between rclone remotes (cloud-to-cloud).
/// This is async to avoid blocking Tauri's IPC thread during potentially
/// long-running cloud-to-cloud transfers.
pub async fn rclone_transfer(source: String, destination: String) -> Result<String, String> {
    // Validate inputs
    if source.is_empty() || destination.is_empty() {
        return Err("Source and destination cannot be empty.".to_string());
    }
    // Prevent local filesystem exfiltration — both paths must be remote
    validate_remote_path(&source)?;
    validate_remote_path(&destination)?;

    // Run rclone copy with progress — use tokio::process::Command to avoid blocking
    let output = tokio::process::Command::new("rclone")
        .args([
            "copy",
            &source,
            &destination,
            "--progress",
            "--stats", "5s",
            "--transfers", "4",
            "--checkers", "8",
            "-v",
        ])
        .output()
        .await
        .map_err(|e| format!("rclone failed: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        Ok(format!("Transfer complete!\n\n{}\n{}", stdout.trim(), stderr.trim()))
    } else {
        Err(format!("Transfer failed:\n{}\n{}", stdout.trim(), stderr.trim()))
    }
}

/// Get rclone version info.
pub fn rclone_version() -> Result<String, String> {
    let output = std::process::Command::new("rclone")
        .arg("version")
        .output()
        .map_err(|e| format!("rclone not found: {}", e))?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// List contents of a remote path.
pub fn rclone_ls(remote_path: String) -> Result<String, String> {
    // Validate remote_path isn't accessing local filesystem directly
    let trimmed = remote_path.trim();
    if trimmed.starts_with(':') || trimmed.starts_with('/') || trimmed.starts_with('\\') 
        || (trimmed.len() >= 2 && trimmed.as_bytes()[1] == b':') // drive letter like C:
        || !trimmed.contains(':') // no remote prefix at all
    {
        return Err("Invalid remote path. Must use the format 'remote:path'. Local paths are not allowed.".to_string());
    }
    
    // Use trimmed path to ensure no leading/trailing whitespace reaches rclone
    let output = std::process::Command::new("rclone")
        .args(["ls", trimmed, "--max-depth", "1"])
        .output()
        .map_err(|e| format!("rclone ls failed: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}
