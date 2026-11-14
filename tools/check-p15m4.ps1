# P15-M4's scripted check of the search patterns: two runs of target\release\twinfm.exe against a
# scratch settings file and tree in %TEMP%\twinfm-p15m4-check\ (never Jeremy's). Run 1: the live
# filter with a substring that has brackets, a glob whose bracket has no partner, classes, an
# expression, a half-typed one (the listing kept, the note), the expression finished, the `\re:`
# escape; name searches with a class and with an expression; Return on a half-typed expression; a
# content search for plain text, for an expression and for a half-typed one; the expression's
# search saved, its tab closed, and the window closed through WM_CLOSE (which writes the settings).
# Run 2 reads the settings back and opens the saved search from the sidebar. Prints the [filter],
# [search], [sidebar] and [auto] rows lines, and the status notes.
param([string]$Exe = (Join-Path $PSScriptRoot '..\target\release\twinfm.exe'))
$ErrorActionPreference = 'Stop'
$root = Join-Path $env:TEMP 'twinfm-p15m4-check'
if (Test-Path $root) { Remove-Item -Recurse -Force $root }
$files = Join-Path $root 'files'
New-Item -ItemType Directory -Force $files, (Join-Path $files 'sub') | Out-Null
$names = 'main.c', 'main.h', 'main.cpp', 'notes [draft].txt', 'notes d.txt', 'report-2019.docx', 'report-2024.docx', 'report-202x.docx',
    'IMG_0412.jpg', 'IMG_0413.JPEG', 'IMG_041.jpg', 'a re:port.txt', 'half[open.txt'
foreach ($n in $names) { [System.IO.File]::WriteAllText((Join-Path $files $n), "file $n`r`n") }
[System.IO.File]::WriteAllText((Join-Path $files 'sub\todo.txt'), "first line`r`n  TODO: write the docs`r`nlast`r`n")
[System.IO.File]::WriteAllText((Join-Path $files 'sub\fixme.c'), "int x; // fixme later`r`n// nothing`r`n")
[System.IO.File]::WriteAllText((Join-Path $files 'sub\IMG_0500.jpg'), "not a picture: todo is mid-line here`r`n")
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
    Get-Content $err | Where-Object { $_ -match '^\[filter\]|^\[search\] pane=\d (gen=\d+ (start|content|done)|".*not a pattern|box Return|content ".*refused|saved search)|^\[sidebar\] .*saved search|^\[auto\] rows|^\[fileops\] pane=\d note: Not a pattern|panicked' }
}

$f = { param($text) "filter 0 $text", 'sleep 500', 'rows 0 20' }
Run 'run1' (@('sleep 1500', "nav 0 $files", 'sleep 600', 'split', 'sleep 400') +
    (& $f 'notes [draft].txt') + (& $f '[draft]') + (& $f 'half[*') + (& $f '*[draft]*') +
    (& $f '*.[ch]') + (& $f 'report-202[0-9]*') + (& $f '[!a-m]*') +
    (& $f 're:^IMG_\d{4}\.jpe?g$') + (& $f 're:^IMG_\d{4}\.(jpe?g') + (& $f 're:^IMG_\d{4}\.(jpe?g|png)$') +
    (& $f 're:') + (& $f '\re:') + (& $f 're:(?-i)img') + @('filter 0', 'sleep 400') +
    @('filter 0 *.[ch]', 'sleep 300', 'searchreturn 0', 'waitsearch 5000', 'sleep 500', 'rows 0 20', 'tab close', 'sleep 300',
      'filter 0 re:^img_\d+\.jpe?g$', 'sleep 300', 'searchreturn 0', 'waitsearch 5000', 'sleep 500', 'rows 0 20', 'tab close', 'sleep 300',
      'filter 0 re:img_(', 'sleep 300', 'searchreturn 0', 'sleep 500', 'search 0', 'sleep 500', 'filter 0', 'sleep 300',
      'csearch 0 todo', 'waitsearch 5000', 'sleep 500', 'rows 0 20', 'tab close', 'sleep 300',
      'csearch 0 re:^\s*(TODO|FIXME)', 'waitsearch 5000', 'sleep 500', 'rows 0 20', 'tab close', 'sleep 300',
      'csearch 0 re:TODO|FIXME', 'waitsearch 5000', 'sleep 500', 'rows 0 20', 'searchsave 0', 'sleep 300', 'tab close', 'sleep 300',
      'csearch 0 re:TODO|(', 'sleep 500', 'contentmode 0 off', 'sleep 300', 'filter 0', 'sleep 300',
      'window close'))
'--- the saved search in the settings ---'
Get-Content $config | Select-String -Pattern '^\[\[search\]\]' -Context 0, 4 | ForEach-Object { $_.Line; $_.Context.PostContext }
Run 'run2' @('sleep 1500', 'searchopen 0', 'waitsearch 5000', 'sleep 500', 'rows 0 20', 'window close')
