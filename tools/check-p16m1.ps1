# P16-M1's scripted check of the drag cue: one run of target\release\twinfm.exe in a scratch tree,
# %TEMP%\twinfm-p16m1-check (left with three files; right with the folders Documents and Photos and a
# file). No settings file. With SLINT_BACKEND=winit-software (the femtovg window captures blank);
# pictures are taken with PrintWindow from the window of the run's own process id into -Out.
# The steps: both panes shown, the left on left, the right on right; a drag of left's three files
# hovers (dragover: DragEnter, DragOver, DragLeave, never a drop) the right pane's Documents row (a
# move, then with Ctrl a copy, then Ctrl let go and Shift pressed inside one hover), the right pane's
# list below its rows, a sidebar row, the right pane's tab and its parent crumb; then the setting,
# staged with `settings set` and closed by a click on the dialog's real Cancel and OK (`clickat`: the
# buttons sat outside the dialog's rectangle, out of the mouse's reach, until the console check's
# fix): "Always copy" (the hover reads Copy, Ctrl gives a move, a dropfiles auto copies on the same
# drive), "Always move" (Ctrl gives a copy), "Always ask" (the hover reads Choose on drop, with Ctrl
# held too; every left-button drop opens the menu, a key held or not), and back to Explorer's rule;
# last the path bar's Up, held 2600 ms by a drag, and Back, held 1300 ms (the right pane goes from
# Documents up to right, once only however long Up is held, and back to Documents; nothing is dropped
# there), and a sidebar row (row 1, the system
# drive) held 1300 ms by a drag counted as TwinFM's own out of the left pane (`dragfrom 0`): it opens
# in the right pane, the one the drag did not leave. A release on Up (`dropfiles nav 1 up`) must read
# "DragEnter effect copy, Drop reported none" with no [fileops] line after it.
# Prints the [dnd] cue, drop menu, spring and Drop lines, the [settings] dialog lines, the [nav] history
# lines and the [fileops] done lines. The window is the scripted default, 1200 x 800: the dialog's
# Cancel is at (744, 743) and its OK at (800, 743).
param(
    [string]$Exe = (Join-Path $PSScriptRoot '..\target\release\twinfm.exe'),
    [string]$Out = (Join-Path $env:TEMP 'twinfm-p16m1-check\shots'),
    [string]$Scheme = '',
    [int]$SideRow = 2
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public class P16M1Shot {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
}
"@
$root = Join-Path $env:TEMP 'twinfm-p16m1-check'
$work = Join-Path $root 'work'
if (Test-Path $work) { Remove-Item -Recurse -Force $work }
$left = Join-Path $work 'left'
$right = Join-Path $work 'right'
New-Item -ItemType Directory -Force $left, (Join-Path $right 'Documents'), (Join-Path $right 'Photos'), $Out | Out-Null
'a.txt', 'b.txt', 'c.txt' | ForEach-Object { [System.IO.File]::WriteAllText((Join-Path $left $_), "file $_`r`n") }
[System.IO.File]::WriteAllText((Join-Path $right 'readme.txt'), "readme`r`n")
$files = ('a.txt', 'b.txt', 'c.txt' | ForEach-Object { Join-Path $left $_ }) -join '|'
$one = Join-Path $left 'a.txt'

# right lists Documents, Photos, readme.txt (rows 0 to 2).
$wait = 'waitops 60000'
$steps = @(
    'sleep 2500', "nav 0 $left", 'sleep 500', 'split', 'sleep 500', "nav 1 $right", 'sleep 800', 'click 0 0', 'sleep 300',
    # Every hover over a row, a tab, a crumb or a sidebar row lasts under the spring time (1000 ms).
    # Explorer's rule on one drive: a move; with Ctrl a copy; Ctrl let go and Shift pressed mid-hover.
    "dragover row 1 0 901 $files", 'sleep 200',
    "dragover row 1 0 902 keys ctrl $files", 'sleep 200',
    "dragover row 1 0 960 keys none,ctrl,none,shift $files", 'sleep 200',
    # The list below its rows, a sidebar row, the tab, the parent crumb.
    "dragover 1 bottom 903 $files", 'sleep 200',
    "dragover 1 bottom 904 keys ctrl $one", 'sleep 200',
    "dragover side $SideRow 907 $files", 'sleep 200',
    "dragover tab 1 0 908 $files", 'sleep 200',
    "dragover crumb 1 -2 909 $files", 'sleep 200',
    # The right button asks whatever the setting.
    "dragover row 1 0 905 keys right $files", 'sleep 200',
    # The dialog's own buttons by the mouse: Cancel discards a staged change, OK applies one.
    'settings open', 'sleep 300', 'settings set drag_default move', 'clickat 744 743', 'sleep 300',
    # Always copy: the hover reads Copy on the same drive, Ctrl gives the move, Shift still moves,
    # and a drop copies.
    'settings open', 'sleep 300', 'settings set drag_default copy', 'clickat 800 743', 'sleep 300',
    "dragover row 1 0 600 $files", 'sleep 200',
    "dragover row 1 0 600 keys ctrl $files", 'sleep 200',
    "dragover row 1 0 600 keys shift $files", 'sleep 200',
    "dropfiles 1 0 auto $one", $wait,
    # Always move: Ctrl still copies.
    'settings open', 'sleep 300', 'settings set drag_default move', 'settings ok', 'sleep 300',
    "dragover row 1 0 600 $files", 'sleep 200',
    "dragover row 1 0 600 keys ctrl $files", 'sleep 200',
    # Always ask: the hover says so, with Ctrl held too; every left-button drop opens the menu
    # (Cancel, then Copy here picked; then one with a key held, which asks all the same: Cancel).
    'settings open', 'sleep 300', 'settings set drag_default ask', 'settings ok', 'sleep 300',
    "dragover row 1 0 906 $files", 'sleep 200',
    "dragover row 1 0 600 keys ctrl $files", 'sleep 200',
    "dropfiles 1 1 ask cancel $(Join-Path $left 'b.txt')", $wait,
    "dropfiles 1 1 ask copy $(Join-Path $left 'b.txt')", $wait,
    "dropfiles 1 1 copy $(Join-Path $left 'c.txt')", $wait,
    # Back to Explorer's rule.
    'settings open', 'sleep 300', 'settings set drag_default explorer', 'settings ok', 'sleep 300',
    "dragover row 1 0 600 $files", 'sleep 200',
    # The path bar's Up and Back, held by a drag: up out of Documents, then back into it.
    "nav 1 $(Join-Path $right 'Documents')", 'sleep 600',
    # Up held for 2600 ms goes up once: one "goes up" line, and Back then finds Documents.
    "dragover nav 1 up 2600 $files", 'sleep 500',
    "dragover nav 1 back 1300 $files", 'sleep 500',
    # Let go on Up: the source was shown an effect (no refusal sign), and nothing is copied or moved.
    "dropfiles nav 1 up auto $(Join-Path $left 'b.txt')", $wait,
    # A sidebar row held by TwinFM's own drag out of the left pane opens in the right pane.
    'dragfrom 0', "dragover side 1 1300 $files", 'sleep 600', 'dragfrom none',
    'sleep 300', 'quit'
)
$script = Join-Path $root 'script.txt'
[System.IO.File]::WriteAllText($script, ($steps -join ';'))
$err = Join-Path $root 'stderr.txt'
$env:TWINFM_SCRIPT = "@$script"
$env:SLINT_BACKEND = 'winit-software'
if ($Scheme) { $env:TWINFM_SCHEME = $Scheme }
Remove-Item Env:TWINFM_CONFIG -ErrorAction SilentlyContinue
try {
    $proc = Start-Process -FilePath $Exe -PassThru -RedirectStandardError $err
    $null = $proc.Handle # kept, or ExitCode reads blank after the exit
} finally {
    Remove-Item Env:TWINFM_SCRIPT, Env:SLINT_BACKEND, Env:TWINFM_SCHEME -ErrorAction SilentlyContinue
}
$tag = if ($Scheme) { $Scheme } else { 'cue' }
$shots = [ordered]@{
    'ms: 901' = "$tag-move-row.png"; 'ms: 902' = "$tag-copy-row.png"; 'ms: 903' = "$tag-move-list.png"
    'ms: 904' = "$tag-copy-list.png"; 'ms: 905' = "$tag-ask-right.png"; 'ms: 906' = "$tag-ask-setting.png"
    'ms: 907' = "$tag-side.png"; 'ms: 908' = "$tag-tab.png"; 'ms: 909' = "$tag-crumb.png"
}
$taken = @{}
$deadline = (Get-Date).AddSeconds(180)
while (-not $proc.HasExited -and (Get-Date) -lt $deadline) {
    Start-Sleep -Milliseconds 40
    $text = if (Test-Path $err) { Get-Content $err -Raw -ErrorAction SilentlyContinue } else { '' }
    foreach ($mark in $shots.Keys) {
        if (-not $taken[$mark] -and $text -and $text.Contains($mark)) {
            Start-Sleep -Milliseconds 250
            $proc.Refresh()
            $h = $proc.MainWindowHandle
            $r = New-Object P16M1Shot+RECT
            [P16M1Shot]::GetWindowRect($h, [ref]$r) | Out-Null
            $bmp = New-Object System.Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
            $g = [System.Drawing.Graphics]::FromImage($bmp)
            $hdc = $g.GetHdc()
            [P16M1Shot]::PrintWindow($h, $hdc, 2) | Out-Null
            $g.ReleaseHdc($hdc)
            $png = Join-Path $Out $shots[$mark]
            $bmp.Save($png, [System.Drawing.Imaging.ImageFormat]::Png)
            $g.Dispose(); $bmp.Dispose()
            $taken[$mark] = $true
            "captured: $png"
        }
    }
}
if (-not $proc.HasExited) { $proc.Kill(); 'timed out' }
$proc.WaitForExit()
Get-Content $err | Where-Object { $_ -match '^\[dnd\] (cue|drop menu|Drop at|over |drop into|spring)|^\[settings\] dialog|^\[nav\] pane=\d history|^\[fileops\] done|^\[auto\] (dragover|dropfiles)|panicked' }
"exit $($proc.ExitCode)"
