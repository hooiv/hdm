Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

Add-Type @"
using System;
using System.Runtime.InteropServices;
public class WinAPI {
    [DllImport("user32.dll")]
    public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")]
    [return: MarshalAs(UnmanagedType.Bool)]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
    [DllImport("user32.dll")]
    public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
}
"@

Start-Sleep 2
$proc = Get-Process hyperstream -ErrorAction SilentlyContinue
if (-not $proc) {
    Write-Host "HyperStream not running"
    exit 1
}

Write-Host "HyperStream PID: $($proc.Id)"
$hwnd = $proc.MainWindowHandle

if ($hwnd -eq [IntPtr]::Zero) {
    Write-Host "No main window handle found"
    exit 1
}

# Bring window to front
[WinAPI]::ShowWindow($hwnd, 9) | Out-Null  # SW_RESTORE
Start-Sleep 1
[WinAPI]::SetForegroundWindow($hwnd) | Out-Null
Start-Sleep 1

$rect = New-Object WinAPI+RECT
[WinAPI]::GetWindowRect($hwnd, [ref]$rect) | Out-Null
$w = $rect.Right - $rect.Left
$h = $rect.Bottom - $rect.Top
Write-Host "Window: ${w}x${h} at ($($rect.Left),$($rect.Top))"

if ($w -le 0 -or $h -le 0) {
    Write-Host "Invalid window dimensions"
    exit 1
}

$bmp = New-Object System.Drawing.Bitmap($w, $h)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($rect.Left, $rect.Top, 0, 0, (New-Object System.Drawing.Size($w, $h)))
$bmp.Save("c:\Users\aditya\Desktop\hdm\hyperstream\screenshots\app_main.png")
$g.Dispose()
$bmp.Dispose()
Write-Host "Screenshot saved to screenshots\app_main.png"
