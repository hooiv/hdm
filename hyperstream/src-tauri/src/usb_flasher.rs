use serde::Serialize;
use std::process::Command;

#[derive(Serialize, Clone, Debug)]
pub struct UsbDrive {
    pub number: u32,
    pub model: String,
    pub size_bytes: u64,
    pub size_display: String,
}

/// Enumerate removable USB drives on Windows via PowerShell.
pub fn list_usb_drives() -> Result<Vec<UsbDrive>, String> {
    // Use PowerShell to get removable disk info
    let output = Command::new("powershell")
        .args([
            "-NoProfile", "-Command",
            "Get-Disk | Where-Object { $_.BusType -eq 'USB' } | Select-Object Number, FriendlyName, Size | ConvertTo-Json -Compress"
        ])
        .output()
        .map_err(|e| format!("Failed to execute PowerShell: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    
    if stdout.trim().is_empty() {
        return Ok(Vec::new());
    }

    // Parse JSON output - could be an object (single) or array (multiple)
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .map_err(|e| format!("Failed to parse disk info: {} — raw: {}", e, stdout))?;

    let mut drives = Vec::new();

    let items = if parsed.is_array() {
        parsed.as_array().unwrap().clone()
    } else {
        vec![parsed]
    };

    for item in items {
        let number = item.get("Number").and_then(|n| n.as_u64()).unwrap_or(0) as u32;
        let model = item.get("FriendlyName").and_then(|n| n.as_str()).unwrap_or("Unknown").to_string();
        let size_bytes = item.get("Size").and_then(|n| n.as_u64()).unwrap_or(0);
        
        let size_display = if size_bytes > 1_000_000_000 {
            format!("{:.1} GB", size_bytes as f64 / 1_000_000_000.0)
        } else {
            format!("{:.1} MB", size_bytes as f64 / 1_000_000.0)
        };

        drives.push(UsbDrive {
            number,
            model,
            size_bytes,
            size_display,
        });
    }

    Ok(drives)
}

/// Flash an ISO/IMG file to a USB drive.
/// WARNING: This is a destructive operation that will erase the USB drive.
pub async fn flash_to_usb(iso_path: String, drive_number: u32) -> Result<String, String> {
    let path = std::path::Path::new(&iso_path);
    if !path.exists() {
        return Err(format!("File not found: {}", iso_path));
    }

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    if ext != "iso" && ext != "img" {
        return Err("Only .iso and .img files can be flashed.".to_string());
    }

    // Step 1: Clean the disk via diskpart
    let diskpart_script = format!(
        "select disk {}\nclean\ncreate partition primary\nformat fs=fat32 quick\nactive\nassign\nexit",
        drive_number
    );

    // Write diskpart script to temp file
    let temp_dir = std::env::temp_dir();
    let script_path = temp_dir.join("hyperstream_diskpart.txt");
    std::fs::write(&script_path, &diskpart_script)
        .map_err(|e| format!("Failed to write diskpart script: {}", e))?;

    // Run diskpart (requires admin)
    let diskpart_result = Command::new("diskpart")
        .args(["/s", &script_path.to_string_lossy()])
        .output()
        .map_err(|e| format!("diskpart failed: {}", e))?;

    if !diskpart_result.status.success() {
        let stderr = String::from_utf8_lossy(&diskpart_result.stderr);
        return Err(format!("diskpart failed: {}. Ensure HyperStream is run as Administrator.", stderr));
    }

    // Step 2: Write the ISO/IMG directly to the physical drive
    // Use PowerShell to copy the file content
    let ps_script = format!(
        r#"
        $source = [System.IO.File]::OpenRead('{}')
        $target = [System.IO.File]::OpenWrite('\\.\PhysicalDrive{}')
        $buffer = New-Object byte[] 1048576
        $totalRead = 0
        while (($bytesRead = $source.Read($buffer, 0, $buffer.Length)) -gt 0) {{
            $target.Write($buffer, 0, $bytesRead)
            $totalRead += $bytesRead
        }}
        $source.Close()
        $target.Close()
        Write-Output "Written $totalRead bytes"
        "#,
        iso_path.replace("\\", "\\\\"),
        drive_number
    );

    let flash_result = Command::new("powershell")
        .args(["-NoProfile", "-Command", &ps_script])
        .output()
        .map_err(|e| format!("Flash failed: {}", e))?;

    let stdout = String::from_utf8_lossy(&flash_result.stdout).to_string();

    if flash_result.status.success() {
        // Clean up temp file
        let _ = std::fs::remove_file(&script_path);
        Ok(format!("Successfully flashed to Drive {}. {}", drive_number, stdout.trim()))
    } else {
        let stderr = String::from_utf8_lossy(&flash_result.stderr);
        Err(format!("Flash write failed: {}. Ensure HyperStream is run as Administrator.", stderr))
    }
}
