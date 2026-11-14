# P15-M4, settled first: what a content search costs with a typed expression against the same text
# searched plainly, on a fixed tree. Builds %TEMP%\twinfm-p15m4-time\tree once (400 text files of
# about 256 KB, 100 MB in all, the needle in every fiftieth file), then one scripted run of
# target\release\twinfm.exe searches it seven times in one search tab: a warm-up (the file cache),
# then plain text and `re:` and the same text, three times each, alternating. Prints each search's
# `done` line and the two medians of its `in T ms`. Run it with the machine quiet (no build, no
# soak, no other TwinFM window): the figures are compared with each other only.
param([string]$Exe = (Join-Path $PSScriptRoot '..\target\release\twinfm.exe'))
$ErrorActionPreference = 'Stop'
$root = Join-Path $env:TEMP 'twinfm-p15m4-time'
$tree = Join-Path $root 'tree'
if (-not (Test-Path $tree)) {
    New-Item -ItemType Directory -Force $tree | Out-Null
    $rng = New-Object System.Random 154
    $words = 'alpha', 'beta', 'gamma', 'delta', 'Epsilon', 'ZETA', 'eta', 'theta', 'iota', 'kappa', 'lambda', 'mu'
    for ($i = 0; $i -lt 400; $i++) {
        $sub = Join-Path $tree ('d{0:00}' -f [int][math]::Floor($i / 20))
        New-Item -ItemType Directory -Force $sub | Out-Null
        $sb = New-Object System.Text.StringBuilder
        $line = 0
        while ($sb.Length -lt 256KB) {
            $n = 6 + $rng.Next(10)
            for ($w = 0; $w -lt $n; $w++) { [void]$sb.Append($words[$rng.Next($words.Count)]).Append(' ') }
            if ($i % 50 -eq 7 -and $line -eq 900) { [void]$sb.Append('the Quick-Needle 2026 sits here') }
            [void]$sb.Append("`r`n")
            $line++
        }
        [System.IO.File]::WriteAllText((Join-Path $sub ('f{0:000}.txt' -f $i)), $sb.ToString())
    }
}
$plain = 'quick-needle 2026'
$steps = @('sleep 1500', "nav 0 $tree", 'sleep 800', "csearch 0 $plain", 'waitsearch 120000')
for ($k = 0; $k -lt 3; $k++) {
    $steps += "csearch 0 $plain", 'waitsearch 120000', "csearch 0 re:$plain", 'waitsearch 120000'
}
$steps += 'quit'
$script = Join-Path $root 'script.txt'
[System.IO.File]::WriteAllText($script, ($steps -join ';'))
$err = Join-Path $root 'stderr.txt'
$env:TWINFM_SCRIPT = "@$script"
try {
    $p = Start-Process -FilePath $Exe -PassThru -RedirectStandardError $err -WindowStyle Normal
    if (-not $p.WaitForExit(900000)) { $p.Kill(); 'timed out' }
    "exit $($p.ExitCode)"
} finally {
    Remove-Item Env:TWINFM_SCRIPT -ErrorAction SilentlyContinue
}
$log = Get-Content $err
$starts = @($log | Select-String -Pattern '^\[search\] pane=0 gen=\d+ content "(.*)" \((\w+)\) in ')
$dones = @($log | Select-String -Pattern '^\[search\] pane=0 gen=\d+ done: (\d+) found in (\d+) files read.* in (\d+) ms')
if ($starts.Count -ne 7 -or $dones.Count -ne 7) { "expected 7 searches, got $($starts.Count) started and $($dones.Count) done" }
$times = @{ text = @(); regex = @() }
for ($k = 0; $k -lt [math]::Min($starts.Count, $dones.Count); $k++) {
    $kind = $starts[$k].Matches[0].Groups[2].Value
    $d = $dones[$k].Matches[0]
    '{0} {1,-5} found {2} in {3} files read, {4} ms' -f $(if ($k) { "run $k" } else { 'warm-up' }), $kind, $d.Groups[1].Value, $d.Groups[2].Value, $d.Groups[3].Value
    if ($k) { $times[$kind] += [int]$d.Groups[3].Value }
}
function Median($v) { $x = @($v | Sort-Object); if ($x.Count) { $x[[int][math]::Floor(($x.Count - 1) / 2)] } }
"plain text median $(Median $times.text) ms ($($times.text -join ', ')); re: median $(Median $times.regex) ms ($($times.regex -join ', '))"
"panicked lines: $(@($log | Select-String -SimpleMatch 'panicked').Count)"
