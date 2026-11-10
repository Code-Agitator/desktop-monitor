param([int]$X,[int]$Y)
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public class CA {
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f,uint dx,uint dy,uint d,UIntPtr e);
}
'@
[CA]::SetCursorPos($X,$Y) | Out-Null
Start-Sleep -Milliseconds 200
[CA]::mouse_event(0x0002,0,0,0,[UIntPtr]::Zero)
Start-Sleep -Milliseconds 50
[CA]::mouse_event(0x0004,0,0,0,[UIntPtr]::Zero)
Start-Sleep -Milliseconds 400
