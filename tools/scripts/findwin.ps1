param([int]$TargetPid)
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public class FW {
  public delegate bool EnumProc(IntPtr h, IntPtr lp);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr lp);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L,T,R,B; }
}
'@
$lines = New-Object System.Collections.ArrayList
[FW]::EnumWindows({ param($h,$lp)
  $p=0; [FW]::GetWindowThreadProcessId($h,[ref]$p) | Out-Null
  if ($p -eq $TargetPid) {
    $sb = New-Object System.Text.StringBuilder 256
    [FW]::GetWindowText($h,$sb,256) | Out-Null
    $cb = New-Object System.Text.StringBuilder 256
    [FW]::GetClassName($h,$cb,256) | Out-Null
    $r = New-Object FW+RECT; [FW]::GetWindowRect($h,[ref]$r) | Out-Null
    [void]$lines.Add("$h [$cb] title='$sb' vis=$([FW]::IsWindowVisible($h)) rect=($($r.L),$($r.T))-($($r.R),$($r.B))")
  }
  return $true
}, [IntPtr]::Zero) | Out-Null
$lines
