Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$form = New-Object System.Windows.Forms.Form
$form.FormBorderStyle = 'None'
$form.StartPosition = 'Manual'
$form.Bounds = New-Object System.Drawing.Rectangle 1900, 1050, 660, 390
$form.BackColor = [System.Drawing.Color]::White
$form.TopMost = $false
$form.Show()
$sw = [Diagnostics.Stopwatch]::StartNew()
while ($sw.ElapsedMilliseconds -lt 1200) { [System.Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 30 }
$bmp = New-Object System.Drawing.Bitmap 440, 230
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen(2010, 1140, 0, 0, (New-Object System.Drawing.Size 440, 230))
$bmp.Save("D:\Work\desktop-monitor\docs\hud-white-bg.png", [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
$form.Close()
"captured"
