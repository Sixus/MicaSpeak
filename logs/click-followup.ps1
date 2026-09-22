param([int]$X, [int]$Y)
Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class MouseClick {
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int maxCount);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc cb, IntPtr lParam);
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
}
"@
$target = [IntPtr]::Zero
$cb = [MouseClick+EnumWindowsProc]{
  param($hWnd, $lParam)
  $text = New-Object System.Text.StringBuilder 256
  [MouseClick]::GetWindowText($hWnd, $text, 256) | Out-Null
  if ($text.ToString() -eq "MicaSpeak 设置") { $script:target = $hWnd; return $false }
  return $true
}
[MouseClick]::EnumWindows($cb, [IntPtr]::Zero) | Out-Null
if ($target -ne [IntPtr]::Zero) { [MouseClick]::SetForegroundWindow($target) | Out-Null; Start-Sleep -Milliseconds 150 }
[MouseClick]::SetCursorPos($X, $Y) | Out-Null
[MouseClick]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
[MouseClick]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
