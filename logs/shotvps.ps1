param([string]$OutDir = "D:\MicaSpeak\logs")
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public class WinEnum {
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc cb, IntPtr lp);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder sb, int n);
  public delegate bool EnumWindowsProc(IntPtr h, IntPtr lp);
  public static List<string> Results = new List<string>();
  public static List<IntPtr> Handles = new List<IntPtr>();
}
public struct RECT { public int Left, Top, Right, Bottom; }
"@
$procs = @(Get-Process micaspeak -ErrorAction SilentlyContinue)
if ($procs.Count -eq 0) { Write-Error "no micaspeak process"; exit 1 }
$pids = $procs | ForEach-Object { $_.Id }
$cb = {
  param($h, $lp)
  $wpid = 0
  [WinEnum]::GetWindowThreadProcessId($h, [ref]$wpid) | Out-Null
  if ($pids -contains $wpid -and [WinEnum]::IsWindowVisible($h)) {
    $r = New-Object RECT
    [WinEnum]::GetWindowRect($h, [ref]$r) | Out-Null
    $sb = New-Object System.Text.StringBuilder 256
    [WinEnum]::GetWindowText($h, $sb, 256) | Out-Null
    $w = $r.Right - $r.Left; $hgt = $r.Bottom - $r.Top
    if ($w -gt 50 -and $hgt -gt 50) {
      [WinEnum]::Results.Add("$($r.Left),$($r.Top) ${w}x${hgt} title=$($sb.ToString())")
      [WinEnum]::Handles.Add($h)
    }
  }
  return $true
}
[WinEnum]::EnumWindows($cb, [IntPtr]::Zero) | Out-Null
$i = 0
foreach ($h in [WinEnum]::Handles) {
  $r = New-Object RECT
  [WinEnum]::GetWindowRect($h, [ref]$r) | Out-Null
  $w = $r.Right - $r.Left; $hgt = $r.Bottom - $r.Top
  $bmp = New-Object System.Drawing.Bitmap($w, $hgt)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($r.Left, $r.Top, 0, 0, (New-Object System.Drawing.Size($w, $hgt)))
  $out = Join-Path $OutDir ("vp{0}.png" -f $i)
  $bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
  Write-Output ("saved $out : " + [WinEnum]::Results[$i])
  $i++
}
