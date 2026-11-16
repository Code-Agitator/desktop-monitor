param(
  [switch]$Debug,
  [switch]$SkipBuild,
  [switch]$NoProbe,
  [string]$Tag = "run"
)

# Build + install + launch + measure HUD layout.
# NOTE: must build through the Tauri CLI. A raw `cargo build` is treated as dev
# mode by Tauri, so the window tries to load http://localhost:1420 and shows a
# blank page when the Vite dev server is not running.
# Keep this file ASCII-only: Windows PowerShell reads .ps1 as ANSI when there is
# no BOM, and non-ASCII comments can break parsing.
$ErrorActionPreference = "Continue"

# Repo root = two levels up from tools/scripts (works locally and on CI).
$root = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$node = "C:\Users\14820\.dsh\dsh-runtimes\dsh-primary-runtime\dependencies\node\bin\node.exe"
$py = "C:\Users\14820\.dsh\dsh-runtimes\dsh-primary-runtime\dependencies\python\python.exe"
$profileName = if ($Debug) { "debug" } else { "release" }

Get-Process desktop-monitor -ErrorAction SilentlyContinue | ForEach-Object {
  "  stop pid=$($_.Id)"
  Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue
}
Start-Sleep -Milliseconds 900

if (-not $SkipBuild) {
  Push-Location $root
  $tauriArgs = @("node_modules\@tauri-apps\cli\tauri.js", "build", "--no-bundle")
  if ($Debug) { $tauriArgs += "--debug" }
  & $node @tauriArgs 2>&1 | Select-Object -Last 5
  $buildExit = $LASTEXITCODE
  Pop-Location
  if ($buildExit -ne 0) {
    "  build failed (exit=$buildExit), aborting instead of installing a stale exe"
    exit 1
  }
}

$built = Join-Path $root "src-tauri\target\$profileName\desktop-monitor.exe"
"  built: $built exists=$(Test-Path $built)"
if (-not (Test-Path $built)) { exit 1 }

Copy-Item $built "C:\Program Files\desktop-monitor\desktop-monitor.exe" -Force

$logDir = Join-Path $root "tools\.logs"
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
$err = Join-Path $logDir "app-stderr-$Tag.txt"
Remove-Item $err -ErrorAction SilentlyContinue

$p = Start-Process -FilePath "C:\Program Files\desktop-monitor\desktop-monitor.exe" `
  -WorkingDirectory "C:\Program Files\desktop-monitor" -PassThru `
  -RedirectStandardError $err -RedirectStandardOutput (Join-Path $logDir "app-stdout-$Tag.txt")
Start-Sleep -Seconds 8
"  alive=$(-not $p.HasExited)"

"--- stderr ---"
Get-Content $err -ErrorAction SilentlyContinue | Select-String -Pattern "panic|error" | Select-Object -First 5

if (-not $NoProbe) {
  $env:PYTHONIOENCODING = "utf-8"
  & $py (Join-Path $root "tools/scripts/hud_probe.py") --out (Join-Path $logDir "hud-probe-$Tag.png")
}
