param([int]$X=150,[int]$Y=100)
Add-Type -TypeDefinition @'
using System; using System.Text; using System.Runtime.InteropServices;
public class CT2 {
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f,uint dx,uint dy,uint d,UIntPtr e);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
}
'@
# Win+D
[CT2]::keybd_event(0x5B,0,0,[UIntPtr]::Zero)
[CT2]::keybd_event(0x44,0,0,[UIntPtr]::Zero)
Start-Sleep -Milliseconds 80
[CT2]::keybd_event(0x44,0,2,[UIntPtr]::Zero)
[CT2]::keybd_event(0x5B,0,2,[UIntPtr]::Zero)
Start-Sleep -Milliseconds 800
[CT2]::SetCursorPos($X,$Y) | Out-Null
Start-Sleep -Milliseconds 250
[CT2]::mouse_event(0x0002,0,0,0,[UIntPtr]::Zero)
Start-Sleep -Milliseconds 60
[CT2]::mouse_event(0x0004,0,0,0,[UIntPtr]::Zero)
Start-Sleep -Milliseconds 500
$fg = [CT2]::GetForegroundWindow()
$sb = New-Object System.Text.StringBuilder 128
[CT2]::GetClassName($fg, $sb, 128) | Out-Null
"fg=$fg class=$sb"
