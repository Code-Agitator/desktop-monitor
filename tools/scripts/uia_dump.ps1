param([long]$Hwnd)
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$win = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$Hwnd)
$all = $win.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
foreach ($e in $all) {
  $t = $e.Current.ControlType.ProgrammaticName
  $n = $e.Current.Name
  if ($t -match "Button|ComboBox|Spinner|CheckBox|TabItem" -and $n) {
    $v = ""
    try { $vp = $e.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern); $v = " value=" + $vp.Current.Value } catch {}
    "$t`t$n$v"
  }
}
