Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$source = @"
using System;
using System.Runtime.InteropServices;
using System.Collections.Generic;
using System.Text;
using System.Drawing;

public class WindowCapture {
    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);

    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);

    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr hWnd);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetClassName(IntPtr hWnd, StringBuilder lpClassName, int nMaxCount);

    [DllImport("user32.dll")]
    public static extern bool SetForegroundWindow(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);

    [DllImport("user32.dll")]
    public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdcBlt, uint nFlags);

    [StructLayout(LayoutKind.Sequential)]
    public struct RECT {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }

    public static List<IntPtr> FindWindowsByPid(uint targetPid) {
        var result = new List<IntPtr>();
        EnumWindows((hWnd, lParam) => {
            uint pid;
            GetWindowThreadProcessId(hWnd, out pid);
            if (pid == targetPid && IsWindowVisible(hWnd)) {
                result.Add(hWnd);
            }
            return true;
        }, IntPtr.Zero);
        return result;
    }

    public static string GetTitle(IntPtr hWnd) {
        var sb = new StringBuilder(256);
        GetWindowText(hWnd, sb, 256);
        return sb.ToString();
    }

    public static string GetClass(IntPtr hWnd) {
        var sb = new StringBuilder(256);
        GetClassName(hWnd, sb, 256);
        return sb.ToString();
    }

    public static Bitmap CaptureWindow(IntPtr hWnd) {
        RECT rect;
        GetWindowRect(hWnd, out rect);
        int w = rect.Right - rect.Left;
        int h = rect.Bottom - rect.Top;
        if (w <= 0 || h <= 0) return null;
        var bmp = new Bitmap(w, h);
        using (var g = Graphics.FromImage(bmp)) {
            IntPtr hdc = g.GetHdc();
            PrintWindow(hWnd, hdc, 2); // PW_RENDERFULLCONTENT = 2
            g.ReleaseHdc(hdc);
        }
        return bmp;
    }
}
"@

Add-Type -TypeDefinition $source -ReferencedAssemblies System.Drawing -Language CSharp

# Find hyperstream process
$proc = Get-Process -Name "hyperstream" -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $proc) {
    Write-Output "hyperstream process not found"
    exit 1
}

Write-Output "Process PID: $($proc.Id)"

# Find all visible windows for this process
$windows = [WindowCapture]::FindWindowsByPid([uint32]$proc.Id)
Write-Output "Found $($windows.Count) visible windows"

$bestHwnd = [IntPtr]::Zero
$bestArea = 0

foreach ($hwnd in $windows) {
    $rect = New-Object WindowCapture+RECT
    [WindowCapture]::GetWindowRect($hwnd, [ref]$rect) | Out-Null
    $w = $rect.Right - $rect.Left
    $h = $rect.Bottom - $rect.Top
    $title = [WindowCapture]::GetTitle($hwnd)
    $class = [WindowCapture]::GetClass($hwnd)
    $area = $w * $h
    Write-Output "  Window $hwnd : ${w}x${h} class='$class' title='$title' area=$area"
    if ($area -gt $bestArea) {
        $bestArea = $area
        $bestHwnd = $hwnd
    }
}

if ($bestHwnd -ne [IntPtr]::Zero -and $bestArea -gt 1000) {
    Write-Output "Capturing largest window: $bestHwnd (area=$bestArea)"

    # Bring to front first
    [WindowCapture]::ShowWindow($bestHwnd, 9) | Out-Null  # SW_RESTORE
    Start-Sleep -Milliseconds 300
    [WindowCapture]::SetForegroundWindow($bestHwnd) | Out-Null
    Start-Sleep -Milliseconds 500

    # Capture using PrintWindow
    $bmp = [WindowCapture]::CaptureWindow($bestHwnd)
    if ($bmp) {
        $bmp.Save("C:\Users\aditya\Desktop\hdm\screenshot_tauri.png", [System.Drawing.Imaging.ImageFormat]::Png)
        Write-Output "Saved screenshot_tauri.png ($($bmp.Width)x$($bmp.Height))"
        $bmp.Dispose()
    } else {
        Write-Output "CaptureWindow returned null"
    }

    # Also take a desktop screenshot after bringing to front
    Start-Sleep -Milliseconds 300
    $bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $deskBmp = New-Object System.Drawing.Bitmap($bounds.Width, $bounds.Height)
    $g = [System.Drawing.Graphics]::FromImage($deskBmp)
    $g.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
    $deskBmp.Save("C:\Users\aditya\Desktop\hdm\screenshot_app_front.png", [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose()
    $deskBmp.Dispose()
    Write-Output "Saved desktop screenshot too"
} else {
    Write-Output "No suitable window found (bestArea=$bestArea)"
}
