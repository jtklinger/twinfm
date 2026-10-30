# P14-M3's scripted check: the hover in the details list (a plain row, a selected row, the row beside a
# selection), in the large-icon grid, off over the scrollbar; then a held press on the left list's track
# each way. One run of target\release\twinfm.exe on C:\traces\bigdir with SLINT_BACKEND=winit-software (the
# femtovg window captures blank), no settings file, no clipboard or Recycle Bin steps. Each pose ends in a
# sleep of its own length (2001, 2002, ...); when stderr logs it, the window found by the run's own process
# id is captured with PrintWindow into %TEMP%\twinfm-p14m3-check\pose-N.png. Prints the [auto] trackhold
# lines and any panic.
param(
    [string]$Exe = (Join-Path $PSScriptRoot '..\target\release\twinfm.exe'),
    [string]$Scheme = 'light'
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public class P14Shot {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
}
"@
$out = Join-Path $env:TEMP 'twinfm-p14m3-check'
if (Test-Path $out) { Remove-Item -Recurse -Force $out }
New-Item -ItemType Directory -Force $out | Out-Null
$poses = @(
    @('hover 600 260'),                          # 1: a plain row under the pointer
    @('click 0 4', 'sleep 300', 'hover 600 260'),  # 2: row 4 selected, the pointer on it (no hover over the selection)
    @('hover 600 301'),                          # 3: a plain row two below the selection
    @('hover 600 323'),                          # 4: the next row down
    @('hover 1193 260'),                         # 5: over the list's scrollbar strip (no hover drawn)
    @('view icons', 'sleep 1200', 'hover 330 260'),  # 6: a tile in the grid
    @('hover 330 420'),                          # 7: a tile in the second grid row
    @('view details', 'sleep 800', 'hover 20 20')  # 8: off the lists
)
$steps = @('sleep 2500')
$n = 0
foreach ($p in $poses) { $n++; $steps += $p; $steps += "sleep $(2000 + $n)" }
$steps += @('trackhold 0 bottom 1500', 'sleep 300', 'click 0 1', 'key End', 'sleep 600', 'trackhold 0 top 1500', 'sleep 300', 'quit')
$err = Join-Path $out 'stderr.txt'
$env:TWINFM_SCRIPT = ($steps -join ';')
$env:SLINT_BACKEND = 'winit-software'
$env:TWINFM_SCHEME = $Scheme
Remove-Item Env:TWINFM_CONFIG -ErrorAction SilentlyContinue
try {
    $proc = Start-Process -FilePath $Exe -ArgumentList 'C:\traces\bigdir' -PassThru -RedirectStandardError $err
} finally {
    Remove-Item Env:TWINFM_SCRIPT, Env:SLINT_BACKEND, Env:TWINFM_SCHEME -ErrorAction SilentlyContinue
}
$taken = @{}
$deadline = (Get-Date).AddSeconds(90)
while (-not $proc.HasExited -and (Get-Date) -lt $deadline) {
    Start-Sleep -Milliseconds 200
    $text = if (Test-Path $err) { Get-Content $err -Raw -ErrorAction SilentlyContinue } else { '' }
    for ($k = 1; $k -le $poses.Count; $k++) {
        if (-not $taken[$k] -and $text -and $text.Contains("[auto] Sleep($(2000 + $k))")) {
            Start-Sleep -Milliseconds 600
            $proc.Refresh()
            $h = $proc.MainWindowHandle
            $r = New-Object P14Shot+RECT
            [P14Shot]::GetWindowRect($h, [ref]$r) | Out-Null
            $bmp = New-Object System.Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
            $g = [System.Drawing.Graphics]::FromImage($bmp)
            $hdc = $g.GetHdc()
            [P14Shot]::PrintWindow($h, $hdc, 2) | Out-Null
            $g.ReleaseHdc($hdc)
            $png = Join-Path $out "pose-$k.png"
            $bmp.Save($png, [System.Drawing.Imaging.ImageFormat]::Png)
            $g.Dispose(); $bmp.Dispose()
            $taken[$k] = $true
            "pose $k captured: $png"
        }
    }
}
if (-not $proc.HasExited) { $proc.Kill(); 'timed out' }
Get-Content $err | Where-Object { $_ -match '^\[auto\] trackhold|panicked' }
"exit $($proc.ExitCode)"
