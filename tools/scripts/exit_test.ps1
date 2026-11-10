param([long]$Hwnd, [int]$QuitButtonIndex = 7)
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Windows.Forms
Add-Type -TypeDefinition @'
using System; using System.Runtime.InteropServices;
public class ET { [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h); }
'@
$win = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$Hwnd)
$cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Button)
$buttons = $win.FindAll([System.Windows.Automation.TreeScope]::Descendants, $cond)
if ($QuitButtonIndex -ge $buttons.Count) { "only $($buttons.Count) buttons"; return }
$quit = $buttons[$QuitButtonIndex]
"clicking " + $quit.Current.Name
$quit.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
Start-Sleep -Milliseconds 700
[ET]::SetForegroundWindow([IntPtr]$Hwnd) | Out-Null
[System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
Start-Sleep -Milliseconds 500
"enter sent"
