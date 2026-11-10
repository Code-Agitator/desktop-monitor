param(
    [string]$Exe = ".\src-tauri\target\debug\desktop-monitor.exe",
    [string]$InstallDir = "",
    [switch]$Uninstall
)

$ErrorActionPreference = "Stop"
$installDir = if ($InstallDir) { $InstallDir } else { Join-Path $env:LOCALAPPDATA "desktop-monitor" }
$taskName = "desktop-monitor"
$exeName = "desktop-monitor.exe"

if ($Uninstall) {
    schtasks /Delete /TN $taskName /F 2>$null | Out-Null
    Remove-Item (Join-Path ([Environment]::GetFolderPath("StartMenu")) "Programs\desktop-monitor.lnk") -ErrorAction SilentlyContinue
    Remove-Item $installDir -Recurse -Force -ErrorAction SilentlyContinue
    Write-Output "已卸载 desktop-monitor"
    return
}

$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    throw "请以管理员身份运行本脚本（注册最高权限的计划任务需要提权）"
}

if (-not $InstallDir -and $env:LOCALAPPDATA -like "*\Packages\*") {
    Write-Warning "检测到打包（MSIX）环境的 LOCALAPPDATA：$env:LOCALAPPDATA。计划任务可能访问不到包内路径，请用 -InstallDir 指向真实用户目录。"
}

if (-not (Test-Path $Exe)) {
    throw "找不到可执行文件：$Exe。先执行 pnpm tauri build --debug --no-bundle"
}

New-Item -ItemType Directory -Force -Path $installDir | Out-Null
Copy-Item $Exe (Join-Path $installDir $exeName) -Force

$action = New-ScheduledTaskAction -Execute (Join-Path $installDir $exeName) -WorkingDirectory $installDir
$trigger = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME
$settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -StartWhenAvailable
$principal = New-ScheduledTaskPrincipal -UserId "$env:USERDOMAIN\$env:USERNAME" -LogonType Interactive -RunLevel Highest
Register-ScheduledTask -TaskName $taskName -Action $action -Trigger $trigger -Settings $settings -Principal $principal -Force | Out-Null

$lnk = Join-Path ([Environment]::GetFolderPath("StartMenu")) "Programs\desktop-monitor.lnk"
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($lnk)
$shortcut.TargetPath = Join-Path $installDir $exeName
$shortcut.WorkingDirectory = $installDir
$shortcut.Save()

Write-Output "已安装到 $installDir，并注册计划任务 $taskName（登录时启动、最高权限、无 UAC 弹窗）"
