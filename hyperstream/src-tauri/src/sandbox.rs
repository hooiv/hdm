use std::path::Path;

/// Generates a Windows Sandbox configuration file (.wsb) and launches it.
/// The sandbox maps the download directory as a read-only folder and
/// auto-executes the specified file on startup.
pub fn run_in_sandbox(executable_path: String) -> Result<String, String> {
    let exe_path = Path::new(&executable_path);

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

    // Get the parent directory to map as a shared folder
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

    // The sandbox maps the host folder to C:\Users\WDAGUtilityAccount\Desktop\Downloads
    let sandbox_path = format!("C:\\Users\\WDAGUtilityAccount\\Desktop\\Downloads\\{}", file_name);

    let logon_command = if extension == "msi" {
        format!("msiexec /i \"{}\"", sandbox_path)
    } else {
        format!("\"{}\"", sandbox_path)
    };

    // Build the .wsb XML configuration
    let wsb_content = format!(
        r#"<Configuration>
  <MappedFolders>
    <MappedFolder>
      <HostFolder>{}</HostFolder>
      <SandboxFolder>C:\Users\WDAGUtilityAccount\Desktop\Downloads</SandboxFolder>
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
        host_folder, logon_command
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
