param([long]$Hwnd)
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$win = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$Hwnd)
$cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button)
$buttons = $win.FindAll([System.Windows.Automation.TreeScope]::Descendants, $cond)
foreach ($b in $buttons) {
  if ($b.Current.Name.StartsWith("+")) {
    $p = $b.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
    $p.Invoke()
    "invoked " + $b.Current.Name
    return
  }
}
"button not found"
