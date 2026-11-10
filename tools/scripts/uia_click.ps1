param([long]$Hwnd, [int]$Index = 0)
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$win = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$Hwnd)
$cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button)
$buttons = $win.FindAll([System.Windows.Automation.TreeScope]::Descendants, $cond)
if ($Index -ge $buttons.Count) { "only $($buttons.Count) buttons"; return }
$b = $buttons[$Index]
$b.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
"clicked [$Index] " + $b.Current.Name
