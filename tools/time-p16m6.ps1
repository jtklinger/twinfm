# P16-M6, settled first: what following a content search costs while many files are written under
# its root. One scripted run of target\release\twinfm.exe with a content search tab ("needle") open on
# %TEMP%\twinfm-p16m6-time\tree (200 text files of 64 KB), while this script writes files below it in
# four phases, each inside one of TwinFM's long sleeps (the script watches stderr for the mark):
#   sync   -Sync small text files (4 KB each) written as fast as PowerShell writes them, as a checkout
#          or a sync client does;
#   build  for -Seconds s, 20 binary files of 100 KB and 5 text files of 20 KB a second, as a build does;
#   log    for -Seconds s, one 12 MB text file appended to five times a second, as a log being written
#          (the worst case: under the 16 MB cap, so every window reads it whole again);
#   idle   -Seconds s with nothing written (the process's own baseline).
# Before each phase the search is run again (F5), so a phase that ended the following does not hide
# the next. For each phase: the windows that landed, their records, the files read, the follow
# thread's time (the windows' `in T ms` summed), whether the following ended, and the CPU time the
# whole process took against the phase's length. -Name runs the same with a name search (*.txt) in
# place of the content search, for comparison. Run it with the machine quiet.
param(
    [string]$Exe = (Join-Path $PSScriptRoot '..\target\release\twinfm.exe'),
    [int]$Sync = 2000,
    [int]$Seconds = 20,
    [switch]$Name
)
$ErrorActionPreference = 'Stop'
$root = Join-Path $env:TEMP 'twinfm-p16m6-time'
if (Test-Path $root) { Remove-Item -Recurse -Force $root }
$tree = Join-Path $root 'tree'
New-Item -ItemType Directory -Force (Join-Path $tree 'base'), (Join-Path $tree 'sync'), (Join-Path $tree 'build'), (Join-Path $tree 'logs') | Out-Null
$line = 'alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu' + "`r`n"
$block64 = $line * [int](64KB / $line.Length)
for ($i = 0; $i -lt 200; $i++) {
    $text = if ($i % 50 -eq 7) { $block64 + "the needle sits here`r`n" } else { $block64 }
    [System.IO.File]::WriteAllText((Join-Path $tree ('base\f{0:d3}.txt' -f $i)), $text)
}
$small = $line * [int](4KB / $line.Length)
$text20 = $line * [int](20KB / $line.Length)
$bin = New-Object byte[] 100KB
(New-Object System.Random 166).NextBytes($bin)
$bin[10] = 0
$log = Join-Path $tree 'logs\big.log'
[System.IO.File]::WriteAllText($log, $line * [int](12MB / $line.Length))

$hold = ($Seconds + 12) * 1000
$search = if ($Name) { @('filter 0 *.txt', 'sleep 300', 'searchreturn 0') } else { @('csearch 0 needle') }
$again = @('click 0 0', 'key F5', 'waitsearch 120000', 'sleep 1500')
$steps = @('sleep 2500', "nav 0 $tree", 'sleep 800') + $search + @('waitsearch 120000', 'sleep 1500', "sleep $($hold + 1)") +
    $again + @("sleep $($hold + 2)") + $again + @("sleep $($hold + 3)") + $again + @("sleep $($hold + 4)", 'quit')
$script = Join-Path $root 'script.txt'
[System.IO.File]::WriteAllText($script, ($steps -join ';'))
$err = Join-Path $root 'stderr.txt'
$env:TWINFM_SCRIPT = "@$script"
Remove-Item Env:TWINFM_CONFIG -ErrorAction SilentlyContinue
try {
    $proc = Start-Process -FilePath $Exe -PassThru -RedirectStandardError $err
    $null = $proc.Handle
} finally {
    Remove-Item Env:TWINFM_SCRIPT -ErrorAction SilentlyContinue
}
$phases = [ordered]@{
    "[auto] Sleep($($hold + 1))" = @('sync', {
            for ($i = 0; $i -lt $Sync; $i++) { [System.IO.File]::WriteAllText((Join-Path $tree ('sync\s{0:d5}.txt' -f $i)), $small) }
        })
    "[auto] Sleep($($hold + 2))" = @('build', {
            $sw = [System.Diagnostics.Stopwatch]::StartNew(); $n = 0
            while ($sw.Elapsed.TotalSeconds -lt $Seconds) {
                for ($k = 0; $k -lt 4; $k++) { [System.IO.File]::WriteAllBytes((Join-Path $tree ('build\o{0:d5}.obj' -f $n)), $bin); $n++ }
                [System.IO.File]::WriteAllText((Join-Path $tree ('build\g{0:d5}.txt' -f $n)), $text20); $n++
                Start-Sleep -Milliseconds 200
            }
        })
    "[auto] Sleep($($hold + 3))" = @('log', {
            $sw = [System.Diagnostics.Stopwatch]::StartNew()
            while ($sw.Elapsed.TotalSeconds -lt $Seconds) { [System.IO.File]::AppendAllText($log, $line); Start-Sleep -Milliseconds 200 }
        })
    "[auto] Sleep($($hold + 4))" = @('idle', { Start-Sleep -Seconds $Seconds })
}
$done = @{}
$report = @()
$deadline = (Get-Date).AddSeconds(4 * ($Seconds + 60) + 120)
while (-not $proc.HasExited -and (Get-Date) -lt $deadline) {
    Start-Sleep -Milliseconds 100
    $text = if (Test-Path $err) { Get-Content $err -Raw -ErrorAction SilentlyContinue } else { '' }
    foreach ($mark in $phases.Keys) {
        if (-not $done[$mark] -and $text -and $text.Contains($mark)) {
            $done[$mark] = $true
            $from = @(Get-Content $err).Count
            $proc.Refresh(); $cpu0 = $proc.TotalProcessorTime.TotalMilliseconds
            $sw = [System.Diagnostics.Stopwatch]::StartNew()
            & $phases[$mark][1]
            $wrote = $sw.ElapsedMilliseconds
            # The watch's longest window (2 s once it backs off) and the follow thread after it.
            Start-Sleep -Seconds 8
            $proc.Refresh(); $cpu1 = $proc.TotalProcessorTime.TotalMilliseconds
            $lines = @(Get-Content $err | Select-Object -Skip $from)
            $wins = @($lines | Select-String -Pattern '^\[search\] pane=0 window: (\d+) records, (\d+) added, (\d+) removed, (\d+) changed(?:, (\d+) files read)?, in ([\d.]+) ms')
            $recs = ($wins | ForEach-Object { [int]$_.Matches[0].Groups[1].Value } | Measure-Object -Sum).Sum
            $added = ($wins | ForEach-Object { [int]$_.Matches[0].Groups[2].Value } | Measure-Object -Sum).Sum
            $read = ($wins | ForEach-Object { if ($_.Matches[0].Groups[5].Success) { [int]$_.Matches[0].Groups[5].Value } else { 0 } } | Measure-Object -Sum).Sum
            $ms = ($wins | ForEach-Object { [double]$_.Matches[0].Groups[6].Value } | Measure-Object -Sum -Maximum)
            $ended = @($lines | Select-String -SimpleMatch 'not following changes')
            $report += '{0,-5} wrote for {1} ms; windows {2}, records {3}, rows added {4}, files read {5}, follow thread {6:n0} ms in all (longest {7:n0} ms); process CPU {8:n0} ms over {9:n0} ms ({10:p0} of one core); {11}' -f
                $phases[$mark][0], $wrote, $wins.Count, [int]$recs, [int]$added, [int]$read, [double]$ms.Sum, [double]$ms.Maximum, ($cpu1 - $cpu0), $sw.ElapsedMilliseconds,
                (($cpu1 - $cpu0) / $sw.ElapsedMilliseconds), $(if ($ended.Count) { "following ended: $($ended[0].Line -replace '^.*not following changes ', '')" } else { 'still following' })
        }
    }
}
if (-not $proc.HasExited) { $proc.Kill(); 'timed out' }
$proc.WaitForExit()
"search: $(if ($Name) { 'name (*.txt)' } else { 'content (needle)' })"
$report
"panicked lines: $(@(Get-Content $err | Select-String -SimpleMatch 'panicked').Count); exit $($proc.ExitCode)"
