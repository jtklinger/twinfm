# P2-M1 soak: directory watching under churn. Both panes sit on watched scratch folders and
# scroll, while a separate PowerShell writer job creates, renames, deletes and appends to files
# in those folders, a few operations per second with a burst every 15 s.
#
#   powershell -File tools\soak-p2m1.ps1 -Minutes 30
#
# Builds %TEMP%\twinfm-m5\watch\w1 and w2 (2,000 small files each, so the lists scroll) and runs
# twinfm with TWINFM_SOAK=1, which fences the scripted `touch` and `rmfile` steps (and any file
# operation) to %TEMP%\twinfm-m3 and %TEMP%\twinfm-m5. Never set it outside a soak. The script
# uses no clipboard and no Recycle Bin: only the writer job and `touch`/`rmfile` change files.
#
# Each cycle: both panes on their folders, wheel both down and back, `touch` and `rmfile` a file
# in each pane with a `waitwatch` after each, a new tab on the left pane and back (the left
# folder's tab goes stale and refreshes when shown), Ctrl+R. The writer keeps changing both
# folders throughout, so most refreshes are watch-driven.
#
# At -Minutes the window is closed (WM_CLOSE, a clean exit) and the writer stopped. Reported:
# exit code, `panicked` lines, working set at start and end (sampled every 30 s into
# soak-p2m1-memory.csv), counts of `[watch]` and `[refresh]` lines, watch events delivered,
# watches lost, `waitwatch` timeouts, and the writer's operation count.
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
if (Test-Path $base) { Remove-Item -LiteralPath $base -Recurse -Force }
foreach ($d in $w1, $w2) {
    New-Item -ItemType Directory -Force $d | Out-Null
    for ($j = 1; $j -le $Files; $j++) {
        [System.IO.File]::WriteAllText((Join-Path $d ("f{0:0000}.txt" -f $j)), "scratch $j`r`n")
    }
}

$cycle = @(
    "nav 0 $w1", "nav 1 $w2", 'sleep 1000',
    'wheel 0 -40 30', 'wheel 1 -40 30', 'wheel 0 40 30', 'wheel 1 40 30',
    'touch 0 soak-touch.txt', 'waitwatch 3000',
    'touch 1 soak-touch.txt', 'waitwatch 3000',
    'rmfile 0 soak-touch.txt', 'waitwatch 3000',
    'rmfile 1 soak-touch.txt', 'waitwatch 3000',
    'tab new', 'sleep 500', 'tab close', 'sleep 1000',
    'key CtrlR', 'sleep 1000'
)
$steps = @('sleep 1500', 'split', 'sleep 1000')
for ($c = 0; $c -lt $Cycles; $c++) { $steps += $cycle }
$steps += 'quit'
$scriptFile = Join-Path $root 'soak-p2m1-script.txt'
[System.IO.File]::WriteAllText($scriptFile, ($steps -join ';'))

$t0 = Get-Date
$deadline = $t0.AddMinutes($Minutes)
# The writer: a separate process, so its file system activity is another program's, as for a
# real user. It stops at the deadline (or when the job is stopped).
$writer = Start-Job -ArgumentList $w1, $w2, $deadline -ScriptBlock {
    param($w1, $w2, $deadline)
    $rand = New-Object System.Random
    $ops = 0
    $nextBurst = (Get-Date).AddSeconds(15)
    while ((Get-Date) -lt $deadline) {
        $dir = @($w1, $w2)[$rand.Next(2)]
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
$changed = @($log | Select-String -SimpleMatch 'changed: refresh').Count
$deferred = @($log | Select-String -SimpleMatch 'changed while listing').Count
$lost = @($log | Select-String -SimpleMatch '] lost ').Count
$noWatch = @($log | Select-String -SimpleMatch 'no watch for').Count
$waitTimeouts = @($log | Select-String -SimpleMatch 'waitwatch: no watch-driven refresh').Count
$refused = @($log | Select-String -SimpleMatch 'soak: refused').Count
$cyclesRun = @($log | Select-String -SimpleMatch "[auto] Nav { pane: 0, path: `"$($w1.Replace('\', '\\'))`" }").Count
$first = $samples[0]; $last = $samples[$samples.Count - 1]
''
"ran {0:N1} min, window closed by script: {1}, exit code {2}" -f ((Get-Date) - $t0).TotalMinutes, $closed, $p.ExitCode
"panicked lines: $panics"
"cycles started: $cyclesRun; writer operations: $writerOps; soak refusals: $refused"
"[watch] lines: $watchLines (changes refreshed $changed, deferred behind a listing $deferred, lost $lost, no watch $noWatch); [refresh] lines: $refreshLines; waitwatch timeouts: $waitTimeouts"
"working set: start $($first.working_set_mb) MB at $($first.seconds) s, end $($last.working_set_mb) MB at $($last.seconds) s; private: $($first.private_mb) -> $($last.private_mb) MB"
"min/max working set: {0} / {1} MB" -f ($samples | Measure-Object working_set_mb -Minimum).Minimum, ($samples | Measure-Object working_set_mb -Maximum).Maximum
"memory samples: $csv"
"stderr: $stderr"
