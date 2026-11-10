param([long]$Hwnd, [int]$Index = 0, [int]$ItemIndex = 1)
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$win = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$Hwnd)
$cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::ComboBox)
$combos = $win.FindAll([System.Windows.Automation.TreeScope]::Descendants, $cond)
if ($Index -ge $combos.Count) { "only $($combos.Count) combos"; return }
$combo = $combos[$Index]
$ep = $combo.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern)
$ep.Expand(); Start-Sleep -Milliseconds 150
$itemCond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::ListItem)
$items = $combo.FindAll([System.Windows.Automation.TreeScope]::Descendants, $itemCond)
if ($ItemIndex -ge $items.Count) { "only $($items.Count) items: " + (($items | ForEach-Object { $_.Current.Name }) -join ", ") }
$it = $items[$ItemIndex]
$it.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
"selected " + $it.Current.Name
