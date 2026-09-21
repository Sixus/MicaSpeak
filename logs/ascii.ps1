param([string]$Img, [int]$X0, [int]$Y0, [int]$X1, [int]$Y1, [int]$StepY, [int]$StepX)
Add-Type -AssemblyName System.Drawing
$bmp = [System.Drawing.Bitmap]::FromFile($Img)
$chars = ' .:-=+*#%@'
for ($y = $Y0; $y -lt $Y1; $y += $StepY) {
  $line = ''
  for ($x = $X0; $x -lt $X1; $x += $StepX) {
    $c = $bmp.GetPixel($x, $y)
    $lum = ($c.R + $c.G + $c.B) / 768.0
    if ($lum -gt 0.88) { $line += ' ' }
    else { $idx = [math]::Floor((1.0 - $lum) * 9.9); if ($idx -gt 9) { $idx = 9 }; $line += $chars[$idx] }
  }
  Write-Output ('{0,3} {1}' -f $y, $line)
}
$bmp.Dispose()
