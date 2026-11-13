# P15-M2's scripted check of the Size on disk column's grip, width and sort: three runs of
# target\release\twinfm.exe against a scratch settings file in %TEMP%\twinfm-p15m2-check\ (never
# Jeremy's). The panes are set up unequal (two tabs on the left, one on the right, the split line at
# 0.85, then 0.5, then 0.15). Run 1 turns the column on, drags, fits and sets its grip in the left pane
# (to its floor and Modified's), sorts by it both ways with a watch edit under the sort, narrows the
# pane until the column makes way (its grip refused, the sort kept), does the same in the right pane,
# resets the widths through the dialog, and closes through WM_CLOSE, which writes the settings. Run 2
# reads them back (the saved width, the saved sort) and turns the column off while sorted by it. Run 3
# starts from a file that names the sort with the column off. Prints the [cols], [sort], [settings] and
# [auto] lines and the settings' column and sort keys.
param([string]$Exe = (Join-Path $PSScriptRoot '..\target\release\twinfm.exe'))
$ErrorActionPreference = 'Stop'
$root = Join-Path $env:TEMP 'twinfm-p15m2-check'
if (Test-Path $root) { Remove-Item -Recurse -Force $root }
$files = Join-Path $root 'files'
$other = Join-Path $root 'other'
New-Item -ItemType Directory -Force $files, (Join-Path $files 'zdir'), (Join-Path $files 'adir'), $other | Out-Null
# Sizes on disk that do not follow the sizes: a 2 MB sparse file holding nothing, two plain files, a
# tiny one.
$sparse = Join-Path $files 'sparse-big.bin'
[System.IO.File]::WriteAllBytes($sparse, [byte[]]@())
fsutil sparse setflag $sparse | Out-Null
$fs = [System.IO.File]::Open($sparse, 'Open', 'Write'); $fs.SetLength(2MB); $fs.Close()
$rng = New-Object System.Random 15
foreach ($f in @(@('plain-mid.bin', 300000), @('plain-small.bin', 50000))) {
    $b = New-Object byte[] $f[1]; $rng.NextBytes($b); [System.IO.File]::WriteAllBytes((Join-Path $files $f[0]), $b)
}
Set-Content -Path (Join-Path $files 'tiny.txt') -Value 'tiny'
Set-Content -Path (Join-Path $other 'o.txt') -Value 'other'
$config = Join-Path $root 'config.toml'

function Run([string]$name, [string[]]$steps) {
    $script = Join-Path $root "$name-script.txt"
    [System.IO.File]::WriteAllText($script, ($steps -join ';'))
    $err = Join-Path $root "$name-stderr.txt"
    $env:TWINFM_SCRIPT = "@$script"
    $env:TWINFM_CONFIG = $config
    try {
        $p = Start-Process -FilePath $Exe -PassThru -RedirectStandardError $err -WindowStyle Normal
        if (-not $p.WaitForExit(120000)) { $p.Kill(); "$name timed out" }
        "$name exit $($p.ExitCode)"
    } finally {
        Remove-Item Env:TWINFM_SCRIPT, Env:TWINFM_CONFIG -ErrorAction SilentlyContinue
    }
    Get-Content $err | Where-Object { $_ -match '^\[cols\]|^\[sort\] pane=\d \d+ of \d+ entries by (OnDisk|Name)|^\[auto\] (rows|col|coldrag|colfit|splitter)|^\[settings\] (pane=|the Size on disk|Size on disk column|dialog: (size_on_disk|cols_reset))|^\[watch\] .*applied|panicked' }
}
function Keys { Get-Content $config | Where-Object { $_ -match '^(col_\w+|size_on_disk|sort_column|sort) = ' } }

Run 'run1' @(
    'sleep 1500', "nav 0 $files", 'sleep 500', 'split', 'sleep 500', "nav 1 $files", 'sleep 500',
    'click 0 0', 'tab new', 'sleep 500',
    'settings open', 'settings set size_on_disk true', 'settings ok', 'sleep 1000',
    'colfit 0 ondisk', 'sleep 200',
    'splitter 0.85', 'sleep 600', 'rows 0 6',
    'coldrag 0 ondisk -20', 'sleep 200', 'coldrag 0 ondisk 500', 'sleep 200', 'coldrag 0 ondisk -500', 'sleep 200',
    'colfit 0 ondisk', 'sleep 200', 'colwidth 0 ondisk 5', 'sleep 200', 'colwidth 0 ondisk 130', 'sleep 200',
    'coldrag 0 modified 10', 'sleep 200', 'colwidth 0 modified 140', 'sleep 200',
    'sort 0 4', 'sleep 500', 'rows 0 6', 'sort 0 4', 'sleep 500', 'rows 0 6',
    'touch 0 watched.txt', 'waitedit 3000', 'sleep 300', 'rows 0 7', 'rmfile 0 watched.txt', 'waitedit 3000', 'sleep 300',
    'splitter 0.5', 'sleep 600', 'colfit 0 ondisk', 'coldrag 0 ondisk 10', 'rows 0 6',
    'splitter 0.15', 'sleep 600', 'coldrag 1 ondisk -20', 'sleep 200', 'colfit 1 ondisk', 'sleep 200',
    'sort 1 4', 'sleep 500', 'rows 1 6', 'sort 1 0', 'sleep 500',
    'splitter 0.85', 'sleep 600',
    'settings open', 'settings set cols_reset', 'settings ok', 'sleep 400',
    'colwidth 0 ondisk 130', 'sleep 200', 'sort 0 4', 'sleep 500', 'rows 0 6',
    'window close'
)
'--- settings after run 1 ---'
Keys
$saved = Get-Content $config -Raw
Run 'run2' @(
    'sleep 1500', 'rows 0 6',
    'settings open', 'settings set size_on_disk false', 'settings ok', 'sleep 1000', 'rows 0 6',
    'colwidth 0 ondisk 90', 'sleep 200',
    'window close'
)
'--- settings after run 2 ---'
Keys
# Run 3: run 1's file with the column off, its tabs and its last sort still naming the column.
[System.IO.File]::WriteAllText($config, ($saved -replace 'size_on_disk = true', 'size_on_disk = false'))
'--- settings before run 3 ---'
Keys
Run 'run3' @('sleep 1500', 'rows 0 6', 'window close')
'--- settings after run 3 ---'
Keys
