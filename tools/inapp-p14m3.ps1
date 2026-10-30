# P14-M3's in-app pair: the hover's cost with the pointer over a list while it scrolls. The gate's
# scenario on C:\traces\bigdir with its thumb drags replaced by more wheel turns, so the pointer stays
# over the lists the whole 10 s busy window (the gate's drags park it on a scrollbar, where the hover is
# off): `hover` parks it over the left list after the split, and each `wheel` step moves it to its
# pane's middle. Runs the reference and the candidate in pairs, the order swapped between pairs (the
# second slot reads high, P4-M2 and ab53), unelevated, no settings file. Prints each run's in-app busy,
# frame interval p50 (16.4 ms at 60 Hz; an RDP 32 Hz run is not comparable: rerun it) and render p50.
# Close every TwinFM window first; only one measurement at a time on this machine.
param(
    [string]$Reference = 'C:\Users\jtkli\projects\twinfm\target-p14m1\release\twinfm.exe',
    [string]$Candidate = (Join-Path $PSScriptRoot '..\target\release\twinfm.exe'),
    [int]$Pairs = 3,
    [string]$Dir = 'C:\traces\bigdir'
)
$ErrorActionPreference = 'Stop'
if (Get-Process -Name twinfm -ErrorAction SilentlyContinue) { throw 'A TwinFM window is open: close every one first.' }
foreach ($e in @($Reference, $Candidate)) { if (-not (Test-Path $e)) { throw "$e is not built." } }
$out = Join-Path $env:TEMP 'twinfm-p14m3-inapp'
New-Item -ItemType Directory -Force $out | Out-Null
Remove-Item Env:TWINFM_CONFIG -ErrorAction SilentlyContinue
$scenario = 'sleep 2000; split; sleep 500; hover 400 400; sleep 100; wheel 1 -120 16; wheel 0 -120 16; wheel 1 120 16; wheel 0 120 16; ' +
    'sleep 2500; sort 0 1; sleep 1000; sort 1 2; sleep 3000; quit'

function Measure-Run([string]$label, [string]$exe, [int]$n) {
    $err = Join-Path $out "$label-$n.stderr.txt"
    $env:TWINFM_SCRIPT = $scenario
    try {
        $p = Start-Process -FilePath $exe -ArgumentList $Dir -PassThru -RedirectStandardError $err
        if (-not $p.WaitForExit(120000)) { $p.Kill(); throw "$label-$n timed out" }
    } finally { Remove-Item Env:TWINFM_SCRIPT -ErrorAction SilentlyContinue }
    $log = Get-Content $err
    $busy = [regex]::Match(($log | Select-String -SimpleMatch '[busy] 10 s after split' | Select-Object -First 1).Line, '([0-9.]+)% busy').Groups[1].Value
    $scroll = @($log | Select-String -Pattern 'render\[n=(\d+) p50=([0-9.]+)ms.*interval\[n=\d+ p50=([0-9.]+)ms' | ForEach-Object { $_.Matches[0] } |
        Where-Object { [int]$_.Groups[1].Value -ge 30 })
    $med = { param($v) $x = @($v | Sort-Object); if ($x.Count) { '{0:N2}' -f $x[[int][math]::Floor(($x.Count - 1) / 2)] } else { '?' } }
    [pscustomobject]@{
        Run = "$label-$n"
        Busy = $busy
        IntervalP50 = & $med ($scroll | ForEach-Object { [double]$_.Groups[3].Value })
        RenderP50 = & $med ($scroll | ForEach-Object { [double]$_.Groups[2].Value })
        Panics = @($log | Select-String -SimpleMatch 'panicked').Count
    }
}

$rows = @()
for ($k = 1; $k -le $Pairs; $k++) {
    $order = if ($k % 2) { @('ref', 'cand') } else { @('cand', 'ref') }
    foreach ($b in $order) {
        $rows += Measure-Run $b $(if ($b -eq 'ref') { $Reference } else { $Candidate }) $k
        Start-Sleep -Seconds 2
    }
}
$rows | Format-Table -AutoSize
foreach ($b in 'ref', 'cand') {
    $v = @($rows | Where-Object { $_.Run -like "$b-*" } | ForEach-Object { [double]$_.Busy } | Sort-Object)
    "${b}: busy median $($v[[int][math]::Floor(($v.Count - 1) / 2)])% over $($v.Count) runs ($($v -join ', '))"
}
"logs: $out"
