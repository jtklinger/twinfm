# P15-M3's scripted check of the small items: one run of target\release\twinfm.exe in a scratch tree,
# %TEMP%\twinfm-p15m3-check\work (folders d1, d2, d3 with a few files, notes.txt, a.zip made with Windows'
# tar.exe). No settings file. With SLINT_BACKEND=winit-software (the femtovg window captures blank); three
# pictures are taken with PrintWindow from the window of the run's own process id into the scratch folder:
# tabmenu.png (the menu on a background tab), flash.png (the saved search's sidebar row in the hover
# colour) and sizing.png (the status line and the Size cells while folders are sized).
# The steps: three tabs, the menu on the middle (background) one, Close other tabs; the only tab's menu
# (three items off); Duplicate; Close tabs to the right from the background tab; a right click on a tab
# through the real pointer path (-TabX, -TabY: a point on the left pane's first tab); a search tab, its
# menu's Save this search twice (saved, then saved already: the row flashed both times), its Duplicate
# (the search runs again); Size all folders in work (the sizing lines) and in -Big (the picture; the walk
# is cancelled by leaving); a.zip extracted Here twice, the second answered Skip ("Nothing extracted").
# Prints the [tab], [size] sizing, [sidebar], [toolbar] picks, [archive] extract and [fileops] lines.
param(
    [string]$Exe = (Join-Path $PSScriptRoot '..\target\release\twinfm.exe'),
    [string]$Big = 'C:\Program Files',
    [int]$TabX = 320,
    [int]$TabY = 60
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public class P15M3Shot {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
}
"@
$root = Join-Path $env:TEMP 'twinfm-p15m3-check'
if (Test-Path $root) { Remove-Item -Recurse -Force $root }
$work = Join-Path $root 'work'
$src = Join-Path $root 'src'
New-Item -ItemType Directory -Force $src | Out-Null
foreach ($d in 'd1', 'd2', 'd3') {
    New-Item -ItemType Directory -Force (Join-Path $work "$d\sub") | Out-Null
    1..5 | ForEach-Object { [System.IO.File]::WriteAllText((Join-Path $work "$d\sub\f$_.txt"), "file $_`r`n") }
}
[System.IO.File]::WriteAllText((Join-Path $work 'notes.txt'), "notes`r`n")
[System.IO.File]::WriteAllText((Join-Path $src 'top.txt'), "top`r`n")
& 'C:\Windows\System32\tar.exe' -cf (Join-Path $work 'a.zip') --format zip -C $src top.txt
if ($LASTEXITCODE) { throw 'tar.exe failed' }

# work lists d1, d2, d3, a.zip, notes.txt (rows 0 to 4).
$wait = 'waitops 60000'
$steps = @(
    'sleep 2500', "nav 0 $work", 'sleep 800',
    # Three tabs (the third active); the menu on the middle one (the picture), Close other tabs.
    'tab new', 'sleep 400', 'tab new', 'sleep 400',
    'tabmenu 0 1', 'waitmenu 2000', 'sleep 2001', 'toolmenu Close other tabs', 'sleep 600',
    # The only tab: Close, Close other tabs and Close tabs to the right are off.
    'tabmenu 0 0', 'waitmenu 2000', 'menuclose', 'sleep 200',
    # Duplicate (the duplicate is shown), then from the background first tab, Close tabs to the right.
    'tabmenu 0 0', 'waitmenu 2000', 'toolmenu Duplicate', 'sleep 800',
    'tabmenu 0 0', 'waitmenu 2000', 'toolmenu Close tabs to the right', 'sleep 600',
    # The real pointer path: a right click on the first tab.
    "clickat $TabX $TabY right", 'waitmenu 2000', 'menuclose', 'sleep 200',
    # A search tab (the second tab), the first tab shown again; its menu from the background.
    'filter 0 notes', 'sleep 300', 'searchreturn 0', 'waitsearch 5000', 'sleep 500', 'tab 1', 'sleep 400',
    'tabmenu 0 1', 'waitmenu 2000', 'toolmenu Save this search', 'sleep 2003',
    'tabmenu 0 1', 'waitmenu 2000', 'toolmenu Save this search', 'sleep 500',
    'tabmenu 0 1', 'waitmenu 2000', 'toolmenu Duplicate', 'waitsearch 5000', 'sleep 500',
    'tabmenu 0 0', 'waitmenu 2000', 'toolmenu Close other tabs', 'sleep 600', 'filter 0', 'sleep 300',
    # Size all folders: the count appears and goes.
    'sizeall 0', 'waitsize 10000', 'sleep 300'
)
if (Test-Path $Big) {
    # A folder whose sizes take a while (the picture); leaving it cancels the walks.
    $steps += @("nav 0 $Big", 'sleep 1500', 'sizeall 0', 'sleep 2002', "nav 0 $work", 'sleep 800')
}
$steps += @(
    # Here twice: the second finds top.txt there, Skip leaves it: nothing extracted.
    'click 0 3', 'extract 0 here', $wait,
    'click 0 3', 'extract 0 here', 'waitquestion 5000', 'answer Skip', $wait,
    'sleep 500', 'quit'
)
$script = Join-Path $root 'script.txt'
[System.IO.File]::WriteAllText($script, ($steps -join ';'))
$err = Join-Path $root 'stderr.txt'
$env:TWINFM_SCRIPT = "@$script"
$env:SLINT_BACKEND = 'winit-software'
Remove-Item Env:TWINFM_CONFIG -ErrorAction SilentlyContinue
try {
    $proc = Start-Process -FilePath $Exe -PassThru -RedirectStandardError $err
} finally {
    Remove-Item Env:TWINFM_SCRIPT, Env:SLINT_BACKEND -ErrorAction SilentlyContinue
}
$shots = [ordered]@{ '[auto] Sleep(2001)' = 'tabmenu.png'; '[auto] Sleep(2003)' = 'flash.png'; '[auto] Sleep(2002)' = 'sizing.png' }
$taken = @{}
$deadline = (Get-Date).AddSeconds(240)
while (-not $proc.HasExited -and (Get-Date) -lt $deadline) {
    Start-Sleep -Milliseconds 100
    $text = if (Test-Path $err) { Get-Content $err -Raw -ErrorAction SilentlyContinue } else { '' }
    foreach ($mark in $shots.Keys) {
        if (-not $taken[$mark] -and $text -and $text.Contains($mark)) {
            Start-Sleep -Milliseconds 300
            $proc.Refresh()
            $h = $proc.MainWindowHandle
            $r = New-Object P15M3Shot+RECT
            [P15M3Shot]::GetWindowRect($h, [ref]$r) | Out-Null
            $bmp = New-Object System.Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
            $g = [System.Drawing.Graphics]::FromImage($bmp)
            $hdc = $g.GetHdc()
            [P15M3Shot]::PrintWindow($h, $hdc, 2) | Out-Null
            $g.ReleaseHdc($hdc)
            $png = Join-Path $root $shots[$mark]
            $bmp.Save($png, [System.Drawing.Imaging.ImageFormat]::Png)
            $g.Dispose(); $bmp.Dispose()
            $taken[$mark] = $true
            "captured: $png"
        }
    }
}
if (-not $proc.HasExited) { $proc.Kill(); 'timed out' }
$proc.WaitForExit()
Get-Content $err | Where-Object { $_ -match '^\[tab\] pane=\d+ (menu|tab \d+:|closed|opened|the menu)|^\[size\] pane=\d+ (sizing|Alt)|^\[sidebar\] (saved|")|^\[toolbar\] pane=\d+ picks|^\[archive\] (extract|\d+ file)|^\[fileops\] (done|pane=\d+ note: Nothing)|^\[ask\]|^\[search\] pane=\d+ gen=\d+ (start|done)|^\[auto\] (clickat|waitmenu:|toolmenu|menupick)|panicked' }
"exit $($proc.ExitCode)"
