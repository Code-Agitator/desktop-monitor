param(
  [switch]$SkipPlugins
)

# Build a Windows installer (NSIS): stage plugins -> tauri build -> print result.
# Keep this file ASCII-only (Windows PowerShell reads .ps1 as ANSI without a BOM).
# $ErrorActionPreference stays "Continue": the CLI writes progress to stderr and
# PowerShell would otherwise treat those lines as terminating errors.
$ErrorActionPreference = "Continue"
# Repo root = two levels up from tools/scripts (works locally and on CI).
$root = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$node = "C:\Users\14820\.dsh\dsh-runtimes\dsh-primary-runtime\dependencies\node\bin\node.exe"

if (-not $SkipPlugins) {
  "== stage built-in plugins =="
  & (Join-Path $root "tools\scripts\prepare_bundle.ps1") -SkipBuild
}

"== tauri build (bundle: nsis) =="
Push-Location $root
& $node "node_modules\@tauri-apps\cli\tauri.js" build 2>&1 | Select-Object -Last 6
$code = $LASTEXITCODE
Pop-Location
if ($code -ne 0) {
  "  tauri build failed (exit=$code)"
  exit 1
}

$setup = Get-ChildItem (Join-Path $root "src-tauri\target\release\bundle\nsis") -Filter "*-setup.exe" |
  Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $setup) {
  "  installer not found"
  exit 1
}

$hash = (Get-FileHash $setup.FullName -Algorithm SHA256).Hash
""
"== installer =="
"  path  : $($setup.FullName)"
"  size  : {0:N2} MB" -f ($setup.Length / 1MB)
"  sha256: $hash"
