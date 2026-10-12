# P2-M4 soak: the M5 soak (file operation cycle, drops through the drop target, scripted context
# menus, the big-folder scroll scenario) plus the P2-M4 drop paths: a drop on a sidebar bookmark,
# a drop on a tab, the right-button drop menu (Copy here, and Cancel), and a right-button drag that
# is cancelled.
#
#   powershell -File tools\soak-p2m4.ps1 -Minutes 30
#
# Builds a scratch tree under %TEMP%\twinfm-m5\ (the M5 fence: src\tree with 10 folders x 20 small
# files, an empty dst, and the targets bm and tabdir), writes a TWINFM_SCRIPT file with many cycles,
# and runs twinfm with TWINFM_SOAK=1. TWINFM_SOAK=1 adds FOF_NOCONFIRMATION to every job (drops
# included, so a copy onto files already there replaces them) and makes the app refuse any file
# operation whose paths leave %TEMP%\twinfm-m3 or %TEMP%\twinfm-m5. Never set it outside a soak.
#
# Before the cycles: bm is bookmarked (Ctrl+D, so it is the sidebar's last row, `side -1`), and the
# left pane gets a second tab on tabdir (tab 1; the cycles work in tab 0, on src).
#
# Each cycle: the M5 cycle (copy the tree to the other pane, rename it, move it back, delete it to
# the Recycle Bin, copy it again and delete that copy permanently, make a folder and rename it,
# Ctrl+C/Ctrl+V it across and delete it, Ctrl+X/Ctrl+V it across and delete it, refresh, drop two
# files on the right pane with Ctrl, drop those copies back on the left pane with Shift, delete
# them permanently, open and close the shell menu on a row and on the right pane's empty space),
# then: drop the two files on the bookmark with Ctrl (`dropfiles side -1 copy`), on the left
# pane's tab 1 with Ctrl (`dropfiles tab 0 1 copy`), on tab 1 again through the drop menu's "Copy
# here" (`dropfiles tab 0 1 menu copy`), and on the right pane's folder through the menu's
# "Cancel" (`dropfiles 1 -1 menu cancel`, nothing happens); then a right-button drag of the left
# pane's row 0 cancelled with Escape (`dragrow 0 0 0 60 right`, `dragcancel`); then the scroll
# scenario (bigdir, System32, WinSxS\Manifests). One Recycle Bin entry (the 200-file tree) is
# added per cycle, and the clipboard is overwritten throughout: do not copy anything you need
# while it runs.
#
# No OLE drop ever runs: `dropfiles` calls the drop target directly, and a scripted drag can only
# be cancelled (see the M5 record in docs/phase-1-plan.md).
#
# At -Minutes the window is closed (WM_CLOSE, a clean exit); a job running at that moment is cut
# short, which can leave scratch files behind. Reported: exit code, `panicked` lines in stderr,
# job outcomes, drops by kind, drop menus, right-button drags, menus, and working set and private
# bytes at start and end (sampled every 30 s into soak-p2m4-memory.csv).
param(
    [double]$Minutes = 30,
    [string]$Exe = (Join-Path $PSScriptRoot '..\target\release\twinfm.exe'),
    [string[]]$ScrollFolders = @('C:\traces\bigdir', 'C:\Windows\System32', 'C:\Windows\WinSxS\Manifests'),
    [int]$Cycles = 500
)
$ErrorActionPreference = 'Stop'
# The copy engine's progress window is an unowned top-level window of the same process, so
# Process.CloseMainWindow could pick it; close the app's own window (title "TwinFM - <folder>") instead.
Add-Type @"
using System; using System.Text; using System.Runtime.InteropServices;
public static class SoakWinP2m4 {
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
$src = Join-Path $root 'src'
$dst = Join-Path $root 'dst'
$bm = Join-Path $root 'bm'
$tabdir = Join-Path $root 'tabdir'
foreach ($d in $src, $dst, $bm, $tabdir) {
    if (Test-Path $d) { Remove-Item -LiteralPath $d -Recurse -Force }
}
for ($i = 1; $i -le 10; $i++) {
    $sub = Join-Path $src ("tree\d{0:00}" -f $i)
    New-Item -ItemType Directory -Force $sub | Out-Null
    for ($j = 1; $j -le 20; $j++) {
        [System.IO.File]::WriteAllText((Join-Path $sub ("f{0:00}.txt" -f $j)), "scratch file $i/$j`r`n")
    }
}
foreach ($d in $dst, $bm, $tabdir) { New-Item -ItemType Directory -Force $d | Out-Null }

$scroll = @($ScrollFolders | Where-Object { Test-Path $_ })
$wait = 'waitops 60000'
# Paths for the drops; `|` separates them (`;` separates script steps).
$pick = (Join-Path $src 'tree\d01\f01.txt') + '|' + (Join-Path $src 'tree\d01\f02.txt')
$back = (Join-Path $dst 'f01.txt') + '|' + (Join-Path $dst 'f02.txt')
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
    'key CtrlR', $wait,
    # M5 drops: copy two files into the empty right pane, move them back into the left pane's
    # folder (after the tree: rows 1 and 2), delete them.
    "dropfiles 1 -1 copy $pick", $wait,
    "dropfiles 0 -1 move $back", $wait,
    'click 0 1', 'shiftclick 0 2', 'sdelete', $wait,
    # Context menus on a row and on empty space, closed from the in-menu timer.
    'rclick 0 0', 'sleep 300', 'menuclose', 'sleep 200',
    'rclickempty 1', 'sleep 300', 'menuclose', 'sleep 200',
    # P2-M4: a bookmark and a tab as drop targets, the right-button drop menu, a right-button drag.
    "dropfiles side -1 copy $pick", $wait,
    "dropfiles tab 0 1 copy $pick", $wait,
    "dropfiles tab 0 1 menu copy $pick", $wait,
    "dropfiles 1 -1 menu cancel $pick", 'sleep 200',
    'dragrow 0 0 0 60 right', 'sleep 300', 'dragcancel', 'sleep 300'
)
if ($scroll.Count -ge 1) { $cycle += @("nav 0 $($scroll[0])", 'sleep 1500', 'wheel 0 -40 30') }
if ($scroll.Count -ge 2) { $cycle += @("nav 1 $($scroll[1])", 'sleep 1500', 'wheel 1 -40 30') }
if ($scroll.Count -ge 3) { $cycle += @("nav 1 $($scroll[2])", 'sleep 1500', 'drag 1 300 1000') }
# Setup, before the split (the left pane is the active one): bookmark bm (the sidebar's last row),
# a second tab on tabdir in the left pane (Ctrl+1 goes back to its tab 0), then the split.
$steps = @('sleep 1500', "nav 0 $bm", 'sleep 500', 'bookmark add', 'tab new', "nav 0 $tabdir", 'sleep 500',
    'tab 1', 'sleep 500', 'split', 'sleep 1000')
for ($c = 0; $c -lt $Cycles; $c++) { $steps += $cycle }
$steps += 'quit'
$scriptFile = Join-Path $root 'soak-p2m4-script.txt'
[System.IO.File]::WriteAllText($scriptFile, ($steps -join ';'))

$stderr = Join-Path $root 'soak-p2m4-stderr.txt'
$csv = Join-Path $root 'soak-p2m4-memory.csv'
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
    $closed = [SoakWinP2m4]::Close([uint32]$p.Id)
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
$drops = @($log | Select-String -SimpleMatch -CaseSensitive '[auto] dropfiles')
$dropCopy = @($drops | Select-String -SimpleMatch 'Row(1, -1) copy: 2 path(s), DragEnter effect copy, Drop reported copy').Count
$dropMove = @($drops | Select-String -SimpleMatch 'Row(0, -1) move: 2 path(s), DragEnter effect move, Drop reported none').Count
$dropSide = @($drops | Select-String -SimpleMatch 'Side(-1) copy: 2 path(s), DragEnter effect copy, Drop reported copy').Count
$dropTab = @($drops | Select-String -SimpleMatch 'Tab(0, 1) copy: 2 path(s), DragEnter effect copy, Drop reported copy').Count
$menuCopy = @($drops | Select-String -SimpleMatch 'Tab(0, 1) menu copy: 2 path(s), DragEnter effect move, Drop reported copy').Count
$menuCancel = @($drops | Select-String -SimpleMatch 'Row(1, -1) menu cancel: 2 path(s), DragEnter effect move, Drop reported none').Count
$dropJobs = @($log | Select-String -SimpleMatch '[dnd] drop into pane').Count
$dropMenus = @($log | Select-String -SimpleMatch '[dnd] drop menu:').Count
$dropMenuBusy = @($log | Select-String -SimpleMatch '[dnd] drop menu loop idle: app state free=false').Count
$rightDrags = @($log | Select-String -SimpleMatch 'DoDragDrop starts (right button)').Count
$dragsCancelled = @($log | Select-String -SimpleMatch '(cancelled)').Count
$dragsDropped = @($log | Select-String -SimpleMatch '(dropped)').Count
$dragBusy = @($log | Select-String -SimpleMatch 'inside the drag loop: app state free=false').Count
$images = @($log | Select-String -SimpleMatch 'InitializeFromBitmap = 0x00000000').Count
$menus = @($log | Select-String -SimpleMatch '[menu] closed').Count
$ticks = @($log | Select-String -SimpleMatch '[auto] in-menu tick')
$ticksBusy = @($ticks | Select-String -SimpleMatch 'free=false').Count
$first = $samples[0]; $last = $samples[$samples.Count - 1]
''
"ran {0:N1} min, window closed by script: {1}, exit code {2}" -f ((Get-Date) - $t0).TotalMinutes, $closed, $p.ExitCode
"panicked lines: $panics"
"cycles started: $cyclesRun; jobs done: $($done.Count) (failed $($failed.Count), cancelled $($cancelled.Count)); soak refusals: $refused; waitops timeouts: $stuck"
"drops: $($drops.Count) (pane copy $dropCopy, pane move $dropMove, bookmark copy $dropSide, tab copy $dropTab, menu copy $menuCopy, menu cancel $menuCancel, as expected), drop jobs submitted: $dropJobs"
"drop menus: $dropMenus (loops that found the app state busy: $dropMenuBusy)"
"right-button drags: $rightDrags; drags cancelled: $dragsCancelled, dropped: $dragsDropped (must be 0); drag loops that found the state busy: $dragBusy; drag images set: $images"
"menus opened and closed: $menus; in-menu ticks: $($ticks.Count), with state or driver busy: $ticksBusy"
"working set: start $($first.working_set_mb) MB at $($first.seconds) s, end $($last.working_set_mb) MB at $($last.seconds) s; private: $($first.private_mb) -> $($last.private_mb) MB"
"min/max working set: {0} / {1} MB" -f ($samples | Measure-Object working_set_mb -Minimum).Minimum, ($samples | Measure-Object working_set_mb -Maximum).Maximum
"memory samples: $csv"
"stderr: $stderr"
if ($failed.Count -gt 0) { 'failures:'; $failed | Select-Object -First 5 | ForEach-Object { "  $_" } }
