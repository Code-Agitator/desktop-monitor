param([long]$Hwnd)
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$win = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$Hwnd)
$cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Edit)
$items = $win.FindAll([System.Windows.Automation.TreeScope]::Descendants, $cond)
$i = 0
foreach ($e in $items) {
  $v = ""
  try { $v = $e.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value } catch {}
  "edit[$i] name='$($e.Current.Name)' value='$v'"
  $i++
}
