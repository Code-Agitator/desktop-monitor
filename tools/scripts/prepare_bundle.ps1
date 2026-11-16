param(
  [switch]$SkipBuild
)

# Stage the built-in plugins into src-tauri/resources so the installer ships them.
# Keep this file ASCII-only (Windows PowerShell reads .ps1 as ANSI without a BOM).
$ErrorActionPreference = "Stop"
# Repo root = two levels up from tools/scripts (works locally and on CI).
$root = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$res = Join-Path $root "src-tauri\resources\plugins"

function Publish-Plugin($name, $exe, $proj) {
  $out = Join-Path $root "build\$name"
  if (-not $SkipBuild) {
    "  dotnet publish $name"
    Push-Location $proj
    & dotnet publish -c Release -o $out | Out-Null
    if ($LASTEXITCODE -ne 0) { Pop-Location; throw "publish failed: $name" }
    Pop-Location
  }
  $src = Join-Path $out $exe
  if (-not (Test-Path $src)) { throw "missing $src (build plugins first)" }
  $dest = Join-Path $res $name
  New-Item -ItemType Directory -Force -Path $dest | Out-Null
  Copy-Item $src (Join-Path $dest $exe) -Force
  Copy-Item (Join-Path $proj "manifest.json") (Join-Path $dest "manifest.json") -Force
  "  staged $name -> $dest"
}

New-Item -ItemType Directory -Force -Path $res | Out-Null
Publish-Plugin "device-battery" "DeviceBattery.exe" (Join-Path $root "plugins\builtin\device-battery")
Publish-Plugin "sys-collector" "SysCollector.exe" (Join-Path $root "plugins\builtin\sys-collector")

Get-ChildItem $res -Recurse -File | ForEach-Object {
  "  {0}  {1:N1} MB" -f $_.FullName.Replace($root + "\", ""), ($_.Length / 1MB)
}
