param([string]$Out = "D:\MicaSpeak\logs\win.png")
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Win32 {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  public struct RECT { public int Left, Top, Right, Bottom; }
}
"@
$p = Get-Process micaspeak -ErrorAction Stop | Select-Object -First 1
if ($p -eq $null -or $p.MainWindowHandle -eq 0) { Write-Error "no window"; exit 1 }
[Win32]::SetForegroundWindow($p.MainWindowHandle) | Out-Null
Start-Sleep -Milliseconds 600
$rect = New-Object Win32+RECT
[Win32]::GetWindowRect($p.MainWindowHandle, [ref]$rect) | Out-Null
$w = $rect.Right - $rect.Left; $h = $rect.Bottom - $rect.Top
Write-Output "window rect: $($rect.Left),$($rect.Top) ${w}x${h}"
$bmp = New-Object System.Drawing.Bitmap($w, $h)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($rect.Left, $rect.Top, 0, 0, (New-Object System.Drawing.Size($w, $h)))
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
Write-Output "saved $Out"
