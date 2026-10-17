# P5-M3 soak: the P4-M3 soak (file operation cycle, drops through the drop target, scripted context
# menus, a bookmark and a tab as drop targets, the drop menu, a right-button drag, the grid cycle,
# the big-folder scroll scenario) plus a spring-load and a crumb drop: the right pane goes to
# spring (made by the setup, holding the folder inner), a drag held 1300 ms over its row 0 opens
# inner there, two files are dropped on the breadcrumb's parent crumb (a copy into spring), a drag
# hovers the sidebar list's bottom edge for 300 ms (its auto-scroll, when the sidebar overflows)
# and the left pane's inactive tab for 300 ms (under SPRING_MS: armed, never opened), then the
# right pane goes back to spring and Shift+Delete removes the two copies.
#
#   powershell -File tools\soak-p5m3.ps1 -Minutes 30
#
# Same scratch tree, fence (TWINFM_SOAK=1, %TEMP%\twinfm-m5\), setup and reports as soak-p4m3.ps1,
# plus: spring armed, spring-loaded (rows, tabs: tabs must be 0), spring disarmed, crumb drops,
# and the sidebar and tab-strip auto-scroll lines (counted, not required: the sidebar scrolls only
# when it overflows). Never set TWINFM_SOAK outside a soak. One Recycle Bin entry per cycle; the
# clipboard is overwritten.
#
# P4-M3's grid steps run between the P2-M4 steps and the scroll scenario, while the left pane shows
# src (the tree alone) and the right pane dst: two files are dropped into dst, both panes go to
# the grid (Ctrl+Shift+2 on the active pane: a click makes a pane active), the drops land on tile
# 0 (a file: into the pane's folder, replacing) and on empty space, the band selects both files
# from the gap after the last tile (x 202, y 60 of the right list: the grid holds 5 tiles to a
# row at this window width), the right pane goes back to details (it is active after the band),
# Shift+Delete removes the selection, and the left pane goes back to details.
#
# P6-M2's medium-grid cycle follows: two files are dropped into dst again, the right pane goes to
# medium tiles (Ctrl+Shift+3), a drop lands on tile 0, a left-button drag out of it is cancelled
# (its 80 px drag image), the band selects both files from the gap after the last tile (x 202, y
# 40: tile 2 of a medium grid row starts at 160) and leaves the cursor on tile 0, Shift+Right
# extends the selection from there (anchor 0 to 1), the right pane goes back to details, and
# Shift+Delete removes the selection. Reported: medium switches, 80 px drag images, the bands' and
# the Shift+Right's lines.
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
public static class SoakWinP5m3 {
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
$spring = Join-Path $root 'spring'
foreach ($d in $src, $dst, $bm, $tabdir, $spring) {
    if (Test-Path $d) { Remove-Item -LiteralPath $d -Recurse -Force }
}
for ($i = 1; $i -le 10; $i++) {
    $sub = Join-Path $src ("tree\d{0:00}" -f $i)
    New-Item -ItemType Directory -Force $sub | Out-Null
    for ($j = 1; $j -le 20; $j++) {
        [System.IO.File]::WriteAllText((Join-Path $sub ("f{0:00}.txt" -f $j)), "scratch file $i/$j`r`n")
    }
}
foreach ($d in $dst, $bm, $tabdir, (Join-Path $spring 'inner')) { New-Item -ItemType Directory -Force $d | Out-Null }

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
    'dragrow 0 0 0 60 right', 'sleep 300', 'dragcancel', 'sleep 300',
    # P4-M3: the grid cycle (see the header).
    "dropfiles 1 -1 copy $pick", $wait,
    'click 1 0', 'view icons', 'click 0 0', 'view icons', 'sleep 300',
    "dropfiles 1 0 copy $pick", $wait,
    "dropfiles 1 -1 copy $pick", $wait,
    'rclick 0 0', 'sleep 300', 'menuclose', 'sleep 200',
    'dragrow 0 0 0 60', 'sleep 300', 'dragcancel', 'sleep 300',
    'dragrow 0 0 0 60 right', 'sleep 300', 'dragcancel', 'sleep 300',
    'band 1 202 60 5 10', 'sleep 200',
    'view details', 'sdelete', $wait,
    'click 0 0', 'view details', 'sleep 200',
    # P6-M2: the medium-grid cycle (see the header).
    "dropfiles 1 -1 copy $pick", $wait,
    'click 1 0', 'view medium', 'sleep 300',
    "dropfiles 1 0 copy $pick", $wait,
    'dragrow 1 0 0 40', 'sleep 300', 'dragcancel', 'sleep 300',
    'band 1 202 40 5 10', 'sleep 200',
    'key ShiftRight', 'sleep 200',
    'view details', 'sdelete', $wait,
    # P5-M3: the spring-load into spring\inner, the crumb drop into spring, the sidebar's bottom
    # edge, the inactive tab under the spring time, and the cleanup (see the header).
    "nav 1 $spring", 'sleep 500',
    "dragover row 1 0 1300 $pick", 'sleep 800',
    "dropfiles crumb 1 -2 copy $pick", $wait,
    "dragover side bottom 300 $pick", 'sleep 200',
    "dragover tab 0 1 300 $pick", 'sleep 200',
    "nav 1 $spring", 'sleep 500', 'click 1 1', 'shiftclick 1 2', 'sdelete', $wait
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
$scriptFile = Join-Path $root 'soak-p5m3-script.txt'
[System.IO.File]::WriteAllText($scriptFile, ($steps -join ';'))

$stderr = Join-Path $root 'soak-p5m3-stderr.txt'
$csv = Join-Path $root 'soak-p5m3-memory.csv'
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
    $closed = [SoakWinP5m3]::Close([uint32]$p.Id)
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
# P4-M3 counts.
$gridOn = @($log | Select-String -SimpleMatch 'details -> large icons').Count
$gridOff = @($log | Select-String -SimpleMatch 'large icons -> details').Count
$dropTile = @($drops | Select-String -SimpleMatch 'Row(1, 0) copy: 2 path(s), DragEnter effect copy, Drop reported copy').Count
$tileImages = @($log | Select-String -SimpleMatch 'drag image 128x128 px, 96 px icon').Count
$leftDrags = @($log | Select-String -SimpleMatch 'DoDragDrop starts (left button)').Count
$bandsStarted = @($log | Select-String -SimpleMatch '[band] pane=1 start at').Count
$bandsDone = @($log | Select-String -SimpleMatch '[band] pane=1 done: 2 selected').Count
$bandsCancelled = @($log | Select-String -SimpleMatch '[band] pane=1 cancelled').Count
# P6-M2 counts.
$mediumOn = @($log | Select-String -SimpleMatch 'details -> medium icons').Count
$mediumOff = @($log | Select-String -SimpleMatch 'medium icons -> details').Count
$mediumImages = @($log | Select-String -SimpleMatch 'drag image 80x80 px, 48 px icon').Count
$bandsCursor = @($log | Select-String -SimpleMatch '[band] pane=1 done: 2 selected, cursor 0').Count
$shiftFromCursor = @($log | Select-String -SimpleMatch '[select] pane=1 Range row=1 selected=2').Count
# P5-M3 counts.
$springArmed = @($log | Select-String -SimpleMatch '[dnd] spring armed:').Count
$springRows = @($log | Select-String -SimpleMatch '[dnd] spring-loaded:' | Select-String -SimpleMatch ' opens ').Count
$springTabs = @($log | Select-String -SimpleMatch '[dnd] spring-loaded:' | Select-String -SimpleMatch ' shows tab ').Count
$springDisarmed = @($log | Select-String -SimpleMatch '[dnd] spring disarmed').Count
$springInner = @($log | Select-String -SimpleMatch '[nav] pane=1 gen=' | Select-String -SimpleMatch "path=$(Join-Path $spring 'inner')").Count
$dropCrumb = @($drops | Select-String -SimpleMatch 'Crumb(1, -2) copy: 2 path(s), DragEnter effect copy, Drop reported copy').Count
$sideScrolls = @($log | Select-String -SimpleMatch '[dnd] auto-scroll sidebar').Count
$stripScrolls = @($log | Select-String -SimpleMatch 'tab strip left', 'tab strip right').Count
$menus = @($log | Select-String -SimpleMatch '[menu] closed').Count
$ticks = @($log | Select-String -SimpleMatch '[auto] in-menu tick')
$ticksBusy = @($ticks | Select-String -SimpleMatch 'free=false').Count
$first = $samples[0]; $last = $samples[$samples.Count - 1]
''
"ran {0:N1} min, window closed by script: {1}, exit code {2}" -f ((Get-Date) - $t0).TotalMinutes, $closed, $p.ExitCode
"panicked lines: $panics"
"cycles started: $cyclesRun; jobs done: $($done.Count) (failed $($failed.Count), cancelled $($cancelled.Count)); soak refusals: $refused; waitops timeouts: $stuck"
"drops: $($drops.Count) (pane copy $dropCopy, pane move $dropMove, bookmark copy $dropSide, tab copy $dropTab, menu copy $menuCopy, menu cancel $menuCancel, as expected: pane copy is three per cycle here), drop jobs submitted: $dropJobs"
"drop menus: $dropMenus (loops that found the app state busy: $dropMenuBusy)"
"right-button drags: $rightDrags; drags cancelled: $dragsCancelled, dropped: $dragsDropped (must be 0); drag loops that found the state busy: $dragBusy; drag images set: $images"
"menus opened and closed: $menus; in-menu ticks: $($ticks.Count), with state or driver busy: $ticksBusy"
"grid: switched on $gridOn, off $gridOff; tile drops: $dropTile (two per cycle: large and medium); 128 px drag images: $tileImages (left-button drags started: $leftDrags); bands: $bandsStarted started, $bandsDone done with 2 selected (two per cycle), $bandsCancelled cancelled"
"medium grid: switched on $mediumOn, off $mediumOff; 80 px drag images: $mediumImages; bands done with the cursor on tile 0: $bandsCursor (two per cycle); Shift+Right from the band's cursor: $shiftFromCursor"
"spring: armed $springArmed, spring-loaded rows $springRows (navigations into spring\inner: $springInner), tabs $springTabs (must be 0), disarmed $springDisarmed; crumb drops: $dropCrumb; auto-scroll lines: sidebar $sideScrolls, tab strip $stripScrolls"
"working set: start $($first.working_set_mb) MB at $($first.seconds) s, end $($last.working_set_mb) MB at $($last.seconds) s; private: $($first.private_mb) -> $($last.private_mb) MB"
"min/max working set: {0} / {1} MB" -f ($samples | Measure-Object working_set_mb -Minimum).Minimum, ($samples | Measure-Object working_set_mb -Maximum).Maximum
"memory samples: $csv"
"stderr: $stderr"
if ($failed.Count -gt 0) { 'failures:'; $failed | Select-Object -First 5 | ForEach-Object { "  $_" } }
