# M3 soak: file operations in a loop, interleaved with the big-folder scroll scenario.
#
#   powershell -File tools\soak-m3.ps1 -Minutes 30
#
# Builds a scratch tree under %TEMP%\twinfm-m3\ (src\tree: 10 folders x 20 small files, and an
# empty dst), writes a TWINFM_SCRIPT file with many cycles, and runs twinfm with TWINFM_SOAK=1.
# TWINFM_SOAK=1 adds FOF_NOCONFIRMATION to every job (so Shift+Delete does not stop at the shell's
# confirmation) and makes the app refuse any file operation whose paths leave %TEMP%\twinfm-m3.
# Never set it outside a soak.
#
# Each cycle: copy the tree to the other pane, rename it, move it back, delete it to the Recycle
# Bin, copy it again and delete that copy permanently, make a folder and rename it, Ctrl+C/Ctrl+V
# it across and delete it, Ctrl+X/Ctrl+V it across and delete it, refresh, then scroll bigdir,
# System32 and WinSxS\Manifests. One Recycle Bin entry (the 200-file tree) is added per cycle,
# and the clipboard is overwritten throughout: do not copy anything you need while it runs.
#
# At -Minutes the window is closed (WM_CLOSE, a clean exit); a job running at that moment is cut
# short, which can leave scratch files behind. Reported: exit code, `panicked` lines in stderr,
# job outcomes, and working set at start and end (sampled every 30 s into soak-m3-memory.csv).
param(
    [double]$Minutes = 10,
    [string]$Exe = (Join-Path $PSScriptRoot '..\target\release\twinfm.exe'),
    [string[]]$ScrollFolders = @('C:\traces\bigdir', 'C:\Windows\System32', 'C:\Windows\WinSxS\Manifests'),
    [int]$Cycles = 500
)
$ErrorActionPreference = 'Stop'
# The copy engine's progress window is an unowned top-level window of the same process, so
# Process.CloseMainWindow could pick it; close the app's own window (title "TwinFM - <folder>", "twinfm" before M6b) instead.
Add-Type @"
using System; using System.Text; using System.Runtime.InteropServices;
public static class SoakWin {
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
      if (p == pid && (t.ToString() == "twinfm" || t.ToString().StartsWith("TwinFM - "))) { found = h; return false; }
      return true;
    }, IntPtr.Zero);
    return found != IntPtr.Zero && PostMessage(found, 0x10, IntPtr.Zero, IntPtr.Zero);
  }
}
"@
$Exe = (Resolve-Path $Exe).Path
$root = Join-Path ([System.IO.Path]::GetTempPath()) 'twinfm-m3'
$src = Join-Path $root 'src'
$dst = Join-Path $root 'dst'
foreach ($d in $src, $dst) {
    if (Test-Path $d) { Remove-Item -LiteralPath $d -Recurse -Force }
}
for ($i = 1; $i -le 10; $i++) {
    $sub = Join-Path $src ("tree\d{0:00}" -f $i)
    New-Item -ItemType Directory -Force $sub | Out-Null
    for ($j = 1; $j -le 20; $j++) {
        [System.IO.File]::WriteAllText((Join-Path $sub ("f{0:00}.txt" -f $j)), "scratch file $i/$j`r`n")
    }
}
New-Item -ItemType Directory -Force $dst | Out-Null

$scroll = @($ScrollFolders | Where-Object { Test-Path $_ })
$wait = 'waitops 60000'
$cycle = @(
    "nav 0 $src", "nav 1 $dst", 'sleep 1000',
    'click 0 0', 'key CtrlA', 'copy', $wait,
    'click 1 0', 'rename moved', $wait,
    'click 1 0', 'move', $wait,
    'click 0 0', 'delete', $wait,
    'click 0 0', 'copy', $wait, 'click 1 0', 'sdelete', $wait,
    'newfolder nf', $wait, 'click 1 0', 'rename nf2', $wait,
    'click 1 0', 'cc', 'click 0 0', 'cv', $wait, 'click 0 0', 'sdelete', $wait,
    'click 1 0', 'cx', 'click 0 0', 'cv', $wait, 'click 0 0', 'sdelete', $wait,
    'key CtrlR', $wait
)
if ($scroll.Count -ge 1) { $cycle += @("nav 0 $($scroll[0])", 'sleep 1500', 'wheel 0 -40 30') }
if ($scroll.Count -ge 2) { $cycle += @("nav 1 $($scroll[1])", 'sleep 1500', 'wheel 1 -40 30') }
if ($scroll.Count -ge 3) { $cycle += @("nav 1 $($scroll[2])", 'sleep 1500', 'drag 1 300 1000') }
$steps = @('sleep 1500', 'split', 'sleep 1000')
for ($c = 0; $c -lt $Cycles; $c++) { $steps += $cycle }
$steps += 'quit'
$scriptFile = Join-Path $root 'soak-m3-script.txt'
[System.IO.File]::WriteAllText($scriptFile, ($steps -join ';'))

$stderr = Join-Path $root 'soak-m3-stderr.txt'
$csv = Join-Path $root 'soak-m3-memory.csv'
$env:TWINFM_SCRIPT = "@$scriptFile"
$env:TWINFM_SOAK = '1'
try {
    $p = Start-Process -FilePath $Exe -ArgumentList 'C:\' -RedirectStandardError $stderr -PassThru
    $null = $p.Handle # keep the handle so ExitCode is available after exit
} finally {
    $env:TWINFM_SOAK = $null
    $env:TWINFM_SCRIPT = $null
}
$t0 = Get-Date
$deadline = $t0.AddMinutes($Minutes)
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
    $closed = [SoakWin]::Close([uint32]$p.Id)
    if (-not $p.WaitForExit(60000)) { $p.Kill(); $p.WaitForExit(); $closed = 'killed' }
}
$samples | Export-Csv -NoTypeInformation -Path $csv

$log = Get-Content $stderr
$panics = @($log | Select-String -SimpleMatch 'panicked').Count
$done = @($log | Select-String -SimpleMatch '[fileops] done')
$failed = @($done | Select-String -SimpleMatch 'Failed')
$cancelled = @($done | Select-String -SimpleMatch 'Cancelled')
$refused = @($log | Select-String -SimpleMatch 'soak: refused').Count
$stuck = @($log | Select-String -SimpleMatch 'waitops: still busy').Count
$cyclesRun = @($log | Select-String -SimpleMatch "[auto] Nav { pane: 0, path: `"$($src.Replace('\', '\\'))`" }").Count
$first = $samples[0]; $last = $samples[$samples.Count - 1]
''
"ran {0:N1} min, window closed by script: {1}, exit code {2}" -f ((Get-Date) - $t0).TotalMinutes, $closed, $p.ExitCode
"panicked lines: $panics"
"cycles started: $cyclesRun; jobs done: $($done.Count) (failed $($failed.Count), cancelled $($cancelled.Count)); soak refusals: $refused; waitops timeouts: $stuck"
"working set: start $($first.working_set_mb) MB at $($first.seconds) s, end $($last.working_set_mb) MB at $($last.seconds) s; private: $($first.private_mb) -> $($last.private_mb) MB"
"min/max working set: {0} / {1} MB" -f ($samples | Measure-Object working_set_mb -Minimum).Minimum, ($samples | Measure-Object working_set_mb -Maximum).Maximum
"memory samples: $csv"
"stderr: $stderr"
if ($failed.Count -gt 0) { 'failures:'; $failed | Select-Object -First 5 | ForEach-Object { "  $_" } }
