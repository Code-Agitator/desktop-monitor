$all = Get-CimInstance Win32_Process | Where-Object { $_.Name -eq 'msedgewebview2.exe' }
foreach ($p in $all) {
  $cl = $p.CommandLine
  if ($cl -notmatch 'desktop-monitor') { continue }
  $type = if ($cl -match '--type=([a-z-]+)') { $Matches[1] } else { 'browser' }
  $rid = if ($cl -match '--renderer-client-id=(\d+)') { $Matches[1] } else { '' }
  [pscustomobject]@{ Pid=$p.ProcessId; Parent=$p.ParentProcessId; Type=$type; Client=$rid; MemMB=[math]::Round((Get-Process -Id $p.ProcessId).WorkingSet64/1MB,1) }
}
