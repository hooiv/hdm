use std::process::Command;
use std::path::Path;

pub fn set_app_firewall_rule(exe_path: &str, blocked: bool) -> Result<String, String> {
    // Validate exe_path — must be an existing file with .exe extension
    let path = Path::new(exe_path);
    if !path.is_file() {
        return Err(format!("File does not exist: {}", exe_path));
    }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    if ext != "exe" {
        return Err("Only .exe files can have firewall rules applied".to_string());
    }
    // Reject paths with suspicious characters that could confuse netsh argument parsing
    if exe_path.contains('"') || exe_path.contains('`') || exe_path.contains('$') || exe_path.contains('\n') || exe_path.contains('\r') || exe_path.contains('(') || exe_path.contains(')') {
        return Err("Invalid characters in file path".to_string());
    }

    // Requires Windows Administrator privileges to run `netsh advfirewall`
    let rule_name = format!("HyperStream_WFP_Block_{}", exe_path.replace("\\", "_").replace(":", "").replace(" ", "_"));
    
    // 1. Delete existing rule if it exists to avoid duplicates
    let _ = Command::new("netsh")
        .args(&["advfirewall", "firewall", "delete", "rule", &format!("name=\"{}\"", rule_name)])
        .output();

    if blocked {
        // 2. Add block rule
        let output = Command::new("netsh")
            .args(&[
                "advfirewall", "firewall", "add", "rule",
                &format!("name={}", rule_name),
                "dir=out",
                "action=block",
                &format!("program={}", exe_path),
                "enable=yes"
            ])
            .output()
            .map_err(|e| format!("Command execution failed: {}", e))?;

        if output.status.success() {
            Ok(format!("Successfully blocked {} via Windows Filtering Platform (netsh)", exe_path))
        } else {
            Err(format!("Netsh failed (requires Admin privileges): {}", String::from_utf8_lossy(&output.stderr)))
        }
    } else {
        Ok(format!("Unblocked {}. Firewall rule removed.", exe_path))
    }
}
