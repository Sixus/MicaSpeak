param(
  [string]$Img,
  [int]$X0, [int]$Y0, [int]$X1, [int]$Y1,
  [int]$Row = -1   
)
Add-Type -AssemblyName System.Drawing
$bmp = [System.Drawing.Bitmap]::FromFile($Img)
# 1) 水平投影找文字行（该行内 ink 像素数 > 0）
$rows = @()
for ($y = $Y0; $y -lt $Y1; $y++) {
  $ink = 0
  for ($x = $X0; $x -lt $X1; $x++) {
    $c = $bmp.GetPixel($x, $y)
    $lum = ($c.R + $c.G + $c.B) / 3.0
    if ($lum -lt 200) { $ink++ }
  }
  $rows += ,@($y, $ink)
}
# 找连续 ink>2 的行段
$bands = @(); $cur = $null
for ($i = 0; $i -lt $rows.Count; $i++) {
  $y = $rows[$i][0]; $ink = $rows[$i][1]
  if ($ink -gt 1) {
    if ($null -eq $cur) { $cur = @($y, $y) } else { $cur[1] = $y }
  } else {
    if ($null -ne $cur) { $bands += ,$cur; $cur = $null }
  }
}
if ($null -ne $cur) { $bands += ,$cur }
Write-Output ("text rows: " + (($bands | ForEach-Object { "{0}-{1}" -f $_[0], $_[1] }) -join "  "))
if ($bands.Count -eq 0) { $bmp.Dispose(); exit }
$bandIdx = if ($Row -ge 0 -and $Row -lt $bands.Count) { $Row } else { 0 }
$yA = $bands[$bandIdx][0]; $yB = $bands[$bandIdx][1]
Write-Output ("measuring row band y=$yA..$yB")
# 2) 列投影：找 ink 列簇（相邻 gap>=3px 分簇）
$cols = @()
for ($x = $X0; $x -lt $X1; $x++) {
  $ink = 0
  for ($y = $yA; $y -le $yB; $y++) {
    $c = $bmp.GetPixel($x, $y)
    $lum = ($c.R + $c.G + $c.B) / 3.0
    if ($lum -lt 200) { $ink++ }
  }
  $cols += ,@($x, $ink)
}
$clusters = @(); $cc = $null; $gap = 0
for ($i = 0; $i -lt $cols.Count; $i++) {
  $x = $cols[$i][0]; $ink = $cols[$i][1]
  if ($ink -gt 0) {
    if ($null -eq $cc) { $cc = @($x, $x) } else { $cc[1] = $x }
    $gap = 0
  } else {
    if ($null -ne $cc) {
      $gap++
      if ($gap -ge 3) { $clusters += ,$cc; $cc = $null; $gap = 0 }
    }
  }
}
if ($null -ne $cc) { $clusters += ,$cc }
# 按簇输出 + 簇间距
for ($i = 0; $i -lt $clusters.Count; $i++) {
  $line = "cluster[{0}] x {1}-{2} (w{3})" -f $i, $clusters[$i][0], $clusters[$i][1], ($clusters[$i][1] - $clusters[$i][0] + 1)
  if ($i -gt 0) { $line += "   gap_from_prev={0}px" -f ($clusters[$i][0] - $clusters[$i-1][1] - 1) }
  Write-Output $line
}
$bmp.Dispose()
