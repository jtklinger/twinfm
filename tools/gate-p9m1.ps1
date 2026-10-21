# P9-M1 gate, tag ab38: target-p9m1 from PR #87 at b9fbd9d (adjustable column widths, main with P9-M2
# merged in; the wrapper's own commit follows it) against target-p9m2 (95ace9f, P9-M2's gate build,
# the last merged milestone's), two pairs interleaved in one session under the one tag
# ab38: the defaults (every details row now reads its widths from the list's properties instead of
# constants: the cost this measures) and other widths (C:\traces\cols.toml, read-only, both panes
# 200, 110, 180), the reference run with the same settings file as the candidate each time (it
# ignores the column keys, so its runs there are its defaults). Each round runs p9m2, p9m1,
# p9m2-cols, p9m1-cols. 12 runs, about 18 minutes, then the analysis (a few minutes).
# Only one gate at a time on this machine.
# Elevated PowerShell at the physical console, Narrator off, every TwinFM window closed:
#   powershell -ExecutionPolicy Bypass -File C:\traces\gate-p9m1.ps1
# (a copy of tools\gate-p9m1.ps1 on the P9-M1 branch; the scripts it calls are the gate checkout's
# tools\wpr-triplet.ps1, which calls wpr-gate.ps1 beside it, and tools\analyze-trace.py).
# Writes C:\traces\twinfm-ab38-*.etl (+ .stderr.txt), C:\traces\export\ab38-*.txt and
# C:\traces\gate-ab38-summary.txt.
# The gate build is already made (Claude built it); the build step below is a no-op check.
$ErrorActionPreference = 'Stop'
$Root = 'C:\Users\jtkli\projects\twinfm'
$Head = 'b9fbd9d'
$Tag = 'ab38'
$Ref = 'p9m2'
$Cand = 'p9m1'
$Gate = "$Root\.claude\worktrees\gate-p9m1"
# Each setting: the settings file ('' for the defaults), its label suffix, and a line the
# candidate's stderr must hold (the reference has no column widths).
$Settings = @(
    [pscustomobject]@{ Toml = ''; Suffix = ''; Line = '[cols] pane=0 widths type 120 size 90 modified 140' },
    [pscustomobject]@{ Toml = 'C:\traces\cols.toml'; Suffix = '-cols'; Line = '[cols] pane=0 widths type 200 size 110 modified 180' }
)

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
foreach ($s in $Settings | Where-Object Toml) {
    if (-not (Test-Path $s.Toml)) { throw "$($s.Toml) is missing." }
    if (-not (Get-Item $s.Toml).IsReadOnly) { throw "$($s.Toml) must be read-only (a run would rewrite it)." }
}
if (Get-Process -Name wpr, xperf -ErrorAction SilentlyContinue) { throw 'Another trace is running (wpr or xperf): one gate at a time.' }
if (-not (Get-Command python -ErrorAction SilentlyContinue)) { throw 'python is not on PATH (needed for analyze-trace.py).' }
if (Test-Path "C:\traces\twinfm-$Tag-$Ref-1.etl") { throw "Tag $Tag has traces already in C:\traces: move them away or pick the next tag." }

# --- Gate build: a detached checkout of the PR head, --locked, its own target dir ------------------
if (-not (Test-Path $Gate)) { git -C $Root worktree add --detach $Gate $Head; if ($LASTEXITCODE) { throw 'worktree add failed' } }
$at = (git -C $Gate rev-parse --short HEAD).Trim()
if ($at -ne $Head) { throw "$Gate is at $at, not $Head (ask Claude to move it)" }
if (git -C $Gate status --porcelain) { throw "$Gate has local changes." }
"=== checking target-$Cand is built from $Head ($(Get-Date -Format HH:mm:ss))"
cargo build --release --locked --manifest-path "$Gate\Cargo.toml" --target-dir "$Root\target-$Cand"
if ($LASTEXITCODE) { throw 'cargo build failed' }

# --- The two pairs, interleaved ------------------------------------------------------------------
$specs = @($Cand)
foreach ($s in $Settings | Where-Object Toml) { $specs += "$Ref@$($s.Toml)"; $specs += "$Cand@$($s.Toml)" }
$started = Get-Date
& "$Gate\tools\wpr-triplet.ps1" -Reference $Ref -Candidate ($specs -join ',') -Tag $Tag -Root $Root

# --- Analysis and checks -----------------------------------------------------------------------
# Windows PowerShell turns a native program's redirected stderr into error records: keep going.
$ErrorActionPreference = 'Continue'
New-Item -ItemType Directory -Force C:\traces\export | Out-Null
$rows = @()
foreach ($s in $Settings) {
    foreach ($build in @($Ref, $Cand)) {
        $label = "$build$($s.Suffix)"
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
            $checks = @()
            if ($log | Select-String -SimpleMatch 'panicked') { $checks += 'PANIC' }
            if (-not $wpr) { $checks += 'no WPR figure' }
            if ($s.Toml -and -not ($log | Select-String -SimpleMatch "[settings] file $($s.Toml) (read)")) { $checks += 'settings file not read' }
            if ($build -eq $Cand -and -not ($log | Select-String -SimpleMatch $s.Line)) { $checks += "no '$($s.Line)'" }
            $rows += [pscustomobject]@{ Run = "$label-$n"; Setting = $s.Suffix; Build = $build; WPR = [double]$wpr; InApp = $inapp; RenderP50 = $p50; Checks = $(if ($checks) { $checks -join ', ' } else { 'ok' }) }
        }
    }
}

function Median($v) { $x = @($v | Sort-Object); $x[[int][math]::Floor(($x.Count - 1) / 2)] }
$lines = @("P9-M1 gate, tag $Tag (both settings under one tag), $(Get-Date -Format 'yyyy-MM-dd HH:mm'), $Head against target-$Ref, $([int]((Get-Date) - $started).TotalMinutes) min", '')
$lines += ($rows | Select-Object Run, WPR, InApp, RenderP50, Checks | Format-Table -AutoSize | Out-String).TrimEnd()
foreach ($s in $Settings) {
    $name = if ($s.Suffix) { $s.Suffix.TrimStart('-') } else { 'defaults' }
    $r = @($rows | Where-Object { $_.Setting -eq $s.Suffix -and $_.Build -eq $Ref } | ForEach-Object WPR)
    $c = @($rows | Where-Object { $_.Setting -eq $s.Suffix -and $_.Build -eq $Cand } | ForEach-Object WPR)
    $rMed = Median $r; $cMed = Median $c
    # The gap rule is about a cost: it counts only when the candidate is the higher one.
    $allBelow = ($r | Measure-Object -Maximum).Maximum -lt ($c | Measure-Object -Minimum).Minimum
    $counts = $allBelow -or ($cMed - $rMed -gt 1.0)
    $under = ($c | Measure-Object -Maximum).Maximum -lt 20
    $lines += "${name}: $Cand median $cMed% against $Ref $rMed% (WPR); every reference run below every candidate run: $allBelow; a gap that counts: $counts; every candidate run under 20%: $under"
}
$lines += 'Compare RenderP50 across runs before the medians: a run about a third higher than the rest is the session changing state (ab23).'
$lines | Set-Content -Encoding utf8 "C:\traces\gate-$Tag-summary.txt"
''
$lines
''
"summary: C:\traces\gate-$Tag-summary.txt; reports: C:\traces\export\$Tag-*.txt"
