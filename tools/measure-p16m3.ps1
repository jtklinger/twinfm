# P16-M3's settle-first measurement: what the extra large (256 px) and small (16 px) grids cost
# beside the large one, in-app (no WPR; the console triplet decides a gate).
#
# -Mode scroll (the default): the gate's scenario on C:\traces\bigdir (40,000 files, no thumbnails),
#   one run per view and round with TWINFM_CONFIG set to that view's read-only settings file
#   (C:\traces\grid.toml large, grid-xlarge.toml, grid-small.toml; `-Views details` runs with none), the
#   order reversed in every second round (the second slot reads high, P4-M2 and ab53). Per run: the
#   in-app busy over the 10 s after the split, the frame interval and render p50 of the scroll seconds,
#   the grid rows composed with their average time (summed from the once-a-second [icons] lines), and
#   the private bytes and working set at their peak (sampled every 250 ms from outside).
# -Mode thumbs: memory with thumbnails at 256 px. Makes -Pictures small JPEGs in
#   %TEMP%\twinfm-p16m3-thumbs (once; kept, so Windows' thumbnail cache serves a second run), opens
#   the folder in extra large icons with thumbnails on (a scratch settings file) and pages through all
#   of it and back to the top, sampling memory; prints the [icons] thumbnail memory lines, the last
#   [icons] line's table and the peak and final private bytes.
# Close every TwinFM window first; only one measurement at a time on this machine.
param(
    [ValidateSet('scroll', 'thumbs')][string]$Mode = 'scroll',
    [string]$Exe = (Join-Path $PSScriptRoot '..\target\release\twinfm.exe'),
    [string[]]$Views = @('large', 'xlarge', 'small'),
    [int]$Rounds = 2,
    [string]$Dir = 'C:\traces\bigdir',
    [int]$Pictures = 3600,
    [int]$PageMs = 100
)
$ErrorActionPreference = 'Stop'
if (Get-Process -Name twinfm -ErrorAction SilentlyContinue) { throw 'A TwinFM window is open: close every one first.' }
if (-not (Test-Path $Exe)) { throw "$Exe is not built." }
$out = Join-Path $env:TEMP 'twinfm-p16m3-measure'
New-Item -ItemType Directory -Force $out | Out-Null
$configs = @{ large = 'C:\traces\grid.toml'; xlarge = 'C:\traces\grid-xlarge.toml'; small = 'C:\traces\grid-small.toml'; medium = 'C:\traces\grid-medium.toml'; details = '' }

# One scripted run: starts the exe, samples its memory until it exits, returns the stderr lines and the peaks.
function Invoke-Run([string]$label, [string]$script, [string]$config, [string]$folder, [int]$timeoutS) {
    $err = Join-Path $out "$label.stderr.txt"
    $file = Join-Path $out "$label.script.txt"
    [System.IO.File]::WriteAllText($file, $script)
    $env:TWINFM_SCRIPT = "@$file"
    if ($config) { $env:TWINFM_CONFIG = $config } else { Remove-Item Env:TWINFM_CONFIG -ErrorAction SilentlyContinue }
    try {
        $p = Start-Process -FilePath $Exe -ArgumentList $folder -PassThru -RedirectStandardError $err
        $null = $p.Handle
    } finally { Remove-Item Env:TWINFM_SCRIPT, Env:TWINFM_CONFIG -ErrorAction SilentlyContinue }
    $peakPrivate = 0; $peakWs = 0; $lastPrivate = 0
    $deadline = (Get-Date).AddSeconds($timeoutS)
    while (-not $p.HasExited -and (Get-Date) -lt $deadline) {
        Start-Sleep -Milliseconds 250
        $p.Refresh()
        if (-not $p.HasExited) {
            $lastPrivate = $p.PrivateMemorySize64
            if ($lastPrivate -gt $peakPrivate) { $peakPrivate = $lastPrivate }
            if ($p.WorkingSet64 -gt $peakWs) { $peakWs = $p.WorkingSet64 }
        }
    }
    if (-not $p.HasExited) { $p.Kill(); throw "$label timed out" }
    [pscustomobject]@{ Log = Get-Content $err; PeakPrivateMB = [math]::Round($peakPrivate / 1MB, 1); PeakWsMB = [math]::Round($peakWs / 1MB, 1); LastPrivateMB = [math]::Round($lastPrivate / 1MB, 1); Exit = $p.ExitCode }
}

if ($Mode -eq 'scroll') {
    foreach ($v in $Views) { if ($configs[$v] -and -not (Test-Path $configs[$v])) { throw "$($configs[$v]) is missing (read-only: both panes on that view)." } }
    $scenario = 'sleep 2000; split; sleep 500; wheel 1 -120 16; wheel 0 -120 16; drag 1 400 1000; drag 0 400 1000; sleep 6000; sort 0 1; sleep 1000; sort 1 2; sleep 3000; quit'
    $med = { param($v) $x = @($v | Sort-Object); if ($x.Count) { '{0:N2}' -f $x[[int][math]::Floor(($x.Count - 1) / 2)] } else { '?' } }
    $rows = for ($r = 1; $r -le $Rounds; $r++) {
        $order = if ($r % 2) { $Views } else { $Views[($Views.Count - 1)..0] }
        foreach ($v in $order) {
            $run = Invoke-Run "scroll-$v-$r" $scenario $configs[$v] $Dir 120
            $log = $run.Log
            $busy = [regex]::Match(($log | Select-String -SimpleMatch '[busy] 10 s after split' | Select-Object -First 1).Line, '([0-9.]+)% busy').Groups[1].Value
            $scroll = @($log | Select-String -Pattern 'render\[n=(\d+) p50=([0-9.]+)ms.*interval\[n=\d+ p50=([0-9.]+)ms' | ForEach-Object { $_.Matches[0] } | Where-Object { [int]$_.Groups[1].Value -ge 30 })
            $comp = @($log | Select-String -Pattern 'compose (\d+) strips avg ([0-9.]+) us' | ForEach-Object { $_.Matches[0] })
            # The line's count is a running total and its average is the period's: weigh each period by its new strips.
            $prev = 0; $n = 0; $us = 0.0
            foreach ($m in $comp) { $t = [int]$m.Groups[1].Value; $d = $t - $prev; $prev = $t; $n += $d; $us += $d * [double]$m.Groups[2].Value }
            [pscustomobject]@{
                Run = "$v-$r"; Busy = $busy
                IntervalP50 = & $med ($scroll | ForEach-Object { [double]$_.Groups[3].Value })
                RenderP50 = & $med ($scroll | ForEach-Object { [double]$_.Groups[2].Value })
                Composed = $n; ComposeUs = if ($n) { [math]::Round($us / $n, 1) } else { 0 }
                PeakPrivateMB = $run.PeakPrivateMB; PeakWsMB = $run.PeakWsMB; Exit = $run.Exit
            }
        }
    }
    $rows | Format-Table -AutoSize
    return
}

# -Mode thumbs
Add-Type -AssemblyName System.Drawing
$pics = Join-Path $env:TEMP 'twinfm-p16m3-thumbs'
New-Item -ItemType Directory -Force $pics | Out-Null
$have = @(Get-ChildItem $pics -Filter *.jpg).Count
if ($have -lt $Pictures) {
    "making $($Pictures - $have) pictures in $pics"
    for ($i = $have + 1; $i -le $Pictures; $i++) {
        $bmp = New-Object System.Drawing.Bitmap 320, 240
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        $g.Clear([System.Drawing.Color]::FromArgb(255, ($i * 37) % 256, ($i * 91) % 256, ($i * 53) % 256))
        $g.FillEllipse([System.Drawing.Brushes]::White, ($i * 13) % 200, ($i * 7) % 140, 100, 80)
        $g.Dispose()
        $bmp.Save((Join-Path $pics ("pic-{0:00000}.jpg" -f $i)), [System.Drawing.Imaging.ImageFormat]::Jpeg)
        $bmp.Dispose()
    }
}
$cfg = Join-Path $out 'thumbs-xlarge.toml'
if (Test-Path $cfg) { Set-ItemProperty $cfg IsReadOnly $false }
[System.IO.File]::WriteAllText($cfg, "view_left = `"icons`"`ntile_left = `"xlarge`"`nthumbnails = true`n")
Set-ItemProperty $cfg IsReadOnly $true
# Three tiles to a row at the default window, and PageDown moves on by one grid row there (two whole
# rows on screen, a screenful less one): three pictures a page.
$pages = [int][math]::Ceiling($Pictures / 3) + 2
$steps = @('sleep 2500', 'click 0 0', 'sleep 500')
for ($k = 0; $k -lt $pages; $k++) { $steps += 'key PageDown'; $steps += "sleep $PageMs" }
$steps += @('sleep 3000', 'key Home', 'sleep 3000')
for ($k = 0; $k -lt 40; $k++) { $steps += 'key PageDown'; $steps += "sleep $PageMs" }
$steps += @('sleep 3000', 'quit')
$run = Invoke-Run 'thumbs-xlarge' ($steps -join ';') $cfg $pics ([int]($pages * ($PageMs + 450) / 1000) + 180)
$run.Log | Where-Object { $_ -match '^\[icons\] thumbnail (memory|cap)|panicked' }
($run.Log | Where-Object { $_ -match '^\[icons\] \+' } | Select-Object -Last 1) -replace ', worker.*', ''
$thumbs = @($run.Log | Select-String -Pattern 'thumbs (\d+) avg' | ForEach-Object { [int]$_.Matches[0].Groups[1].Value } | Measure-Object -Sum).Sum
"thumbnails rendered by the workers: $thumbs; peak private $($run.PeakPrivateMB) MB, last $($run.LastPrivateMB) MB, peak working set $($run.PeakWsMB) MB; exit $($run.Exit)"
