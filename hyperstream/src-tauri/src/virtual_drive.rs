use std::process::Command;
use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
pub struct MountedDrive {
    pub letter: String,
    pub path: String,
    pub status: String,
}

/// Mount a folder as a virtual drive letter using Windows `subst` command.
pub fn mount_drive(path: String, letter: String) -> Result<String, String> {
    let letter = letter.trim().to_uppercase();
    if letter.len() != 1 || !letter.chars().next().unwrap().is_ascii_alphabetic() {
        return Err("Drive letter must be a single letter (e.g. Z).".to_string());
    }

    let drive_spec = format!("{}:", letter);

    // Check if drive letter is already in use
    let check = Command::new("cmd")
        .args(["/C", &format!("if exist {}\\nul echo EXISTS", drive_spec)])
        .output()
        .map_err(|e| format!("Check failed: {}", e))?;

    if String::from_utf8_lossy(&check.stdout).contains("EXISTS") {
        return Err(format!("Drive {} is already in use.", drive_spec));
    }

    // Check if the folder exists
    if !std::path::Path::new(&path).exists() {
        return Err(format!("Folder not found: {}", path));
    }

    // Mount using subst
    let result = Command::new("subst")
        .args([&drive_spec, &path])
        .output()
        .map_err(|e| format!("subst failed: {}", e))?;

    if result.status.success() {
        Ok(format!("Mounted {} as drive {}", path, drive_spec))
    } else {
        let stderr = String::from_utf8_lossy(&result.stderr);
        Err(format!("Mount failed: {}", stderr))
    }
}

/// Unmount a virtual drive letter.
pub fn unmount_drive(letter: String) -> Result<String, String> {
    let letter = letter.trim().to_uppercase();
    let drive_spec = format!("{}:", letter);

    let result = Command::new("subst")
        .args([&drive_spec, "/D"])
        .output()
        .map_err(|e| format!("subst /D failed: {}", e))?;

    if result.status.success() {
        Ok(format!("Unmounted drive {}", drive_spec))
    } else {
        let stderr = String::from_utf8_lossy(&result.stderr);
        Err(format!("Unmount failed: {}", stderr))
    }
}

/// List all subst-mounted virtual drives.
pub fn list_virtual_drives() -> Result<Vec<MountedDrive>, String> {
    let output = Command::new("subst")
        .output()
        .map_err(|e| format!("subst list failed: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let mut drives = Vec::new();

    for line in stdout.lines() {
        // Format: "X:\: => C:\path\to\folder"
        if line.contains("=>") {
            let parts: Vec<&str> = line.splitn(2, "=>").collect();
            if parts.len() == 2 {
                let letter = parts[0].trim().trim_end_matches('\\').trim_end_matches(':').to_string();
                let path = parts[1].trim().to_string();
                drives.push(MountedDrive {
                    letter,
                    path,
                    status: "mounted".to_string(),
                });
            }
        }
    }

    Ok(drives)
}
