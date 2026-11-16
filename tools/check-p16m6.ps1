# P16-M6's scripted check of a search tab that follows changes: one run of target\release\twinfm.exe
# in a scratch tree, %TEMP%\twinfm-p16m6-check\tree (a.txt, b.txt, c.log, sub\s.txt, sub\deep\d.txt,
# hid\h.txt in a hidden folder, hold.txt and sub\also.txt holding the word "needle"). No settings file.
# Part 1, a name search for *.txt left open (its tab in front): files made, renamed into and out of the
# pattern in the root (`touch`, `mvfile`, each window waited for with `waitwatch`) and made and
# deleted below it (this script, at the `[auto] Sleep(30NN)` marks); a file with another extension
# (nothing changes); a file in the hidden folder (not shown); the folder sub renamed (its rows
# follow); the tab sent to the background and brought back.
# Part 2, a content search for "needle" left open: this script writes files while TwinFM sleeps (it
# watches stderr for the `[auto] Sleep(30NN)` marks): a new file with the word (a row), the word taken
# out of it (the row goes), the word written into an old file (a row), a file with the word deleted.
# Part 3, a burst: -Burst small text files written at once into tree\burst (the name search open),
# which either lands as windows or, past the cap, ends the following with the status note.
# Prints the [search] watching, window and not-following lines, the rows after each step and the exit code.
param(
    [string]$Exe = (Join-Path $PSScriptRoot '..\target\release\twinfm.exe'),
    [int]$Burst = 3000
)
$ErrorActionPreference = 'Stop'
$root = Join-Path $env:TEMP 'twinfm-p16m6-check'
if (Test-Path $root) { Remove-Item -Recurse -Force $root }
$tree = Join-Path $root 'tree'
New-Item -ItemType Directory -Force (Join-Path $tree 'sub\deep'), (Join-Path $tree 'hid'), (Join-Path $tree 'burst') | Out-Null
$w = { param($rel, $text) [System.IO.File]::WriteAllText((Join-Path $tree $rel), $text) }
& $w 'a.txt' "alpha`r`n"; & $w 'b.txt' "beta`r`n"; & $w 'c.log' "a log`r`n"
& $w 'sub\s.txt' "in sub`r`n"; & $w 'sub\deep\d.txt' "deep`r`n"; & $w 'hid\h.txt' "hidden`r`n"
& $w 'hold.txt' "this file holds the needle`r`n"; & $w 'sub\also.txt' "first line`r`nanother Needle here`r`n"
(Get-Item (Join-Path $tree 'hid')).Attributes = 'Directory, Hidden'

$ww = 'waitwatch 4000'
$steps = @(
    'sleep 2500', "nav 0 $tree", 'sleep 800',
    # Part 1: the name search.
    'filter 0 *.txt', 'sleep 300', 'searchreturn 0', 'waitsearch 5000', 'sleep 800', 'rows 0 30',
    'touch 0 new.txt', $ww, 'rows 0 30',
    'touch 0 other.log', 'sleep 2500',
    # Below the root the script writes at the marks (`touch` takes a plain name): a file made deep
    # in the tree, one in the hidden folder (no row), the deep one deleted.
    'sleep 3011', 'rows 0 30', 'sleep 3012', 'rows 0 30',
    'mvfile 0 c.log c.txt', $ww, 'rows 0 30',
    'mvfile 0 new.txt new.log', $ww, 'rows 0 30',
    'sleep 3013', 'rows 0 30',
    'mvfile 0 sub moved', $ww, 'sleep 500', 'rows 0 30',
    # The tab in the background while a file is made, then shown again.
    'tab 1', 'sleep 500', 'touch 0 bg.txt', 'sleep 2500', 'tab 2', 'sleep 800', 'rows 0 30', 'tab close', 'sleep 300',
    'filter 0', 'sleep 300',
    # Part 2: the content search; the script writes at the marks.
    'csearch 0 needle', 'waitsearch 5000', 'sleep 800', 'rows 0 30',
    'sleep 3001', 'rows 0 30', 'sleep 3002', 'rows 0 30', 'sleep 3003', 'rows 0 30', 'sleep 3004', 'rows 0 30',
    'tab close', 'sleep 300', 'contentmode 0 off', 'filter 0', 'sleep 300',
    # Part 3: the burst under the name search.
    'filter 0 *.txt', 'sleep 300', 'searchreturn 0', 'waitsearch 5000', 'sleep 800',
    'sleep 8005', 'rows 0 5', 'tab close', 'sleep 300',
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
$acts = [ordered]@{
    '[auto] Sleep(3011)' = { [System.IO.File]::WriteAllText((Join-Path $tree 'sub\deep\made.txt'), "made`r`n") }
    '[auto] Sleep(3012)' = { [System.IO.File]::WriteAllText((Join-Path $tree 'hid\made.txt'), "made`r`n") }
    '[auto] Sleep(3013)' = { Remove-Item (Join-Path $tree 'sub\deep\made.txt') }
    '[auto] Sleep(3001)' = { [System.IO.File]::WriteAllText((Join-Path $tree 'moved\fresh.txt'), "a fresh needle`r`n") }
    '[auto] Sleep(3002)' = { [System.IO.File]::WriteAllText((Join-Path $tree 'moved\fresh.txt'), "nothing to find now`r`n") }
    '[auto] Sleep(3003)' = { [System.IO.File]::WriteAllText((Join-Path $tree 'a.txt'), "alpha`r`nnow with a NEEDLE`r`n") }
    '[auto] Sleep(3004)' = { Remove-Item (Join-Path $tree 'hold.txt') }
    '[auto] Sleep(8005)' = {
        $sw = [System.Diagnostics.Stopwatch]::StartNew()
        for ($i = 0; $i -lt $Burst; $i++) { [System.IO.File]::WriteAllText((Join-Path $tree ('burst\f{0:d5}.txt' -f $i)), "burst $i`r`n") }
        "burst: $Burst files written in $($sw.ElapsedMilliseconds) ms"
    }
}
$done = @{}
$deadline = (Get-Date).AddSeconds(180)
while (-not $proc.HasExited -and (Get-Date) -lt $deadline) {
    Start-Sleep -Milliseconds 100
    $text = if (Test-Path $err) { Get-Content $err -Raw -ErrorAction SilentlyContinue } else { '' }
    foreach ($mark in $acts.Keys) {
        if (-not $done[$mark] -and $text -and $text.Contains($mark)) {
            $done[$mark] = $true
            & $acts[$mark]
        }
    }
}
if (-not $proc.HasExited) { $proc.Kill(); 'timed out' }
$proc.WaitForExit()
Get-Content $err | Where-Object { $_ -match '^\[search\] pane=\d+ (gen=\d+ (watching|start|content|done)|window|not following|\d+ records)|^\[auto\] (rows|touch|rmfile|mvfile)|^\[watch\] pane=\d+ (no watch|tab=\d+ (\d+ records|watch lost))|panicked' } |
    ForEach-Object { $_.Replace($tree, '<tree>') }
"exit $($proc.ExitCode)"
