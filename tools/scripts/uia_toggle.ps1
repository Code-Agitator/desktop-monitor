param([long]$Hwnd, [int]$Index = 0)
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$win = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$Hwnd)
$cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::CheckBox)
$items = $win.FindAll([System.Windows.Automation.TreeScope]::Descendants, $cond)
if ($Index -ge $items.Count) { "only $($items.Count) checkboxes"; return }
$e = $items[$Index]
$tp = $e.GetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern)
$tp.Toggle()
"toggled $($e.Current.Name) -> $($tp.Current.ToggleState)"
