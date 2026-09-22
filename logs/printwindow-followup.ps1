param([string]$OutDir = "D:\MicaSpeak\logs\printwindow-followup")
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class PrintWin {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  public static List<IntPtr> Handles = new List<IntPtr>();
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc cb, IntPtr lParam);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdcBlt, uint nFlags);
  public struct RECT { public int Left, Top, Right, Bottom; }
}
"@
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$pids = @(Get-Process micaspeak -ErrorAction SilentlyContinue | ForEach-Object { [uint32]$_.Id })
$callback = [PrintWin+EnumWindowsProc]{
  param($hWnd, $lParam)
  [uint32]$processId = 0
  [PrintWin]::GetWindowThreadProcessId($hWnd, [ref]$processId) | Out-Null
  if ($pids -contains $processId -and [PrintWin]::IsWindowVisible($hWnd)) { [PrintWin]::Handles.Add($hWnd) }
  return $true
}
[PrintWin]::EnumWindows($callback, [IntPtr]::Zero) | Out-Null
$i = 0
foreach ($hWnd in [PrintWin]::Handles) {
  $r = New-Object PrintWin+RECT
  [PrintWin]::GetWindowRect($hWnd, [ref]$r) | Out-Null
  $w = $r.Right - $r.Left; $h = $r.Bottom - $r.Top
  if ($w -le 0 -or $h -le 0) { continue }
  $bmp = New-Object System.Drawing.Bitmap($w, $h)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $hdc = $g.GetHdc()
  $ok = [PrintWin]::PrintWindow($hWnd, $hdc, 2)
  $g.ReleaseHdc($hdc)
  $out = Join-Path $OutDir ("window{0}.png" -f $i)
  $bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
  Write-Output "$ok $out ${w}x${h}"
  $i++
}
