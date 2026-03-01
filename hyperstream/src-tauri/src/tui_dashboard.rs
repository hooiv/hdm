use std::process::Command;

/// Launch a TUI dashboard in a new terminal window showing live download stats.
/// Uses PowerShell to create an auto-refreshing monitoring display.
pub fn launch_tui_dashboard() -> Result<String, String> {
    let ps_script = r#"
$host.UI.RawUI.WindowTitle = 'HyperStream TUI Dashboard'
$host.UI.RawUI.BackgroundColor = 'Black'
$host.UI.RawUI.ForegroundColor = 'Cyan'
Clear-Host

function Get-DownloadStats {
    $dataDir = "$env:APPDATA\com.hyperstream.dev"
    $dlFile = Join-Path $dataDir "downloads.json"
    if (Test-Path $dlFile) {
        try {
            $data = Get-Content $dlFile -Raw | ConvertFrom-Json
            return $data
        } catch {
            return @()
        }
    }
    return @()
}

while ($true) {
    Clear-Host
    Write-Host ""
    Write-Host "  ╔══════════════════════════════════════════════════════════════╗" -ForegroundColor Cyan
    Write-Host "  ║           ⚡ HYPERSTREAM TUI DASHBOARD ⚡                  ║" -ForegroundColor Cyan
    Write-Host "  ╚══════════════════════════════════════════════════════════════╝" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "  $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')  |  Press Ctrl+C to exit" -ForegroundColor DarkGray
    Write-Host "  ────────────────────────────────────────────────────────────────" -ForegroundColor DarkGray
    Write-Host ""

    $downloads = Get-DownloadStats

    if ($downloads.Count -eq 0) {
        Write-Host "  No downloads found." -ForegroundColor Yellow
    } else {
        $active = @($downloads | Where-Object { $_.status -eq 'Downloading' })
        $done = @($downloads | Where-Object { $_.status -eq 'Completed' })
        $paused = @($downloads | Where-Object { $_.status -eq 'Paused' })
        $failed = @($downloads | Where-Object { $_.status -eq 'Error' })

        Write-Host "  📊 SUMMARY" -ForegroundColor White
        Write-Host "  ──────────" -ForegroundColor DarkGray
        Write-Host "  Active: $($active.Count)  |  Done: $($done.Count)  |  Paused: $($paused.Count)  |  Failed: $($failed.Count)" -ForegroundColor Gray
        Write-Host ""

        if ($active.Count -gt 0) {
            Write-Host "  🔽 ACTIVE DOWNLOADS" -ForegroundColor Green
            Write-Host "  ──────────────────────────────────────────" -ForegroundColor DarkGray
            foreach ($dl in $active) {
                $pct = if ($dl.total_size -gt 0) { [math]::Round(($dl.downloaded_bytes / $dl.total_size) * 100, 1) } else { 0 }
                $bar_length = 30
                $filled = [math]::Floor($pct / 100 * $bar_length)
                $empty = $bar_length - $filled
                $bar = ('█' * $filled) + ('░' * $empty)
                $size_mb = [math]::Round($dl.downloaded_bytes / 1MB, 1)
                $total_mb = [math]::Round($dl.total_size / 1MB, 1)
                Write-Host "  $($dl.filename)" -ForegroundColor White -NoNewline
                Write-Host ""
                Write-Host "  [$bar] ${pct}%  (${size_mb}MB / ${total_mb}MB)" -ForegroundColor Cyan
                Write-Host ""
            }
        }

        if ($done.Count -gt 0) {
            Write-Host "  ✅ COMPLETED (last 5)" -ForegroundColor Green
            Write-Host "  ──────────────────────────────────────────" -ForegroundColor DarkGray
            foreach ($dl in ($done | Select-Object -Last 5)) {
                $size_mb = [math]::Round($dl.total_size / 1MB, 1)
                Write-Host "  ✓ $($dl.filename)  (${size_mb}MB)" -ForegroundColor DarkGreen
            }
            Write-Host ""
        }

        if ($failed.Count -gt 0) {
            Write-Host "  ❌ FAILED" -ForegroundColor Red
            Write-Host "  ──────────────────────────────────────────" -ForegroundColor DarkGray
            foreach ($dl in $failed) {
                Write-Host "  ✗ $($dl.filename)" -ForegroundColor DarkRed
            }
            Write-Host ""
        }
    }

    # System info
    Write-Host "  ────────────────────────────────────────────────────────────────" -ForegroundColor DarkGray
    $cpu = (Get-Counter '\Processor(_Total)\% Processor Time' -ErrorAction SilentlyContinue).CounterSamples.CookedValue
    $mem = [math]::Round((Get-Process -Id $PID).WorkingSet64 / 1MB, 1)
    if ($cpu) {
        Write-Host "  CPU: $([math]::Round($cpu, 1))%  |  Dashboard Memory: ${mem}MB" -ForegroundColor DarkGray
    }

    Start-Sleep -Seconds 2
}
"#;

    // Write the script to a temp file
    let temp_dir = std::env::temp_dir();
    let script_path = temp_dir.join("hyperstream_tui.ps1");
    std::fs::write(&script_path, ps_script)
        .map_err(|e| format!("Failed to write TUI script: {}", e))?;

    // Launch in a new terminal window
    Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy", "Bypass",
            "-File", &script_path.to_string_lossy(),
        ])
        .spawn()
        .map_err(|e| format!("Failed to launch TUI: {}", e))?;

    Ok("TUI Dashboard launched in a new terminal window.".to_string())
}
