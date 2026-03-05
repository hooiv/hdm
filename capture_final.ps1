Add-Type @"
using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;
using System.Text;
using System.Collections.Generic;

public class WinCapture {
    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern int GetWindowTextLength(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdcBlt, uint nFlags);

    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }

    public static List<IntPtr> windowHandles = new List<IntPtr>();
    public static uint targetPid;

    public static bool EnumCallback(IntPtr hWnd, IntPtr lParam) {
        uint pid;
        GetWindowThreadProcessId(hWnd, out pid);
        if (pid == targetPid && IsWindowVisible(hWnd)) {
            windowHandles.Add(hWnd);
        }
        return true;
    }

    public static IntPtr FindLargestWindow(uint pid) {
        targetPid = pid;
        windowHandles.Clear();
        EnumWindows(EnumCallback, IntPtr.Zero);

        IntPtr best = IntPtr.Zero;
        int bestArea = 0;

        foreach (var h in windowHandles) {
            RECT r;
            GetWindowRect(h, out r);
            int area = (r.Right - r.Left) * (r.Bottom - r.Top);
            if (area > bestArea) {
                bestArea = area;
                best = h;
            }
        }
        return best;
    }

    public static void CaptureWindow(IntPtr hWnd, string path) {
        RECT r;
        GetWindowRect(hWnd, out r);
        int w = r.Right - r.Left;
        int h = r.Bottom - r.Top;
        if (w < 100) w = 1024;
        if (h < 100) h = 768;

        using (Bitmap bmp = new Bitmap(w, h)) {
            using (Graphics g = Graphics.FromImage(bmp)) {
                IntPtr hdc = g.GetHdc();
                PrintWindow(hWnd, hdc, 2);
                g.ReleaseHdc(hdc);
            }
            bmp.Save(path, ImageFormat.Png);
        }
    }
}
"@ -ReferencedAssemblies System.Drawing

$procs = Get-Process hyperstream -ErrorAction SilentlyContinue
if (-not $procs) {
    Write-Host "No hyperstream process found"
    exit 1
}
$procId = $procs[0].Id
Write-Host "Found hyperstream PID: $procId"

$hWnd = [WinCapture]::FindLargestWindow($procId)
if ($hWnd -eq [IntPtr]::Zero) {
    Write-Host "No visible window found"
    exit 1
}

$outPath = "C:\Users\aditya\Desktop\hdm\screenshot_final.png"
[WinCapture]::CaptureWindow($hWnd, $outPath)
Write-Host "Screenshot saved to $outPath"
