Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

# Method: Find Tauri WebView2 window by process name
$processes = Get-Process -Name "hyperstream" -ErrorAction SilentlyContinue
if (-not $processes) {
    $processes = Get-Process -Name "hyperstream.exe" -ErrorAction SilentlyContinue
}

Write-Output "Found processes: $($processes.Count)"

foreach ($proc in $processes) {
    $hwnd = $proc.MainWindowHandle
    Write-Output "Process $($proc.Id): MainWindow='$($proc.MainWindowTitle)' Handle=$hwnd"
}

# Try to screenshot via SendKeys (Alt+PrintScreen) on the active Tauri window
# Alternative: Just capture the whole screen — the app should be the topmost window.
# First, bring the Tauri window to front
$procs = Get-Process -Name "hyperstream" -ErrorAction SilentlyContinue
if ($procs) {
    foreach ($p in $procs) {
        if ($p.MainWindowHandle -ne 0) {
            $sig = '[DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);'
            $type = Add-Type -MemberDefinition $sig -Name 'WinAPI' -Namespace 'Capture' -PassThru -ErrorAction SilentlyContinue
            if ($type) {
                $type::SetForegroundWindow($p.MainWindowHandle) | Out-Null
                Start-Sleep -Milliseconds 500
            }
        }
    }
}

# Now screenshot
$bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$bitmap = New-Object System.Drawing.Bitmap($bounds.Width, $bounds.Height)
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
$bitmap.Save("C:\Users\aditya\Desktop\hdm\screenshot_app.png", [System.Drawing.Imaging.ImageFormat]::Png)
$graphics.Dispose()
$bitmap.Dispose()
Write-Output "Screenshot saved"
