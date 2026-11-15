# P16-M1's scripted check of the drag cue: one run of target\release\twinfm.exe in a scratch tree,
# %TEMP%\twinfm-p16m1-check (left with three files; right with the folders Documents and Photos and a
# file). No settings file. With SLINT_BACKEND=winit-software (the femtovg window captures blank);
# pictures are taken with PrintWindow from the window of the run's own process id into -Out.
# The steps: both panes shown, the left on left, the right on right; a drag of left's three files
# hovers (dragover: DragEnter, DragOver, DragLeave, never a drop) the right pane's Documents row (a
# move, then with Ctrl a copy, then Ctrl let go and Shift pressed inside one hover), the right pane's
# list below its rows, a sidebar row, the right pane's tab and its parent crumb; then the setting:
# "Always copy" (the hover reads Copy, a dropfiles auto copies on the same drive), "Always move",
# "Always ask" (the hover reads Choose on drop; a left-button drop opens the menu, Copy here picked;
# Ctrl held does not ask), and back to Explorer's rule.
# Prints the [dnd] cue, drop menu and Drop lines, the [settings] dialog lines and the [fileops] done lines.
param(
    [string]$Exe = (Join-Path $PSScriptRoot '..\target\release\twinfm.exe'),
    [string]$Out = (Join-Path $env:TEMP 'twinfm-p16m1-check\shots'),
    # MOCK-UPS (removed once Jeremy has picked): 1 the badge on the target, 2 a banner, 3 the badge and a tinted highlight.
    [int]$Mock = 1,
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
    # Always copy: the hover reads Copy on the same drive, Shift still moves, and a drop copies.
    'settings open', 'sleep 300', 'settings set drag_default copy', 'settings ok', 'sleep 300',
    "dragover row 1 0 600 $files", 'sleep 200',
    "dragover row 1 0 600 keys shift $files", 'sleep 200',
    "dropfiles 1 0 auto $one", $wait,
    # Always move: Ctrl still copies.
    'settings open', 'sleep 300', 'settings set drag_default move', 'settings ok', 'sleep 300',
    "dragover row 1 0 600 $files", 'sleep 200',
    "dragover row 1 0 600 keys ctrl $files", 'sleep 200',
    # Always ask: the hover says so; a left-button drop opens the menu (Cancel, then Copy here picked);
    # Ctrl held does not ask.
    'settings open', 'sleep 300', 'settings set drag_default ask', 'settings ok', 'sleep 300',
    "dragover row 1 0 906 $files", 'sleep 200',
    "dragover row 1 0 600 keys ctrl $files", 'sleep 200',
    "dropfiles 1 1 ask cancel $(Join-Path $left 'b.txt')", $wait,
    "dropfiles 1 1 ask copy $(Join-Path $left 'b.txt')", $wait,
    "dropfiles 1 1 copy $(Join-Path $left 'c.txt')", $wait,
    # Back to Explorer's rule.
    'settings open', 'sleep 300', 'settings set drag_default explorer', 'settings ok', 'sleep 300',
    "dragover row 1 0 600 $files",
    'sleep 300', 'quit'
)
$script = Join-Path $root 'script.txt'
[System.IO.File]::WriteAllText($script, ($steps -join ';'))
$err = Join-Path $root 'stderr.txt'
$env:TWINFM_SCRIPT = "@$script"
$env:SLINT_BACKEND = 'winit-software'
$env:TWINFM_CUE_MOCK = "$Mock"
if ($Scheme) { $env:TWINFM_SCHEME = $Scheme }
Remove-Item Env:TWINFM_CONFIG -ErrorAction SilentlyContinue
try {
    $proc = Start-Process -FilePath $Exe -PassThru -RedirectStandardError $err
    $null = $proc.Handle # kept, or ExitCode reads blank after the exit
} finally {
    Remove-Item Env:TWINFM_SCRIPT, Env:SLINT_BACKEND, Env:TWINFM_CUE_MOCK, Env:TWINFM_SCHEME -ErrorAction SilentlyContinue
}
$tag = "m$Mock" + $(if ($Scheme) { "-$Scheme" } else { '' })
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
Get-Content $err | Where-Object { $_ -match '^\[dnd\] (cue|drop menu|Drop at|over |drop into)|^\[settings\] dialog|^\[fileops\] done|^\[auto\] (dragover|dropfiles)|panicked' }
"exit $($proc.ExitCode)"
