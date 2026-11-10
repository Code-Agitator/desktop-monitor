param([long]$Hwnd)
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$win = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$Hwnd)
if ($win -eq $null) { "no window"; return }
$cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button)
$buttons = $win.FindAll([System.Windows.Automation.TreeScope]::Descendants, $cond)
"buttons=$($buttons.Count)"
foreach ($b in $buttons) {
  try {
    $r = $b.Current.BoundingRectangle
    "name='" + $b.Current.Name + "' at (" + [int]$r.X + "," + [int]$r.Y + ") " + [int]$r.Width + "x" + [int]$r.Height
  } catch { }
}
