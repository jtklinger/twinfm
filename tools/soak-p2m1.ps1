# P2-M1 soak, extended in P5-M1: directory watching under churn. Both panes sit on watched scratch
# folders and scroll, while a separate PowerShell writer job creates, renames, deletes and appends
# to files in those folders, a few operations per second with a burst every 15 s.
#
#   powershell -File tools\soak-p2m1.ps1 -Minutes 30
#
# Builds %TEMP%\twinfm-m5\watch\w1, w2 and w3 (2,000 small files each, so the lists scroll) and runs
# twinfm with TWINFM_SOAK=1, which fences the scripted `touch` and `rmfile` steps (and any file
# operation) to %TEMP%\twinfm-m3 and %TEMP%\twinfm-m5. Never set it outside a soak. The script
# uses no clipboard and no Recycle Bin: only the writer job and `touch`/`rmfile` change files.
#
# Before the cycles the left pane opens a second tab on w3 and goes back to the first, so w3 is a
# watched background tab the writer churns. Each cycle: both panes on their folders, wheel both
# down and back, `touch` a file in the left pane (`waitedit`: the edit path), `mvfile` it and
# `rmfile` it, `touch` and `rmfile` one in the right pane, with a `waitwatch` after each; a switch
# to the w3 tab and back (no refresh expected: it is watched); a new tab on the left pane and
# back; Ctrl+R. The writer keeps changing all three folders throughout.
#
# At -Minutes the window is closed (WM_CLOSE, a clean exit) and the writer stopped. Reported:
# exit code, `panicked` lines, working set at start and end (sampled every 30 s into
# soak-p2m1-memory.csv), counts of `[watch]` and `[refresh]` lines, records delivered, windows
# applied as row edits (with their +added -removed ~changed sums), fallbacks by reason, windows
# netted to nothing or explained by an operation, background watches and tabs past the cap,
# back-off lines and the largest window, edits dropped, refreshes on the switches to the w3 tab,
# watches lost, `waitwatch`/`waitedit` timeouts, and the writer's operation count.
param(
    [double]$Minutes = 10,
    [string]$Exe = (Join-Path $PSScriptRoot '..\target\release\twinfm.exe'),
    [int]$Files = 2000,
    [int]$Cycles = 2000
)
$ErrorActionPreference = 'Stop'
Add-Type @"
using System; using System.Text; using System.Runtime.InteropServices;
public static class SoakWinP2 {
  delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc f, IntPtr l);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  public static bool Close(uint pid) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, l) => {
      uint p; GetWindowThreadProcessId(h, out p);
      var t = new StringBuilder(64); GetWindowText(h, t, 64);
      if (p == pid && t.ToString().StartsWith("TwinFM - ")) { found = h; return false; }
      return true;
    }, IntPtr.Zero);
    return found != IntPtr.Zero && PostMessage(found, 0x10, IntPtr.Zero, IntPtr.Zero);
  }
}
"@
$Exe = (Resolve-Path $Exe).Path
$root = Join-Path ([System.IO.Path]::GetTempPath()) 'twinfm-m5'
$base = Join-Path $root 'watch'
$w1 = Join-Path $base 'w1'
$w2 = Join-Path $base 'w2'
$w3 = Join-Path $base 'w3'
if (Test-Path $base) { Remove-Item -LiteralPath $base -Recurse -Force }
foreach ($d in $w1, $w2, $w3) {
    New-Item -ItemType Directory -Force $d | Out-Null
    for ($j = 1; $j -le $Files; $j++) {
        [System.IO.File]::WriteAllText((Join-Path $d ("f{0:0000}.txt" -f $j)), "scratch $j`r`n")
    }
}

$cycle = @(
    "nav 0 $w1", "nav 1 $w2", 'sleep 1000',
    'wheel 0 -40 30', 'wheel 1 -40 30', 'wheel 0 40 30', 'wheel 1 40 30',
    'touch 0 soak-touch.txt', 'waitedit 3000',
    'mvfile 0 soak-touch.txt soak-moved.txt', 'waitwatch 3000',
    'touch 1 soak-touch.txt', 'waitwatch 3000',
    'rmfile 0 soak-moved.txt', 'waitwatch 3000',
    'rmfile 1 soak-touch.txt', 'waitwatch 3000',
    'click 0 0', 'tab 2', 'sleep 1000', 'tab 1', 'sleep 500',
    'tab new', 'sleep 500', 'tab close', 'sleep 500', 'tab 1', 'sleep 1000',
    'key CtrlR', 'sleep 1000'
)
# The left pane's second tab on w3, then back to the first: w3 is watched in the background.
$steps = @('sleep 1500', 'split', 'sleep 1000', 'click 0 0', 'tab new', 'sleep 500', "nav 0 $w3", 'sleep 1500', 'tab 1', 'sleep 1000')
for ($c = 0; $c -lt $Cycles; $c++) { $steps += $cycle }
$steps += 'quit'
$scriptFile = Join-Path $root 'soak-p2m1-script.txt'
[System.IO.File]::WriteAllText($scriptFile, ($steps -join ';'))

$t0 = Get-Date
$deadline = $t0.AddMinutes($Minutes)
# The writer: a separate process, so its file system activity is another program's, as for a
# real user. It stops at the deadline (or when the job is stopped).
$writer = Start-Job -ArgumentList $w1, $w2, $w3, $deadline -ScriptBlock {
    param($w1, $w2, $w3, $deadline)
    $rand = New-Object System.Random
    $ops = 0
    $nextBurst = (Get-Date).AddSeconds(15)
    while ((Get-Date) -lt $deadline) {
        $dir = @($w1, $w2, $w3)[$rand.Next(3)]
        $n = $rand.Next(50)
        $f = Join-Path $dir "churn-$n.txt"
        try {
            switch ($rand.Next(4)) {
                0 { [System.IO.File]::WriteAllText($f, "churn`r`n") }
                1 { if (Test-Path $f) { Move-Item -LiteralPath $f -Destination (Join-Path $dir ("churn-{0}.txt" -f $rand.Next(50))) -Force } }
                2 { if (Test-Path $f) { Remove-Item -LiteralPath $f -Force } }
                3 { [System.IO.File]::AppendAllText($f, "more`r`n") }
            }
            $ops++
        } catch { }
        if ((Get-Date) -ge $nextBurst) {
            for ($i = 0; $i -lt 100; $i++) {
                $b = Join-Path $dir "burst-$i.txt"
                try { [System.IO.File]::WriteAllText($b, 'b'); $ops++ } catch { }
            }
            for ($i = 0; $i -lt 100; $i++) {
                try { Remove-Item -LiteralPath (Join-Path $dir "burst-$i.txt") -Force; $ops++ } catch { }
            }
            $nextBurst = (Get-Date).AddSeconds(15)
        }
        Start-Sleep -Milliseconds 300
    }
    $ops
}

$stderr = Join-Path $root 'soak-p2m1-stderr.txt'
$csv = Join-Path $root 'soak-p2m1-memory.csv'
$env:TWINFM_SCRIPT = "@$scriptFile"
$env:TWINFM_SOAK = '1'
try {
    $p = Start-Process -FilePath $Exe -ArgumentList $w1 -RedirectStandardError $stderr -PassThru
    $null = $p.Handle # keep the handle so ExitCode is available after exit
} finally {
    $env:TWINFM_SOAK = $null
    $env:TWINFM_SCRIPT = $null
}
$samples = New-Object System.Collections.Generic.List[object]
function Sample {
    $p.Refresh()
    $samples.Add([pscustomobject]@{
        seconds        = [int]((Get-Date) - $t0).TotalSeconds
        working_set_mb = [math]::Round($p.WorkingSet64 / 1MB, 1)
        private_mb     = [math]::Round($p.PrivateMemorySize64 / 1MB, 1)
    })
}
Start-Sleep -Seconds 5
if (-not $p.HasExited) { Sample }
$next = (Get-Date).AddSeconds(30)
while (-not $p.HasExited -and (Get-Date) -lt $deadline) {
    Start-Sleep -Seconds 1
    if ((Get-Date) -ge $next -and -not $p.HasExited) { Sample; $next = $next.AddSeconds(30) }
}
$closed = $false
if (-not $p.HasExited) {
    Sample
    $closed = [SoakWinP2]::Close([uint32]$p.Id)
    if (-not $p.WaitForExit(60000)) { $p.Kill(); $p.WaitForExit(); $closed = 'killed' }
}
$writerOps = Receive-Job -Job $writer -Wait -AutoRemoveJob
$samples | Export-Csv -NoTypeInformation -Path $csv

$log = Get-Content $stderr
$panics = @($log | Select-String -SimpleMatch 'panicked').Count
$watchLines = @($log | Select-String -SimpleMatch '[watch]').Count
$refreshLines = @($log | Select-String -SimpleMatch '[refresh]').Count
$changed = @($log | Select-String -Pattern '\[watch\].* fallback: .*: refresh$').Count
$deferred = @($log | Select-String -Pattern '\[watch\].* fallback: .*: refresh after it$').Count
$staled = @($log | Select-String -Pattern '\[watch\].* fallback: .*: stale$').Count
$lost = @($log | Select-String -SimpleMatch '] lost ').Count
$noWatch = @($log | Select-String -SimpleMatch 'no watch for').Count
$waitTimeouts = @($log | Select-String -SimpleMatch 'waitwatch: no ').Count
$editTimeouts = @($log | Select-String -SimpleMatch 'waitedit: no ').Count
# P5-M1: records, applied windows, fallbacks by reason, and the rest of the watch's lines.
$records = 0; $largest = 0
foreach ($m in ($log | Select-String -Pattern '\[watch\].* records (\d+), changes \d+, window (\d+) ms')) {
    $records += [int]$m.Matches[0].Groups[1].Value
    $largest = [math]::Max($largest, [int]$m.Matches[0].Groups[2].Value)
}
$applied = 0; $plus = 0; $minus = 0; $tilde = 0
foreach ($m in ($log | Select-String -Pattern 'applied: \+(\d+) -(\d+) ~(\d+)')) {
    $applied++
    $plus += [int]$m.Matches[0].Groups[1].Value
    $minus += [int]$m.Matches[0].Groups[2].Value
    $tilde += [int]$m.Matches[0].Groups[3].Value
}
$reasons = @{}
foreach ($m in ($log | Select-String -Pattern '\[watch\].* fallback: (.*?)(?: \(\d+ records\))?: (refresh after it|refresh|stale)$')) {
    $r = $m.Matches[0].Groups[1].Value -replace '^unknown name .*', 'unknown name' -replace '^\d+ changes over the cap', 'N changes over the cap'
    $reasons[$r] = 1 + [int]$reasons[$r]
}
$fallbacks = ($reasons.GetEnumerator() | Sort-Object Value -Descending | ForEach-Object { "$($_.Key) $($_.Value)" }) -join ', '
$netted = @($log | Select-String -SimpleMatch 'netted to nothing').Count
$explained = @($log | Select-String -SimpleMatch 'explained by the operation').Count
$bgWatched = @($log | Select-String -SimpleMatch 'watched in the background').Count
$pastCap = @($log | Select-String -SimpleMatch 'past the cap').Count
$backoff = @($log | Select-String -SimpleMatch 'back-off:').Count
$dropped = @($log | Select-String -SimpleMatch 'edits dropped').Count
# A switch to the w3 tab and the lines up to the next driver step: a [refresh] there is a refresh
# on the switch.
$switches = 0; $switchRefreshes = 0; $inSwitch = $false
foreach ($line in $log) {
    if ($line.StartsWith('[tab] pane=0 switched to 2 of ')) { $switches++; $inSwitch = $true; continue }
    if ($line.StartsWith('[auto]')) { $inSwitch = $false }
    if ($inSwitch -and $line.StartsWith('[refresh] pane=0 ')) { $switchRefreshes++; $inSwitch = $false }
}
$refused = @($log | Select-String -SimpleMatch 'soak: refused').Count
$cyclesRun = @($log | Select-String -SimpleMatch "[auto] Nav { pane: 0, path: `"$($w1.Replace('\', '\\'))`" }").Count
$first = $samples[0]; $last = $samples[$samples.Count - 1]
''
"ran {0:N1} min, window closed by script: {1}, exit code {2}" -f ((Get-Date) - $t0).TotalMinutes, $closed, $p.ExitCode
"panicked lines: $panics"
"cycles started: $cyclesRun; writer operations: $writerOps; soak refusals: $refused"
"[watch] lines: $watchLines (fallbacks refreshed $changed, deferred behind a listing $deferred, background stale $staled, lost $lost, no watch $noWatch); [refresh] lines: $refreshLines; waitwatch timeouts: $waitTimeouts; waitedit timeouts: $editTimeouts"
"records delivered: $records; windows applied: $applied (+$plus -$minus ~$tilde); edits dropped: $dropped"
"fallbacks by reason: $fallbacks"
"netted to nothing: $netted; explained by the operation: $explained; watched in the background: $bgWatched; past the cap: $pastCap; back-off lines: $backoff; largest window: $largest ms"
"switches to the w3 tab: $switches, with a [refresh] on the switch: $switchRefreshes"
"working set: start $($first.working_set_mb) MB at $($first.seconds) s, end $($last.working_set_mb) MB at $($last.seconds) s; private: $($first.private_mb) -> $($last.private_mb) MB"
"min/max working set: {0} / {1} MB" -f ($samples | Measure-Object working_set_mb -Minimum).Minimum, ($samples | Measure-Object working_set_mb -Maximum).Maximum
"memory samples: $csv"
"stderr: $stderr"
