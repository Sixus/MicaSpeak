param([string]$Src, [int]$X, [int]$Y, [int]$W, [int]$H, [double]$Scale, [string]$Out)
Add-Type -AssemblyName System.Drawing
$bmp0 = [System.Drawing.Image]::FromFile($Src)
$bw = [int]($W * $Scale)
$bh = [int]($H * $Scale)
$big = New-Object System.Drawing.Bitmap($bw, $bh)
$g = [System.Drawing.Graphics]::FromImage($big)
$g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::NearestNeighbor
$srcRect = New-Object System.Drawing.Rectangle($X, $Y, $W, $H)
$dstRect = New-Object System.Drawing.Rectangle(0, 0, $bw, $bh)
$g.DrawImage($bmp0, $dstRect, $srcRect, [System.Drawing.GraphicsUnit]::Pixel)
$big.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp0.Dispose()
Write-Output "saved $Out"
