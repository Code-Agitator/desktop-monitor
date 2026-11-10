param([long]$Hud)
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public class WV2 { [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h); }
'@
$form = New-Object System.Windows.Forms.Form
$form.FormBorderStyle = 'None'
$form.StartPosition = 'Manual'
$form.Bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$form.TopMost = $true
$form.BackColor = [System.Drawing.Color]::Black
$form.Show(); $form.Activate()
$sw = [Diagnostics.Stopwatch]::StartNew()
$firstHidden = -1
while ($sw.ElapsedMilliseconds -lt 4000) {
  [System.Windows.Forms.Application]::DoEvents()
  if ($firstHidden -lt 0 -and -not [WV2]::IsWindowVisible([IntPtr]$Hud)) { $firstHidden = $sw.ElapsedMilliseconds }
  Start-Sleep -Milliseconds 30
}
"first_hidden_ms=$firstHidden still_hidden=$(-not [WV2]::IsWindowVisible([IntPtr]$Hud))"
$form.Close()
$sw.Restart(); $firstShown = -1
while ($sw.ElapsedMilliseconds -lt 3000) {
  [System.Windows.Forms.Application]::DoEvents()
  if ($firstShown -lt 0 -and [WV2]::IsWindowVisible([IntPtr]$Hud)) { $firstShown = $sw.ElapsedMilliseconds }
  Start-Sleep -Milliseconds 30
}
"first_shown_ms=$firstShown"
