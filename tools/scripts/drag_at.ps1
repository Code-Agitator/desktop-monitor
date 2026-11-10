param([int]$X,[int]$Y,[int]$DX,[int]$DY)
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public class DA {
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f,uint dx,uint dy,uint d,UIntPtr e);
}
'@
[DA]::SetCursorPos($X,$Y) | Out-Null
Start-Sleep -Milliseconds 250
[DA]::mouse_event(0x0002,0,0,0,[UIntPtr]::Zero)
Start-Sleep -Milliseconds 150
for ($i=1; $i -le 20; $i++) {
  [DA]::SetCursorPos($X + [int]($DX*$i/20), $Y + [int]($DY*$i/20)) | Out-Null
  Start-Sleep -Milliseconds 30
}
[DA]::mouse_event(0x0004,0,0,0,[UIntPtr]::Zero)
Start-Sleep -Milliseconds 600
