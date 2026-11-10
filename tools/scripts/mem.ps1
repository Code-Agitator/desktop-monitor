param([int]$RootPid, [int]$Seconds = 30)
$all = Get-CimInstance Win32_Process
function Get-Tree([int]$rootPid, $all) {
  $set = New-Object 'System.Collections.Generic.HashSet[int]'
  [void]$set.Add($rootPid)
  $changed = $true
  while ($changed) {
    $changed = $false
    foreach ($p in $all) {
      if ($set.Contains([int]$p.ParentProcessId) -and -not $set.Contains([int]$p.ProcessId)) {
        [void]$set.Add([int]$p.ProcessId); $changed = $true
      }
    }
  }
  return $set
}
$set = Get-Tree $RootPid $all
$start = @{}
foreach ($id in $set) { $p = Get-Process -Id $id -ErrorAction SilentlyContinue; if ($p) { $start[$id] = $p.TotalProcessorTime } }
Start-Sleep -Seconds $Seconds
$set = Get-Tree $RootPid $all
$mem = 0; $cpu = 0.0; $rows = @()
foreach ($id in $set) {
  $p = Get-Process -Id $id -ErrorAction SilentlyContinue
  if ($p) {
    $mem += $p.WorkingSet64
    $delta = if ($start.ContainsKey($id)) { ($p.TotalProcessorTime - $start[$id]).TotalSeconds } else { 0 }
    $cpu += $delta
    $rows += [pscustomobject]@{ Name=$p.ProcessName; Id=$id; MemMB=[math]::Round($p.WorkingSet64/1MB,1); CpuS=[math]::Round($delta,2) }
  }
}
$rows | Sort-Object MemMB -Descending | Format-Table -AutoSize | Out-String -Width 80
"TOTAL mem={0:N1} MB  cpu={1:N2}%  ({2} cores, {3}s)" -f ($mem/1MB), ($cpu/$Seconds/[Environment]::ProcessorCount*100), [Environment]::ProcessorCount, $Seconds
