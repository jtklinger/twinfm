# P13-M3's Size on disk column gate: target-p13od from the column's PR head c25173c (code as cb91532;
# the later commits change no code) against $Ref, the defaults and the column on: a fifth cell in every
# details row (its width 0 with the column off), so the defaults runs show what the cell costs off, and
# the runs with C:\traces\ondisk.toml (read-only, `size_on_disk = true`, made below if missing) what the
# column and its second read cost on. Defaults runs must hold no `size on disk:` line, the column's runs
# at least one; no run may hold a `[sizeview]` line. 9 runs, about 13 minutes, then the analysis.
# The tag and the reference are the coordinating session's to set: change $Tag or $Ref below if it
# allocates another tag or gates against another build (the wrapper refuses a tag that has traces).
# Only one gate at a time on this machine.
# Elevated PowerShell at the physical console, Narrator off, every TwinFM window closed:
#   powershell -ExecutionPolicy Bypass -File C:\traces\gate-p13m3b.ps1
# (a copy of tools\gate-p13m3b.ps1 on the column's branch; the scripts it calls are the gate checkout's
# tools\wpr-triplet.ps1, which calls wpr-gate.ps1 beside it, and tools\analyze-trace.py).
# Writes C:\traces\twinfm-<tag>-*.etl (+ .stderr.txt), C:\traces\export\<tag>-*.txt and
# C:\traces\gate-<tag>-summary.txt.
# The gate build is made by the coordinating session (--locked, from the detached checkout); the build
# step below is a no-op check.
$ErrorActionPreference = 'Stop'
$Root = 'C:\Users\jtkli\projects\twinfm'
$Head = 'c25173c'
$Tag = 'ab69'
$Ref = 'p12seg'
$Cand = 'p13od'
$Gate = "$Root\.claude\worktrees\gate-p13od"
$Cfg = 'C:\traces\ondisk.toml'

# --- Preconditions -------------------------------------------------------------------------------
$elev = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $elev) { throw 'Run this elevated (WPR needs administrator).' }
if (Get-Process -Name twinfm -ErrorAction SilentlyContinue) { throw 'A TwinFM window is open: close every one first.' }
if (Get-Process -Name Narrator -ErrorAction SilentlyContinue) { throw 'Narrator is running: turn it off first.' }
# A build or a test run shares the machine and voids the gate (ab29).
$busy = @(Get-Process | Where-Object { $_.Name -match '^(cargo|rustc|twinfm-[0-9a-f]{16})$' } | ForEach-Object Name | Sort-Object -Unique)
if ($busy) { throw "A build or test run is going ($($busy -join ', ')): wait for it to finish (ask Claude whether a session is running one)." }
# A Claude desktop thread spinning a core slowed two Phase 8 gates (ab33, ab34): sample every claude
# process's CPU over 5 s and refuse when any used more than 2 s of it (a core's worth is 5 s).
$before = @{}
Get-Process -Name claude -ErrorAction SilentlyContinue | ForEach-Object { $before[$_.Id] = $_.TotalProcessorTime.TotalSeconds }
Start-Sleep -Seconds 5
$spinning = @(Get-Process -Name claude -ErrorAction SilentlyContinue | Where-Object { $before.ContainsKey($_.Id) } |
    Where-Object { $_.TotalProcessorTime.TotalSeconds - $before[$_.Id] -gt 2.0 } |
    ForEach-Object { '{0} ({1:N1} s of CPU in 5 s)' -f $_.Id, ($_.TotalProcessorTime.TotalSeconds - $before[$_.Id]) })
if ($spinning) { throw "A Claude process is busy ($($spinning -join ', ')): wait for the sessions to go idle, or restart the Claude app, then run this again." }
foreach ($b in @($Ref, $Cand)) { if (-not (Test-Path "$Root\target-$b\release\twinfm.exe")) { throw "target-$b is not built." } }
if (Get-Process -Name wpr, xperf -ErrorAction SilentlyContinue) { throw 'Another trace is running (wpr or xperf): one gate at a time.' }
if (-not (Get-Command python -ErrorAction SilentlyContinue)) { throw 'python is not on PATH (needed for analyze-trace.py).' }
if (Test-Path "C:\traces\twinfm-$Tag-$Ref-1.etl") { throw "Tag $Tag has traces already in C:\traces: move them away or pick the next tag." }
# The column's settings file, read-only so no run rewrites it.
if (-not (Test-Path $Cfg)) {
    Set-Content -Encoding utf8 -Path $Cfg -Value 'size_on_disk = true'
    Set-ItemProperty -Path $Cfg -Name IsReadOnly -Value $true
}

# --- Gate build: a detached checkout of the PR head, --locked, its own target dir ------------------
if (-not (Test-Path $Gate)) { git -C $Root worktree add --detach $Gate $Head; if ($LASTEXITCODE) { throw 'worktree add failed' } }
$at = (git -C $Gate rev-parse --short HEAD).Trim()
if ($at -ne $Head) { throw "$Gate is at $at, not $Head (ask Claude to move it)" }
if (git -C $Gate status --porcelain) { throw "$Gate has local changes." }
"=== checking target-$Cand is built from $Head ($(Get-Date -Format HH:mm:ss))"
cargo build --release --locked --manifest-path "$Gate\Cargo.toml" --target-dir "$Root\target-$Cand"
if ($LASTEXITCODE) { throw 'cargo build failed' }

# --- The runs, interleaved ------------------------------------------------------------------
$started = Get-Date
& "$Gate\tools\wpr-triplet.ps1" -Reference $Ref -Candidate "$Cand,$Cand@$Cfg" -Tag $Tag -Root $Root

# --- Analysis and checks -----------------------------------------------------------------------
# Windows PowerShell turns a native program's redirected stderr into error records: keep going.
$ErrorActionPreference = 'Continue'
New-Item -ItemType Directory -Force C:\traces\export | Out-Null
$on = "$Cand-$([System.IO.Path]::GetFileNameWithoutExtension($Cfg))"
$rows = @()
foreach ($label in @($Ref, $Cand, $on)) {
    $build = if ($label -eq $Ref) { $Ref } else { $Cand }
    for ($n = 1; $n -le 3; $n++) {
        $etl = "C:\traces\twinfm-$Tag-$label-$n.etl"
        $err = [System.IO.Path]::ChangeExtension($etl, 'stderr.txt')
        $report = "C:\traces\export\$Tag-$label-$n.txt"
        python "$Gate\tools\analyze-trace.py" $etl --symbols --exe "$Root\target-$build\release\twinfm.exe" *> $report
        $wpr = [regex]::Match((Get-Content $report -Raw), '= ([0-9.]+)% busy').Groups[1].Value
        $log = Get-Content $err
        $inapp = [regex]::Match(($log | Select-String -SimpleMatch '[busy] 10 s after split' | Select-Object -First 1).Line, '([0-9.]+)% busy').Groups[1].Value
        # The render p50 of the per-second [perf] lines with 30 frames or more (the scrolling ones),
        # their median: the ab23 note's check that a session did not change state mid-triplet.
        $p50s = @($log | Select-String -Pattern 'render\[n=(\d+) p50=([0-9.]+)ms' | ForEach-Object { $_.Matches[0] } |
            Where-Object { [int]$_.Groups[1].Value -ge 30 } | ForEach-Object { [double]$_.Groups[2].Value } | Sort-Object)
        $p50 = if ($p50s.Count) { '{0:N2}' -f $p50s[[int][math]::Floor(($p50s.Count - 1) / 2)] } else { '?' }
        $reads = @($log | Select-String -SimpleMatch 'size on disk: ').Count
        $checks = @()
        if ($log | Select-String -SimpleMatch 'panicked') { $checks += 'PANIC' }
        if (-not $wpr) { $checks += 'no WPR figure' }
        if ($log | Select-String -SimpleMatch '[sizeview]') { $checks += 'a [sizeview] line' }
        if ($label -eq $on -and $reads -eq 0) { $checks += 'the column read nothing' }
        if ($label -ne $on -and $reads -gt 0) { $checks += 'a size on disk read with the column off' }
        $rows += [pscustomobject]@{ Run = "$label-$n"; Label = $label; WPR = [double]$wpr; InApp = $inapp; RenderP50 = $p50; Reads = $reads; Checks = $(if ($checks) { $checks -join ', ' } else { 'ok' }) }
    }
}

function Median($v) { $x = @($v | Sort-Object); $x[[int][math]::Floor(($x.Count - 1) / 2)] }
$lines = @("P13-M3 Size on disk column gate, tag $Tag, $(Get-Date -Format 'yyyy-MM-dd HH:mm'), $Head against target-$Ref, $([int]((Get-Date) - $started).TotalMinutes) min", '')
$lines += ($rows | Select-Object Run, WPR, InApp, RenderP50, Reads, Checks | Format-Table -AutoSize | Out-String).TrimEnd()
$lines += ''
$r = @($rows | Where-Object { $_.Label -eq $Ref } | ForEach-Object WPR)
$rMed = Median $r
foreach ($label in @($Cand, $on)) {
    $c = @($rows | Where-Object { $_.Label -eq $label } | ForEach-Object WPR)
    $cMed = Median $c
    # The gap rule is about a cost: it counts only when the candidate is the higher one.
    $allBelow = ($r | Measure-Object -Maximum).Maximum -lt ($c | Measure-Object -Minimum).Minimum
    $counts = $allBelow -or ($cMed - $rMed -gt 1.0)
    $lines += "${label}: median $cMed% against $Ref $rMed% (WPR); every reference run below every candidate run: $allBelow; a gap that counts: $counts"
}
$lines += 'Compare RenderP50 across runs before the medians: a run about a third higher than the rest is the session changing state (ab23).'
$lines | Set-Content -Encoding utf8 "C:\traces\gate-$Tag-summary.txt"
''
$lines
''
"summary: C:\traces\gate-$Tag-summary.txt; reports: C:\traces\export\$Tag-*.txt"
