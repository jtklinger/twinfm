# P14-M1's scripted check of saved searches: two runs of target\release\twinfm.exe against a scratch
# settings file in %TEMP%\twinfm-p14m1-check\ (never Jeremy's). Run 1 saves a name search (Ctrl+D's
# command, then Ctrl+D again: saved already), a content search (More's "Save this search") and a
# search under a folder that is then deleted, renames, moves and opens them, and drops a folder into the
# space below the sidebar's list while the Searches group shows (it must append to the bookmarks). The
# window is closed through WM_CLOSE, which writes the settings. Run 2 reads them back, opens the search
# whose root is gone, and removes a row. Prints the [sidebar] and [search] lines and the settings file.
param([string]$Exe = (Join-Path $PSScriptRoot '..\target\release\twinfm.exe'))
$ErrorActionPreference = 'Stop'
$root = Join-Path $env:TEMP 'twinfm-p14m1-check'
if (Test-Path $root) { Remove-Item -Recurse -Force $root }
$proj = Join-Path $root 'proj'
$gone = Join-Path $root 'gone'
$drop = Join-Path $root 'dropped'
New-Item -ItemType Directory -Force $proj, (Join-Path $proj 'sub'), $gone, $drop | Out-Null
Set-Content -Path (Join-Path $proj 'main.rs') -Value 'fn main() {}'
Set-Content -Path (Join-Path $proj 'sub\lib.rs') -Value '// the needle is here'
Set-Content -Path (Join-Path $proj 'notes.txt') -Value 'no match'
Set-Content -Path (Join-Path $gone 'x.txt') -Value 'x'
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
    Get-Content $err | Where-Object { $_ -match '^\[(sidebar|search)\] (?!pane=\d+ gen=\d+ batch)|^\[auto\] (searchopen|toolmenu|MenuPick)|^\[toolbar\] (menu more|pane=\d+ picks)|^\[dnd\] over sidebar|panicked' }
}

Run 'run1' @(
    'sleep 1500', "nav 0 $proj", 'sleep 500', 'bookmark add', 'sleep 300',
    'filter 0 *.rs', 'sleep 300', 'searchreturn 0', 'waitsearch 5000',
    'searchsave 0', 'sleep 300', 'key CtrlD', 'sleep 300', 'tab close', 'sleep 300', 'filter 0', 'sleep 300',
    'csearch 0 needle', 'waitsearch 5000',
    'toolbar 0 more', 'waitmenu 2000', 'toolmenu Save this search', 'sleep 300', 'tab close', 'sleep 300',
    'contentmode 0 off', 'filter 0', 'sleep 300',
    "nav 0 $gone", 'sleep 500', 'filter 0 x', 'sleep 300', 'searchreturn 0', 'waitsearch 5000', 'searchsave 0', 'sleep 300',
    'tab close', 'sleep 300', 'filter 0', 'sleep 300',
    'searchrename 0 Rust files', 'sleep 200', 'searchmove 1 up', 'sleep 200',
    'searchopen 0', 'waitsearch 5000', 'sleep 500', 'tab close', 'sleep 300',
    'searchrename 1 ', 'sleep 200',
    'window max', 'sleep 1000', "dropfiles point 90 1200 auto $drop", 'sleep 800',
    'window close'
)
Remove-Item -Recurse -Force $gone
'--- settings after run 1 ---'
Get-Content $config | Where-Object { $_ -match '^\[\[(search|bookmark)\]\]|^(path|pattern|content|title) = ' }
Run 'run2' @(
    'sleep 1500', 'searchopen 2', 'sleep 800', 'searchremove 2', 'sleep 300', 'searchopen 0', 'waitsearch 5000', 'sleep 300',
    'window close'
)
'--- settings after run 2 ---'
Get-Content $config | Where-Object { $_ -match '^\[\[(search|bookmark)\]\]|^(path|pattern|content|title) = ' }
