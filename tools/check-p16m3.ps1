# P16-M3's scripted check of extra large and small icons: one run of target\release\twinfm.exe in a
# scratch tree, %TEMP%\twinfm-p16m3-check\work (three folders, forty text files, one with a long name,
# and twelve copies of Windows' wallpaper JPEGs). No settings file. With SLINT_BACKEND=winit-software
# (the femtovg window captures blank); pictures are taken with PrintWindow from the window of the
# run's own process id into -Out.
# The steps: the left pane on work; each view by its key (`view xlarge|large|medium|small|details`,
# Explorer's Ctrl+Shift+1, 2, 3, 4 and 6) with a picture of extra large and small; in each of the two
# new views a click on a tile, PageDown and PageUp, a band from the gap after the last tile, a drop
# onto a folder tile (a copy), a cancelled drag out of a tile, F2 (the rename editor) and Escape, and a
# new folder named from the toolbar; then thumbnails on (the settings dialog) and extra large again
# (the pictures as thumbnails), small again, and back to details.
# Prints the [view], [grid], [band] done, [dnd] drag image and Drop lines, the [fileops] done lines and
# the [icons] list geometry lines.
param(
    [string]$Exe = (Join-Path $PSScriptRoot '..\target\release\twinfm.exe'),
    [string]$Out = (Join-Path $env:TEMP 'twinfm-p16m3-check\shots'),
    [string]$Scheme = ''
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public class P16M3Shot {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
}
"@
$root = Join-Path $env:TEMP 'twinfm-p16m3-check'
$work = Join-Path $root 'work'
$src = Join-Path $root 'src'
foreach ($d in $work, $src) { if (Test-Path $d) { Remove-Item -Recurse -Force $d } }
New-Item -ItemType Directory -Force (Join-Path $work 'Alpha'), (Join-Path $work 'Beta'), (Join-Path $work 'Gamma'), $src, $Out | Out-Null
1..40 | ForEach-Object { [System.IO.File]::WriteAllText((Join-Path $work ("note-{0:00}.txt" -f $_)), "note $_`r`n") }
[System.IO.File]::WriteAllText((Join-Path $work 'a file with quite a long name that does not fit in one small cell.txt'), "long`r`n")
foreach ($v in 'xlarge', 'small') { [System.IO.File]::WriteAllText((Join-Path $src "dropped-$v.txt"), "dropped`r`n") }
$jpegs = @(Get-ChildItem 'C:\Windows\Web' -Recurse -Filter *.jpg -ErrorAction SilentlyContinue | Select-Object -First 12)
$k = 0
foreach ($j in $jpegs) { $k++; Copy-Item $j.FullName (Join-Path $work ("photo-{0:00}.jpg" -f $k)) }

# work lists Alpha, Beta, Gamma, then the files by name (rows 0 to 2 are the folders).
$wait = 'waitops 60000'
$new = {
    param($view, $mark, $band)
    @(
        "view $view", "sleep $mark", 'click 0 4', 'sleep 200', 'key PageDown', 'sleep 200', 'key PageUp', 'sleep 200',
        'key End', 'sleep 300', 'key Home', 'sleep 300',
        "dropfiles 0 1 copy $(Join-Path $src "dropped-$view.txt")", $wait, 'sleep 300',
        'dragrow 0 5 40 10', 'sleep 400', 'dragcancel', 'sleep 300',
        'click 0 6', 'key F2', "sleep $($mark + 100)", 'key Escape', 'sleep 300',
        'toolbar 0 newfolder', "sleep $($mark + 200)", 'key Escape', 'sleep 300',
        'key End', 'sleep 400', "band 0 $band", 'sleep 400', 'key Home', 'sleep 300'
    )
}
$steps = @('sleep 2500', "nav 0 $work", 'sleep 1000')
$steps += & $new 'xlarge' 1501 '700 500 400 450'
$steps += & $new 'small' 1502 '300 330 100 300'
$steps += @(
    'view large', 'sleep 500', 'view medium', 'sleep 500', 'view details', 'sleep 500',
    'settings open', 'sleep 300', 'settings set thumbnails true', 'settings ok', 'sleep 800',
    'view xlarge', 'sleep 2503', 'key End', 'sleep 2504',
    'view small', 'sleep 1505', 'view details', 'sleep 500',
    'settings open', 'sleep 300', 'settings set view_left xlarge', 'settings ok', 'sleep 600',
    'settings open', 'sleep 300', 'settings set view_left details', 'settings set thumbnails false', 'settings ok', 'sleep 600',
    'quit'
)
$script = Join-Path $root 'script.txt'
[System.IO.File]::WriteAllText($script, ($steps -join ';'))
$err = Join-Path $root 'stderr.txt'
$env:TWINFM_SCRIPT = "@$script"
$env:SLINT_BACKEND = 'winit-software'
if ($Scheme) { $env:TWINFM_SCHEME = $Scheme }
Remove-Item Env:TWINFM_CONFIG -ErrorAction SilentlyContinue
try {
    $proc = Start-Process -FilePath $Exe -PassThru -RedirectStandardError $err
    $null = $proc.Handle # kept, or ExitCode reads blank after the exit
} finally {
    Remove-Item Env:TWINFM_SCRIPT, Env:SLINT_BACKEND, Env:TWINFM_SCHEME -ErrorAction SilentlyContinue
}
$shots = [ordered]@{
    '[auto] Sleep(1501)' = 'xlarge.png'; '[auto] Sleep(1502)' = 'small.png'; '[auto] Sleep(2503)' = 'xlarge-thumbs.png'
    '[auto] Sleep(2504)' = 'xlarge-thumbs-end.png'; '[auto] Sleep(1505)' = 'small-thumbs.png'
    '[auto] Sleep(1601)' = 'xlarge-rename.png'; '[auto] Sleep(1701)' = 'xlarge-newfolder.png'
    '[auto] Sleep(1602)' = 'small-rename.png'; '[auto] Sleep(1702)' = 'small-newfolder.png'
}
$taken = @{}
$deadline = (Get-Date).AddSeconds(180)
while (-not $proc.HasExited -and (Get-Date) -lt $deadline) {
    Start-Sleep -Milliseconds 100
    $text = if (Test-Path $err) { Get-Content $err -Raw -ErrorAction SilentlyContinue } else { '' }
    foreach ($mark in $shots.Keys) {
        if (-not $taken[$mark] -and $text -and $text.Contains($mark)) {
            Start-Sleep -Milliseconds 1100
            $proc.Refresh()
            $h = $proc.MainWindowHandle
            $r = New-Object P16M3Shot+RECT
            [P16M3Shot]::GetWindowRect($h, [ref]$r) | Out-Null
            $bmp = New-Object System.Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
            $g = [System.Drawing.Graphics]::FromImage($bmp)
            $hdc = $g.GetHdc()
            [P16M3Shot]::PrintWindow($h, $hdc, 2) | Out-Null
            $g.ReleaseHdc($hdc)
            $png = Join-Path $Out $shots[$mark]
            $bmp.Save($png, [System.Drawing.Imaging.ImageFormat]::Png)
            $g.Dispose(); $bmp.Dispose()
            $taken[$mark] = $true
            "captured: $png"
        }
    }
}
if (-not $proc.HasExited) { $proc.Kill(); 'timed out' }
$proc.WaitForExit()
Get-Content $err | Where-Object { $_ -match '^\[view\]|^\[grid\]|^\[band\] pane=\d+ done|^\[dnd\] (drag image|Drop at)|^\[fileops\] done|^\[icons\] (list geometry|thumbnail memory)|^\[settings\] dialog: (view|thumb)|^\[auto\] .*(no such|refused|disabled)|panicked' } | ForEach-Object { $_ -replace [regex]::Escape($root), '...' }
"exit $($proc.ExitCode)"
