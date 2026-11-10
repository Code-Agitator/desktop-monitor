param([long]$Hwnd)
Add-Type -TypeDefinition @'
using System; using System.Text; using System.Runtime.InteropServices;
public class WS2 {
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr h, uint cmd);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
}
'@
$h = [IntPtr]$Hwnd
"iconic=" + [WS2]::IsIconic($h) + " visible=" + [WS2]::IsWindowVisible($h)
$prev = [WS2]::GetWindow($h, 3)  # GW_HWNDPREV
$next = [WS2]::GetWindow($h, 2)  # GW_HWNDNEXT
foreach ($pair in @(@("prev",$prev), @("next",$next))) {
  $w = $pair[1]; $sb = New-Object System.Text.StringBuilder 128
  if ($w -ne [IntPtr]::Zero) { [WS2]::GetClassName($w, $sb, 128) | Out-Null }
  "$($pair[0])=$w class=$sb"
}
