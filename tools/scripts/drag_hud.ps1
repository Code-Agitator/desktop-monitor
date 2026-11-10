param([int]$FromX=150,[int]$FromY=80,[int]$DX=200,[int]$DY=200)
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public class MG {
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f,uint dx,uint dy,uint d,UIntPtr e);
}
'@
[MG]::SetCursorPos($FromX,$FromY) | Out-Null
Start-Sleep -Milliseconds 250
[MG]::mouse_event(0x0002,0,0,0,[UIntPtr]::Zero)
Start-Sleep -Milliseconds 150
for ($i=1; $i -le 20; $i++) {
  [MG]::SetCursorPos($FromX + [int]($DX*$i/20), $FromY + [int]($DY*$i/20)) | Out-Null
  Start-Sleep -Milliseconds 30
}
[MG]::mouse_event(0x0004,0,0,0,[UIntPtr]::Zero)
Start-Sleep -Milliseconds 600
