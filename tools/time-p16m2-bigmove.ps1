# P16-M2's big move, timed (Jeremy's console check 7): 40,000 files moved between two folders and the
# move undone, in one scripted run of the given exe, with stderr redirected to a file as the Log
# setting does. The files are a copy of C:\traces\bigdir (only read) in %TEMP%\twinfm-p16m2-big\src,
# made on the first run and reused (a run leaves them where it found them: the undo moves them back);
# dst beside it starts empty. No TWINFM_SOAK: these are real moves between two %TEMP% folders.
#
# The log has no clock, so this script reads stderr as it grows (every 50 ms) and notes when each
# marker line first shows; a line is seen once its write is done, so a marker's time includes the
# time TwinFM took to write it. It prints:
#   - the marker times, in seconds from the first, and the gaps between them;
#   - every [perf] period with over 500 ms of UI-thread CPU (the freezes);
#   - the worker's own timing lines (items handed to the shell, the shell's first callback) and the
#     drag's start, where the exe prints them;
#   - the size of the log and of its longest line, and the exit code.
# Steps: both panes on src and dst, Ctrl+A in src, a drag of the selection started and cancelled (the
# drag's start), Shift+F6 (the move), the wait, Ctrl+Z, the wait.
# Run with the machine quiet; about five minutes a run.
param(
    [string]$Exe = (Join-Path $PSScriptRoot '..\target\release\twinfm.exe'),
    [string]$Fixture = 'C:\traces\bigdir',
    [string]$Tag = 'run'
)
$ErrorActionPreference = 'Stop'
$root = Join-Path $env:TEMP 'twinfm-p16m2-big'
$src = Join-Path $root 'src'
$dst = Join-Path $root 'dst'
New-Item -ItemType Directory -Force $src, $dst | Out-Null
$have = @([System.IO.Directory]::EnumerateFiles($src)).Count
if ($have -ne 40000) {
    "copying the fixture into $src ($have there)"
    robocopy $Fixture $src /E /NFL /NDL /NJH /NJS /NP /MT:8 | Out-Null
    $have = @([System.IO.Directory]::EnumerateFiles($src)).Count
}
if ($have -ne 40000) { throw "src holds $have files, not 40000" }
if (@([System.IO.Directory]::EnumerateFileSystemEntries($dst)).Count -ne 0) { throw "dst is not empty: $dst" }

$steps = @(
    'sleep 2500', "nav 0 $src", 'sleep 2500', 'split', 'sleep 1000', "nav 1 $dst", 'sleep 1000',
    'click 0 0', 'sleep 300', 'key CtrlA', 'sleep 800',
    'dragrow 0 0 40 40', 'sleep 1500', 'dragcancel', 'sleep 2000',
    'click 0 0', 'sleep 300', 'key CtrlA', 'sleep 800',
    'move', 'waitops 900000', 'sleep 5000',
    'undo', 'waitops 900000', 'sleep 5000',
    'quit'
)
$script = Join-Path $root "script-$Tag.txt"
[System.IO.File]::WriteAllText($script, ($steps -join ';'))
$err = Join-Path $root "stderr-$Tag.txt"
if (Test-Path $err) { Remove-Item -Force $err }
$env:TWINFM_SCRIPT = "@$script"
Remove-Item Env:TWINFM_CONFIG -ErrorAction SilentlyContinue
try {
    $proc = Start-Process -FilePath $Exe -PassThru -RedirectStandardError $err
    $null = $proc.Handle # kept, or ExitCode reads blank after the exit
} finally {
    Remove-Item Env:TWINFM_SCRIPT -ErrorAction SilentlyContinue
}

# The marker lines, each noted the first time it shows.
$markers = [ordered]@{
    'drag: dragging'        = '^\[dnd\] pane=0 row=0: dragging 40000'
    'drag: DoDragDrop'      = '^\[dnd\] DoDragDrop starts'
    'move: submit'          = '^\[fileops\] submit id=1 '
    'move: note'            = '^\[fileops\] pane=0 note: moving 40000'
    'move: progress 0%'     = '^\[fileops\] progress id=1 0%'
    'move: progress 100%'   = '^\[fileops\] progress id=1 100%'
    'move: done'            = '^\[fileops\] done id=1 '
    'undo: pressed'         = '^\[undo\] "Moved 40000 items" with '
    'undo: submit'          = '^\[fileops\] submit id=2 '
    'undo: note'            = '^\[fileops\] pane=0 note: moving 40000.*'
    'undo: progress 0%'     = '^\[fileops\] progress id=2 0%'
    'undo: progress 100%'   = '^\[fileops\] progress id=2 100%'
    'undo: done'            = '^\[fileops\] done id=2 '
}
$seen = [ordered]@{}
$watch = [System.Diagnostics.Stopwatch]::StartNew()
$fs = $null
$pos = 0L
$tail = ''
$longest = 0
$notes = 0
function Read-More {
    if (-not $script:fs) {
        if (-not (Test-Path $err)) { return }
        $script:fs = [System.IO.File]::Open($err, 'Open', 'Read', 'ReadWrite,Delete')
    }
    $len = $script:fs.Length
    if ($len -le $script:pos) { return }
    $buf = New-Object byte[] ($len - $script:pos)
    $null = $script:fs.Seek($script:pos, 'Begin')
    $n = $script:fs.Read($buf, 0, $buf.Length)
    $script:pos += $n
    $now = $watch.Elapsed.TotalSeconds
    $text = $script:tail + [System.Text.Encoding]::UTF8.GetString($buf, 0, $n)
    $cut = $text.LastIndexOf("`n")
    if ($cut -lt 0) { $script:tail = $text; return }
    $script:tail = $text.Substring($cut + 1)
    foreach ($line in $text.Substring(0, $cut).Split("`n")) {
        if ($line.Length -gt $script:longest) { $script:longest = $line.Length }
        if ($line.Length -gt 400) { $line = $line.Substring(0, 400) }
        # The undo job's running note is the second "moving 40000" note.
        if ($line -match '^\[fileops\] pane=0 note: moving 40000') {
            $script:notes++
            $key = if ($script:notes -eq 1) { 'move: note' } else { 'undo: note' }
            if (-not $seen.Contains($key)) { $seen[$key] = $now }
            continue
        }
        foreach ($k in $markers.Keys) {
            if ($k -like '*: note') { continue }
            if (-not $seen.Contains($k) -and $line -match $markers[$k]) { $seen[$k] = $now }
        }
    }
}
while (-not $proc.WaitForExit(50)) {
    Read-More
    if ($watch.Elapsed.TotalMinutes -gt 30) { $proc.Kill(); 'timed out after 30 minutes'; break }
}
$proc.WaitForExit()
Read-More
if ($fs) { $fs.Dispose() }

"== $Tag : $Exe"
$first = $null
$prev = $null
foreach ($k in $markers.Keys) {
    if (-not $seen.Contains($k)) { '{0,-22} not seen' -f $k; continue }
    $t = $seen[$k]
    if ($null -eq $first) { $first = $t }
    $gap = if ($null -eq $prev) { 0 } else { $t - $prev }
    '{0,-22} {1,8:n1} s  (+{2:n1} s)' -f $k, ($t - $first), $gap
    $prev = $t
}
'-- [perf] periods with over 500 ms of UI-thread CPU:'
$log = Get-Content $err | ForEach-Object { if ($_.Length -gt 400) { $_.Substring(0, 400) } else { $_ } }
$n = 0
foreach ($line in $log) {
    $n++
    if ($line -match '^\[perf\] .*render\[n=(\d+) .*ui_cpu=(\d+)ms') {
        if ([int]$Matches[2] -gt 500) { "line $n : ui_cpu=$($Matches[2]) ms, $($Matches[1]) frames" }
    }
}
'-- the exe''s own timing lines:'
$log | Where-Object { $_ -match '^\[fileops\] id=\d+: |^\[dnd\] .*(items parsed|DoDragDrop starts)|^\[watch\] .* applied: \+0 -\d+ .*apply \d{3,}|^\[undo\] .* refused|panicked' } | Select-Object -First 40
'-- item lines: {0}; [undo] and submit lines: {1}' -f @($log | Where-Object { $_ -match '^\[fileops\]\s+item ' }).Count, @($log | Where-Object { $_ -match '^\[undo\] |^\[fileops\] submit ' }).Count
'-- log: {0:n1} MB, {1} lines, longest line {2} characters' -f ((Get-Item $err).Length / 1MB), $log.Count, $longest
'-- src holds {0} files, dst {1}' -f @([System.IO.Directory]::EnumerateFiles($src)).Count, @([System.IO.Directory]::EnumerateFileSystemEntries($dst)).Count
"exit $($proc.ExitCode)"
