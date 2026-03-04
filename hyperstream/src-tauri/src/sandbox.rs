use dunce;

/// Default sandbox folder path inside Windows Sandbox (WDAGUtilityAccount is the standard sandbox user).
#[cfg(target_os = "windows")]
const SANDBOX_MAPPED_FOLDER: &str = r"C:\Users\WDAGUtilityAccount\Desktop\Downloads";

/// Generates a Windows Sandbox configuration file (.wsb) and launches it.
/// The sandbox maps the download directory as a read-only folder and
/// auto-executes the specified file on startup.
#[cfg(target_os = "windows")]
pub fn run_in_sandbox(executable_path: String) -> Result<String, String> {
    // Canonicalize first to resolve any '..' traversal and symlinks,
    // preventing an attacker from mapping arbitrary host directories into the sandbox.
    let exe_path = dunce::canonicalize(&executable_path)
        .map_err(|e| format!("Cannot resolve path '{}': {}", executable_path, e))?;

    if !exe_path.exists() {
        return Err(format!("File not found: {}", executable_path));
    }

    let extension = exe_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if extension != "exe" && extension != "msi" {
        return Err("Only .exe and .msi files can be run in Windows Sandbox.".to_string());
    }

    // Restrict to files within the user's download directory to prevent
    // mapping sensitive host directories (e.g. System32) into the sandbox.
    let settings = crate::settings::load_settings();
    let download_dir = dunce::canonicalize(&settings.download_dir)
        .map_err(|e| format!("Cannot resolve download dir: {}", e))?;
    if !exe_path.starts_with(&download_dir) {
        return Err("File must be within the download directory to run in sandbox.".to_string());
    }

    // Get the parent directory to map as a shared folder (already canonical)
    let host_folder = exe_path
        .parent()
        .ok_or("Cannot determine parent directory")?
        .to_string_lossy()
        .to_string();

    let file_name = exe_path
        .file_name()
        .ok_or("Cannot determine filename")?
        .to_string_lossy()
        .to_string();

    let sandbox_path = format!("{}\\{}", SANDBOX_MAPPED_FOLDER, file_name);

    let logon_command = if extension == "msi" {
        format!("msiexec /i \"{}\"", sandbox_path)
    } else {
        format!("\"{}\"", sandbox_path)
    };

    // XML-escape interpolated values to prevent injection
    fn xml_escape(s: &str) -> String {
        s.replace('&', "&amp;")
         .replace('<', "&lt;")
         .replace('>', "&gt;")
         .replace('"', "&quot;")
         .replace('\'', "&apos;")
    }

    let host_folder_escaped = xml_escape(&host_folder);
    let logon_command_escaped = xml_escape(&logon_command);

    // Build the .wsb XML configuration
    let wsb_content = format!(
        r#"<Configuration>
  <MappedFolders>
    <MappedFolder>
      <HostFolder>{}</HostFolder>
      <SandboxFolder>{}</SandboxFolder>
      <ReadOnly>true</ReadOnly>
    </MappedFolder>
  </MappedFolders>
  <LogonCommand>
    <Command>{}</Command>
  </LogonCommand>
  <Networking>Enable</Networking>
  <vGPU>Enable</vGPU>
  <MemoryInMB>4096</MemoryInMB>
</Configuration>"#,
        host_folder_escaped, SANDBOX_MAPPED_FOLDER, logon_command_escaped
    );

    // Write to a temp .wsb file
    let temp_dir = std::env::temp_dir();
    let wsb_path = temp_dir.join("hyperstream_sandbox.wsb");

    std::fs::write(&wsb_path, wsb_content)
        .map_err(|e| format!("Failed to write .wsb file: {}", e))?;

    // Launch the sandbox
    std::process::Command::new("explorer")
        .arg(wsb_path.to_string_lossy().to_string())
        .spawn()
        .map_err(|e| format!("Failed to launch Windows Sandbox: {}", e))?;

    Ok(format!("Windows Sandbox launched for: {}", file_name))
}

#[cfg(not(target_os = "windows"))]
pub fn run_in_sandbox(_executable_path: String) -> Result<String, String> {
    Err("Windows Sandbox is only available on Windows 10/11 Pro or Enterprise.".to_string())
}
