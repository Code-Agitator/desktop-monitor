Add-Type -TypeDefinition @'
using System; using System.Runtime.InteropServices;
public class SD { [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra); }
'@
[SD]::keybd_event(0x5B,0,0,[UIntPtr]::Zero)
[SD]::keybd_event(0x44,0,0,[UIntPtr]::Zero)
Start-Sleep -Milliseconds 80
[SD]::keybd_event(0x44,0,2,[UIntPtr]::Zero)
[SD]::keybd_event(0x5B,0,2,[UIntPtr]::Zero)
Start-Sleep -Milliseconds 1500
