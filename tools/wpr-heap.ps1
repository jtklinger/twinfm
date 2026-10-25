# Phase 12 memory check (docs/phase-12-plan.md, the coordinating session's tasks): WPR's heap
# snapshots of TwinFM during the P5-M3 soak, to say what the slow private-bytes growth of the
# Phase 11 close's 60-minute soak is. The soak runs target-p11close (a99f2f2, 0.2.0's code) from the
# gate-p11close checkout's own tools\soak-p5m3.ps1; WPR records the HeapSnapshot profile, with heap
# stack tracking switched on for twinfm.exe through IFEO before the soak starts, and takes two
# snapshots: one after the caches have filled, one near the end. It records the VirtualAllocation
# profile beside it, since the 5-minute dry run (heapdry) found the heap only about 96 MB of 289 MB
# private: the rest is memory committed outside the heap (VirtualAlloc, by the graphics driver among
# others). Nothing is compared here: the trace is read afterwards without elevation by
# tools\analyze-heap.py (xperf's heapsnapshot and virtualalloc actions, with twinfm's symbols).
# Elevated PowerShell at the physical console, every TwinFM window closed, Narrator off:
#   powershell -ExecutionPolicy Bypass -File C:\traces\wpr-heap.ps1 -Minutes 5 -Tag heapdry
#   powershell -ExecutionPolicy Bypass -File C:\traces\wpr-heap.ps1 -Minutes 60 -Tag heap1
# (a copy of tools\wpr-heap.ps1). Writes C:\traces\twinfm-<tag>.etl, C:\traces\<tag>-soak.txt (the
# soak's report), C:\traces\<tag>-memory.csv (its samples), C:\traces\<tag>-stderr.txt and
# C:\traces\<tag>-summary.txt. The heap stack tracking is switched off again at the end, failed or not:
# while it is on, every new twinfm.exe records its heap stacks.
param(
    [double]$Minutes = 60,
    [string]$Tag = 'heap1'
)
$ErrorActionPreference = 'Stop'
$Root = 'C:\Users\jtkli\projects\twinfm'
$Head = 'a99f2f2'
$Gate = "$Root\.claude\worktrees\gate-p11close"
$Exe = "$Root\target-p11close\release\twinfm.exe"
$Soak = "$Gate\tools\soak-p5m3.ps1"
$Etl = "C:\traces\twinfm-$Tag.etl"
# The first snapshot after the caches have filled (the soaks climb for about five minutes), the
# second as long before the end; a short dry run takes them at a fifth of the way in and out.
$Edge = [math]::Min(5.0, $Minutes / 5)
$FirstAt = $Edge
$LastAt = $Minutes - $Edge

# --- Preconditions -------------------------------------------------------------------------------
$elev = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $elev) { throw 'Run this elevated (WPR needs administrator).' }
if (Get-Process -Name twinfm -ErrorAction SilentlyContinue) { throw 'A TwinFM window is open: close every one first.' }
if (Get-Process -Name Narrator -ErrorAction SilentlyContinue) { throw 'Narrator is running: turn it off first.' }
if (Get-Process -Name wpr, xperf -ErrorAction SilentlyContinue) { throw 'Another trace is running (wpr or xperf): one at a time.' }
if (-not (Test-Path $Exe)) { throw "target-p11close is not built ($Exe)." }
if (-not (Test-Path $Soak)) { throw "The gate-p11close checkout has no soak script ($Soak)." }
$at = (git -C $Gate rev-parse --short HEAD).Trim()
if ($at -ne $Head) { throw "$Gate is at $at, not $Head (ask Claude)." }
if (Test-Path $Etl) { throw "$Etl exists already: move it away or pick another -Tag." }
if ($Minutes -lt 3) { throw '-Minutes must be at least 3.' }

$summary = New-Object System.Collections.Generic.List[string]
function Say([string]$s) { $line = "$(Get-Date -Format HH:mm:ss) $s"; Write-Host $line; $summary.Add($line) }
Say "heap check $Tag`: target-p11close ($Head), soak $Minutes min, snapshots at $FirstAt and $LastAt min"

$soakProc = $null
$recording = $false
try {
    # --- Heap stack tracking for new twinfm.exe processes, then the trace ---------------------------
    wpr -snapshotconfig heap -name twinfm.exe enable
    if ($LASTEXITCODE) { throw "wpr -snapshotconfig enable failed ($LASTEXITCODE)" }
    wpr -start HeapSnapshot -start VirtualAllocation -filemode
    if ($LASTEXITCODE) { throw "wpr -start HeapSnapshot VirtualAllocation failed ($LASTEXITCODE)" }
    $recording = $true
    Say 'WPR recording HeapSnapshot and VirtualAllocation; heap stack tracking on for twinfm.exe'

    # --- The soak, in its own PowerShell so its report lands in a file ---------------------------
    $t0 = Get-Date
    $soakProc = Start-Process -FilePath powershell.exe -PassThru -WindowStyle Minimized `
        -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', "`"$Soak`"", '-Minutes', "$Minutes", '-Exe', "`"$Exe`"") `
        -RedirectStandardOutput "C:\traces\$Tag-soak.txt" -RedirectStandardError "C:\traces\$Tag-soak-errors.txt"
    $null = $soakProc.Handle # keep the handle so ExitCode is available after exit
    $fm = $null
    for ($i = 0; $i -lt 120 -and -not $fm; $i++) {
        Start-Sleep -Seconds 1
        $fm = Get-Process -Name twinfm -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $Exe } | Select-Object -First 1
        if ($soakProc.HasExited) { throw "The soak exited before TwinFM started (code $($soakProc.ExitCode)); see C:\traces\$Tag-soak-errors.txt." }
    }
    if (-not $fm) { throw 'TwinFM did not start within two minutes.' }
    Say "soak started TwinFM, pid $($fm.Id)"

    # --- The two snapshots ------------------------------------------------------------------------
    foreach ($when in @($FirstAt, $LastAt)) {
        $due = $t0.AddMinutes($when)
        while ((Get-Date) -lt $due) {
            if ($fm.HasExited) { throw "TwinFM exited before the snapshot at $when min (exit code $($fm.ExitCode))." }
            Start-Sleep -Seconds 5
        }
        $fm.Refresh()
        wpr -singlesnapshot heap $fm.Id
        if ($LASTEXITCODE) { throw "wpr -singlesnapshot failed at $when min ($LASTEXITCODE)" }
        Say ("snapshot at {0} min: working set {1:N0} MB, private {2:N0} MB" -f $when, ($fm.WorkingSet64 / 1MB), ($fm.PrivateMemorySize64 / 1MB))
    }

    # --- The soak's end, then the trace -----------------------------------------------------------
    $soakProc.WaitForExit()
    Say "soak finished (code $($soakProc.ExitCode))"
    wpr -stop $Etl
    if ($LASTEXITCODE) { throw "wpr -stop failed ($LASTEXITCODE)" }
    $recording = $false
    Say ("trace {0}: {1:N0} MB" -f $Etl, ((Get-Item $Etl).Length / 1MB))
} finally {
    if ($recording) { wpr -cancel | Out-Null; Say 'WPR cancelled (the run failed)' }
    wpr -snapshotconfig heap -name twinfm.exe disable | Out-Null
    Say 'heap stack tracking off for twinfm.exe'
    if ($soakProc -and -not $soakProc.HasExited) { Say 'the soak is still running: let it finish or close its window' }
    # The soak's own files, beside the trace.
    $m5 = Join-Path $env:TEMP 'twinfm-m5'
    foreach ($f in @(@('soak-p5m3-memory.csv', 'memory.csv'), @('soak-p5m3-stderr.txt', 'stderr.txt'))) {
        $src = Join-Path $m5 $f[0]
        if (Test-Path $src) { Copy-Item $src "C:\traces\$Tag-$($f[1])" -Force }
    }
    $summary | Set-Content -Encoding utf8 "C:\traces\$Tag-summary.txt"
    Write-Host "Summary: C:\traces\$Tag-summary.txt"
}
