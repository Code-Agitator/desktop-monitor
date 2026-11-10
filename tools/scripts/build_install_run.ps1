param(
  [switch]$Debug,
  [switch]$SkipBuild,
  [switch]$NoProbe,
  [string]$Tag = "run"
)
# 构建 + 安装 + 启动 + 量测 HUD 排版。
#
# 必须用 Tauri CLI 构建：直接 `cargo build` 会被 Tauri 判成 dev 模式，窗口会去连
# localhost:1420 的 Vite dev server；dev server 没开时页面加载失败，观感就是
# 「后端无法访问」+ 一片空白（踩过一次）。
$ErrorActionPreference = "Continue"
$root = "D:\Work\desktop-monitor"
$node = "C:\Users\14820\.dsh\dsh-runtimes\dsh-primary-runtime\dependencies\node\bin\node.exe"
$py = "C:\Users\14820\.dsh\dsh-runtimes\dsh-primary-runtime\dependencies\python\python.exe"
$profileName = if ($Debug) { "debug" } else { "release" }

Get-Process desktop-monitor -ErrorAction SilentlyContinue | ForEach-Object {
  "  停止 pid=$($_.Id)"; Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue
}
Start-Sleep -Milliseconds 900

if (-not $SkipBuild) {
  Push-Location $root
  $args = @("node_modules\@tauri-apps\cli\tauri.js", "build", "--no-bundle")
  if ($Debug) { $args += "--debug" }
  & $node @args 2>&1 | Select-Object -Last 5
  Pop-Location
}
$built = Join-Path $root "src-tauri\target\$profileName\desktop-monitor.exe"
"   构建产物：$built 存在=$(Test-Path $built)"
if (-not (Test-Path $built)) { exit 1 }

Copy-Item $built "C:\Program Files\desktop-monitor\desktop-monitor.exe" -Force
$err = Join-Path $root "docs\app-stderr-$Tag.txt"
Remove-Item $err -ErrorAction SilentlyContinue
$p = Start-Process -FilePath "C:\Program Files\desktop-monitor\desktop-monitor.exe" `
  -WorkingDirectory "C:\Program Files\desktop-monitor" -PassThru `
  -RedirectStandardError $err -RedirectStandardOutput (Join-Path $root "docs\app-stdout-$Tag.txt")
Start-Sleep -Seconds 8
"   alive=$(-not $p.HasExited)"
"--- stderr 里的错误 ---"
Get-Content $err -ErrorAction SilentlyContinue | Select-String -Pattern "失败|panic|error" | Select-Object -First 5

if (-not $NoProbe) {
  $env:PYTHONIOENCODING = "utf-8"
  & $py (Join-Path $root "tools\scripts\hud_probe.py") --out (Join-Path $root "docs\hud-probe-$Tag.png")
}
