Add-Type -TypeDefinition @'
using System; using System.Text; using System.Collections.Generic; using System.Runtime.InteropServices;
public class CW3 {
  public delegate bool EnumProc(IntPtr h, IntPtr lp);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr lp);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
}
'@
$rows = New-Object System.Collections.ArrayList
[CW3]::EnumWindows({ param($h,$lp)
  $cb = New-Object System.Text.StringBuilder 256
  [CW3]::GetClassName($h,$cb,256) | Out-Null
  if ($cb.ToString() -eq 'ConsoleWindowClass') {
    $tb = New-Object System.Text.StringBuilder 256
    [CW3]::GetWindowText($h,$tb,256) | Out-Null
    [void]$rows.Add("hwnd=$h vis=$([CW3]::IsWindowVisible($h)) title=$tb")
  }
  return $true
}, [IntPtr]::Zero) | Out-Null
if ($rows.Count -eq 0) { "no console windows" } else { $rows }
