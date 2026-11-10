param([string]$Src, [string]$Out="zoom.png")
Add-Type -AssemblyName System.Drawing
$img = [System.Drawing.Image]::FromFile($Src)
$f = $img.Width / 835.0
$x = [int]((532 - 12) * $f); $y = [int]((619 - 14) * $f)
$w = [int]((777 - 532 + 24) * $f); $h = [int]((662 - 619 + 28) * $f)
$scale = 4
$bmp = New-Object System.Drawing.Bitmap ($w*$scale), ($h*$scale)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::NearestNeighbor
$g.DrawImage($img, (New-Object System.Drawing.Rectangle 0,0,($w*$scale),($h*$scale)), (New-Object System.Drawing.Rectangle $x,$y,$w,$h), [System.Drawing.GraphicsUnit]::Pixel)
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose(); $img.Dispose()
"$($img.Width)x$($img.Height) -> crop $x,$y ${w}x${h}"
