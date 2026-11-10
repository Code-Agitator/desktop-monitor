param([string]$Out = "shot.png", [switch]$Minimize)
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public class DPIH { [DllImport("user32.dll")] public static extern bool SetProcessDPIAware(); }'
[DPIH]::SetProcessDPIAware() | Out-Null
$shell = New-Object -ComObject Shell.Application
if ($Minimize) { $shell.MinimizeAll(); Start-Sleep -Milliseconds 1200 }
$b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($b.X, $b.Y, 0, 0, $bmp.Size)
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
if ($Minimize) { $shell.UndoMinimizeAll() }
Write-Output "saved $Out"
