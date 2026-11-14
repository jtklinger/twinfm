# The scripted check of the blank-part click (Phase 15's console-check fix): one run of
# target\release\twinfm.exe on a scratch folder of twelve files in %TEMP%\twinfm-p15-blank-click\ (no
# settings file, no clipboard, no Recycle Bin). A click on row 1's name selects it; a plain click on
# row 2's blank part (400 px into the list, past the name's text) clears the selection; Ctrl there
# toggles row 2 beside row 1; Shift on row 4's blank part selects the range; a drag from row 2's blank
# part to row 4 still bands. `band <pane> x y x y` with both points the same is a press and a release
# in place. Prints the [band], [select] and [auto] scrollpos lines.
param([string]$Exe = (Join-Path $PSScriptRoot '..\target\release\twinfm.exe'))
$ErrorActionPreference = 'Stop'
$root = Join-Path $env:TEMP 'twinfm-p15-blank-click'
if (Test-Path $root) { Remove-Item -Recurse -Force $root }
$files = Join-Path $root 'files'
New-Item -ItemType Directory -Force $files | Out-Null
0..11 | ForEach-Object { [System.IO.File]::WriteAllText((Join-Path $files ('f{0:d2}.txt' -f $_)), "file $_`r`n") }
$steps = @('sleep 1500', "nav 0 $files", 'sleep 600',
    'click 0 1', 'sleep 300', 'band 0 400 55 400 55', 'sleep 300', 'scrollpos 0',
    'click 0 1', 'sleep 300', 'band 0 400 55 400 55 ctrl', 'sleep 300', 'scrollpos 0',
    'band 0 400 99 400 99 shift', 'sleep 300', 'scrollpos 0',
    'band 0 400 55 400 99', 'sleep 300', 'scrollpos 0',
    'band 0 400 55 400 55', 'sleep 300', 'band 0 400 55 400 55', 'sleep 300', 'quit')
$script = Join-Path $root 'script.txt'
[System.IO.File]::WriteAllText($script, ($steps -join ';'))
$err = Join-Path $root 'stderr.txt'
$env:TWINFM_SCRIPT = "@$script"
try {
    $p = Start-Process -FilePath $Exe -PassThru -RedirectStandardError $err -WindowStyle Normal
    $null = $p.Handle
    if (-not $p.WaitForExit(120000)) { $p.Kill(); 'timed out' }
    "exit $($p.ExitCode)"
} finally {
    Remove-Item Env:TWINFM_SCRIPT -ErrorAction SilentlyContinue
}
Get-Content $err | Where-Object { $_ -match '^\[band\]|^\[select\]|^\[auto\] (scrollpos|band)|panicked' }
