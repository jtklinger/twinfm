# P16-M2's scripted check of the mouse's Back and Forward buttons over a tab, the scrollbars and the
# text boxes: one run of target\release\twinfm.exe in a scratch tree, %TEMP%\twinfm-p16m2-check\work
# (many: 300 files and the folder sub, so its list overflows; long.txt: 600 lines, so the preview's
# list overflows). No settings file; the real renderer (no picture is taken).
# The steps, each a posted side-button press and release through winit's own path (`backclick`,
# `forwardclick`): Back over the left list's scrollbar in many (back to work), Forward over the left
# pane's first tab (many again), Back over the tab (work), Forward over the scrollbar after a move
# back from many\sub (sub again); Back over the search box; Back over the path's text entry (the entry
# closes, the pane goes back) and Forward over it; Back over the rename editor (ignored, as before);
# with two panes, Back over the right pane's tab (that pane becomes the active one); with the preview
# column showing long.txt, Back over its scrollbar (the active pane goes back).
# Prints the [nav] lines and the driver's own, and the exit code.
param(
    [string]$Exe = (Join-Path $PSScriptRoot '..\target\release\twinfm.exe')
)
$ErrorActionPreference = 'Stop'
$root = Join-Path $env:TEMP 'twinfm-p16m2-check'
if (Test-Path $root) { Remove-Item -Recurse -Force $root }
$work = Join-Path $root 'work'
$many = Join-Path $work 'many'
$sub = Join-Path $many 'sub'
New-Item -ItemType Directory -Force $sub | Out-Null
1..300 | ForEach-Object { [System.IO.File]::WriteAllText((Join-Path $many ('f{0:d3}.txt' -f $_)), "file $_`r`n") }
[System.IO.File]::WriteAllText((Join-Path $sub 'in.txt'), "in`r`n")
[System.IO.File]::WriteAllText((Join-Path $work 'long.txt'), ((1..600 | ForEach-Object { "line $_ of the long file" }) -join "`r`n"))

# work lists many, long.txt (rows 0 and 1); many lists sub, then f001.txt on.
$steps = @(
    'sleep 2500', "nav 0 $work", 'sleep 600', "nav 0 $many", 'sleep 800',
    # The list's scrollbar and the tab.
    'backclick 0 scroll', 'sleep 800', 'forwardclick 0 tab', 'sleep 800', 'backclick 0 tab', 'sleep 800',
    "nav 0 $many", 'sleep 600', "nav 0 $sub", 'sleep 600', 'history back', 'sleep 800', 'forwardclick 0 scroll', 'sleep 800',
    # The search box (from sub back to many).
    'backclick 0 search', 'sleep 800',
    # The path's text entry: it closes and the pane moves (many -> sub -> many).
    'pathmode 0 entry', 'sleep 400', 'forwardclick 0 path', 'sleep 800',
    'pathmode 0 entry', 'sleep 400', 'backclick 0 path', 'sleep 800',
    # The rename editor: ignored.
    'click 0 0', 'key F2', 'sleep 400', 'backclick 0 edit', 'sleep 800', 'key Escape', 'sleep 300',
    # Two panes: Back over the right pane's tab makes it the active one (it has nowhere to go back to).
    'split', 'sleep 1000', 'click 0 0', 'sleep 300', 'backclick 1 tab', 'sleep 800', 'click 0 0', 'sleep 300',
    # The preview column on long.txt: Back over its scrollbar moves the active (left) pane.
    "nav 0 $work", 'sleep 600', 'click 0 1', 'preview toggle', 'waitpreview 5000', 'sleep 300',
    'backclick 0 preview', 'sleep 800',
    'quit'
)
$script = Join-Path $root 'script.txt'
[System.IO.File]::WriteAllText($script, ($steps -join ';'))
$err = Join-Path $root 'stderr.txt'
$env:TWINFM_SCRIPT = "@$script"
Remove-Item Env:TWINFM_CONFIG -ErrorAction SilentlyContinue
try {
    $proc = Start-Process -FilePath $Exe -PassThru -RedirectStandardError $err
    $null = $proc.Handle # kept, or ExitCode reads blank after the exit
} finally {
    Remove-Item Env:TWINFM_SCRIPT -ErrorAction SilentlyContinue
}
if (-not $proc.WaitForExit(120000)) { $proc.Kill(); 'timed out' }
$proc.WaitForExit()
Get-Content $err | Where-Object { $_ -match '^\[nav\] pane=\d+ (Back|Forward|history (back|forward))|^\[auto\] (backclick|forwardclick)|^\[rename\]|^\[preview\] pane=\d+ gen=\d+ .*long\.txt|panicked' }
"exit $($proc.ExitCode)"
