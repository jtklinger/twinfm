# P16-M5's scripted check of the List view: one run of target\release\twinfm.exe in a scratch tree,
# %TEMP%\twinfm-p16m5-check\work (three folders and 400 text files, one with a long name: fifteen
# columns of 27 at the default window, so the list scrolls sideways). No settings file. With
# SLINT_BACKEND=winit-software (the femtovg window captures blank); pictures are taken with
# PrintWindow from the window of the run's own process id into -Out.
# The steps: the left pane on work; `view list` (Ctrl+Shift+5); the keys (Down along the column,
# Right and Left a column across, PageDown and PageUp a screenful of columns, End and Home), each
# read back with `scrollpos` (the sideways scroll and the cursor); the wheel and a drag of the list's
# own scrollbar thumb; a drop onto a folder's cell (a copy), a hover over it (the drop highlight), a
# cancelled drag out of a cell; a band from under the last entry of the short last column; F2 (the
# rename box) and Escape; a new folder named from the toolbar and Escape; a right click on the empty
# space (the background menu); then the view by the settings dialog, small icons and back to details.
# Prints the [view], [list], [band] done, [dnd] drag image and Drop lines, the scrollpos lines and the
# [fileops] done lines.
param(
    [string]$Exe = (Join-Path $PSScriptRoot '..\target\release\twinfm.exe'),
    [string]$Out = (Join-Path $env:TEMP 'twinfm-p16m5-check\shots'),
    [string]$Scheme = ''
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public class P16M5Shot {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
}
"@
$root = Join-Path $env:TEMP 'twinfm-p16m5-check'
$work = Join-Path $root 'work'
$src = Join-Path $root 'src'
foreach ($d in $work, $src) { if (Test-Path $d) { Remove-Item -Recurse -Force $d } }
New-Item -ItemType Directory -Force (Join-Path $work 'Alpha'), (Join-Path $work 'Beta'), (Join-Path $work 'Gamma'), $src, $Out | Out-Null
1..399 | ForEach-Object { [System.IO.File]::WriteAllText((Join-Path $work ("note-{0:000}.txt" -f $_)), "note $_`r`n") }
[System.IO.File]::WriteAllText((Join-Path $work 'a file with quite a long name that does not fit in one cell.txt'), "long`r`n")
$dropped = Join-Path $src 'dropped.txt'
[System.IO.File]::WriteAllText($dropped, "dropped`r`n")

# work lists Alpha, Beta, Gamma, then the files by name: 403 entries in columns of 27, the last column holds 25.
$wait = 'waitops 60000'
$steps = @(
    'sleep 2500', "nav 0 $work", 'sleep 1000',
    'view list', 'sleep 1501',
    'click 0 4', 'sleep 200', 'key Down', 'sleep 150', 'scrollpos 0', 'key Right', 'sleep 150', 'scrollpos 0', 'key Left', 'sleep 150', 'scrollpos 0',
    'key PageDown', 'sleep 200', 'scrollpos 0', 'key PageDown', 'sleep 200', 'scrollpos 0', 'key PageUp', 'sleep 200', 'scrollpos 0',
    'key End', 'sleep 1502', 'scrollpos 0',
    # The band, from under the last entry of the short last column (25 of 27 rows) up into its last three.
    'band 0 900 580 850 500', 'sleep 400',
    'key Home', 'sleep 300', 'scrollpos 0',
    'wheelat 600 400 -300', 'sleep 300', 'scrollpos 0', 'key Home', 'sleep 300', 'drag 0 150 400', 'sleep 300', 'scrollpos 0', 'key Home', 'sleep 300',
    "dropfiles 0 1 copy $dropped", $wait, 'sleep 300',
    "dragover row 0 1 950 $dropped", 'sleep 200',
    'dragrow 0 5 40 10', 'sleep 400', 'dragcancel', 'sleep 300',
    'click 0 6', 'key F2', 'sleep 1504', 'key Escape', 'sleep 300',
    'toolbar 0 newfolder', 'sleep 1505', 'key Escape', 'sleep 300',
    'key End', 'sleep 500', 'rclickempty 0', 'waitmenu 2000', 'sleep 1506', 'menuclose', 'sleep 300', 'key Home', 'sleep 300',
    'view details', 'sleep 500',
    'settings open', 'sleep 300', 'settings set view_left list', 'settings ok', 'sleep 600',
    'view small', 'sleep 500', 'view list', 'sleep 500', 'view details', 'sleep 500',
    'quit'
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
$shots = [ordered]@{
    '[auto] Sleep(1501)' = 'list.png'; '[auto] Sleep(1502)' = 'list-end.png'; 'ms: 950' = 'list-drop.png'
    '[auto] Sleep(1504)' = 'list-rename.png'; '[auto] Sleep(1505)' = 'list-newfolder.png'; '[auto] Sleep(1506)' = 'list-menu.png'
}
$taken = @{}
$deadline = (Get-Date).AddSeconds(180)
while (-not $proc.HasExited -and (Get-Date) -lt $deadline) {
    Start-Sleep -Milliseconds 40
    $text = if (Test-Path $err) { Get-Content $err -Raw -ErrorAction SilentlyContinue } else { '' }
    foreach ($mark in $shots.Keys) {
        if (-not $taken[$mark] -and $text -and $text.Contains($mark)) {
            Start-Sleep -Milliseconds 350
            $proc.Refresh()
            $h = $proc.MainWindowHandle
            $r = New-Object P16M5Shot+RECT
            [P16M5Shot]::GetWindowRect($h, [ref]$r) | Out-Null
            $bmp = New-Object System.Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
            $g = [System.Drawing.Graphics]::FromImage($bmp)
            $hdc = $g.GetHdc()
            [P16M5Shot]::PrintWindow($h, $hdc, 2) | Out-Null
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
Get-Content $err | Where-Object { $_ -match '^\[view\]|^\[list\]|^\[grid\]|^\[band\] pane=\d+ (done|press)|^\[dnd\] (drag image|Drop at|over pane)|^\[fileops\] done|^\[winmenu\] pane=\d+ seq=\d+ (item|background) menu|^\[settings\] dialog: view|^\[auto\] (scrollpos|.*(no such|refused|disabled))|panicked' } | ForEach-Object { $_ -replace [regex]::Escape($root), '...' }
"exit $($proc.ExitCode)"
