use serde::{Serialize, Deserialize};

/// Virus scan result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScanResult {
    Clean,
    Infected { threat_name: String },
    Error { message: String },
    NotScanned,
}

/// Virus scanner interface
pub struct VirusScanner {
    enabled: bool,
}

#[allow(dead_code)]
impl VirusScanner {
    pub fn new() -> Self {
        Self { enabled: true }
    }

    /// Check if Windows Defender is available
    pub fn is_available(&self) -> bool {
        #[cfg(windows)]
        {
            // Check if Windows Defender is available via MpCmdRun.exe
            let defender_path = std::path::Path::new(
                r"C:\Program Files\Windows Defender\MpCmdRun.exe"
            );
            defender_path.exists()
        }
        #[cfg(not(windows))]
        {
            // Check for ClamAV on Linux/macOS
            which::which("clamscan").is_ok()
        }
    }

    /// Scan a file using the system's antivirus
    pub async fn scan_file(&self, path: &std::path::Path) -> ScanResult {
        if !self.enabled {
            return ScanResult::NotScanned;
        }

        #[cfg(windows)]
        {
            self.scan_with_defender(path).await
        }
        #[cfg(not(windows))]
        {
            self.scan_with_clamav(path).await
        }
    }

    #[cfg(windows)]
    async fn scan_with_defender(&self, path: &std::path::Path) -> ScanResult {
        use tokio::process::Command;
        
        let output = Command::new(r"C:\Program Files\Windows Defender\MpCmdRun.exe")
            .args(&["-Scan", "-ScanType", "3", "-File"])
            .arg(path)
            .output()
            .await;

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if output.status.success() || stdout.contains("found no threats") {
                    ScanResult::Clean
                } else if stdout.contains("Threat") {
                    ScanResult::Infected {
                        threat_name: "Threat detected by Windows Defender".to_string(),
                    }
                } else {
                    ScanResult::Error {
                        message: format!("Unexpected output: {}", stdout),
                    }
                }
            }
            Err(e) => ScanResult::Error {
                message: format!("Failed to run scan: {}", e),
            },
        }
    }

    #[cfg(not(windows))]
    async fn scan_with_clamav(&self, path: &std::path::Path) -> ScanResult {
        use tokio::process::Command;
        
        let output = Command::new("clamscan")
            .arg("--no-summary")
            .arg(path)
            .output()
            .await;

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if output.status.code() == Some(0) {
                    ScanResult::Clean
                } else if output.status.code() == Some(1) {
                    ScanResult::Infected {
                        threat_name: stdout.to_string(),
                    }
                } else {
                    ScanResult::Error {
                        message: stdout.to_string(),
                    }
                }
            }
            Err(e) => ScanResult::Error {
                message: format!("Failed to run clamscan: {}", e),
            },
        }
    }

    pub fn enable(&mut self) {
        self.enabled = true;
    }

    pub fn disable(&mut self) {
        self.enabled = false;
    }
}

impl Default for VirusScanner {
    fn default() -> Self {
        Self::new()
    }
}
