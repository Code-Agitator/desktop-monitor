param([long]$Hwnd, [int]$Index = 0, [string]$Value = "300")
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$win = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$Hwnd)
$cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Spinner)
$items = $win.FindAll([System.Windows.Automation.TreeScope]::Descendants, $cond)
if ($Index -ge $items.Count) { "only $($items.Count) spinners"; return }
$e = $items[$Index]
$e.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).SetValue($Value)
"set [$Index] $($e.Current.Name) = $Value"
