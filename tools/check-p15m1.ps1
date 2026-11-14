# P15-M1's scripted check of Extract to: one run of target\release\twinfm.exe in a scratch tree,
# %TEMP%\twinfm-p15m1-check\ (work: a.zip, b.7z, c.tar.gz, d.zip, notes.txt, pw.zip with the password
# twinfm; other: empty), made here with Windows' tar.exe. No settings file. With SLINT_BACKEND=winit-software
# (the femtovg window captures blank); the menu with its Extract to submenu open is captured with PrintWindow
# from the window of the run's own process id into %TEMP%\twinfm-p15m1-check\menu.png.
# The steps: the menu on a.zip and its submenu (the picture), "a\" picked, the cursor's row, undo; Here and
# undo; Other pane with the split on, undo; a.zip twice (the second folder numbered); the 7z and the tar.gz
# through the shell; two zips at once ("A folder each"); a zip and a text file (no Extract to); the password
# zip without a password (refused) and with the script's; Here three times (the replace question: Skip, then
# Replace, then Ctrl+Z refused); Open in other pane on a.zip; and, when C:\traces\bigdir.zip is there, its
# extract into `other` stopped by Escape. Every undo puts a folder or two items in the Recycle Bin.
# Prints the [archive] extract, [winmenu], [undo], [ask] and [fileops] done lines.
param(
    [string]$Exe = (Join-Path $PSScriptRoot '..\target\release\twinfm.exe'),
    [string]$Big = 'C:\traces\bigdir.zip'
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public class P15Shot {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
}
"@
$root = Join-Path $env:TEMP 'twinfm-p15m1-check'
if (Test-Path $root) { Remove-Item -Recurse -Force $root }
$work = Join-Path $root 'work'
$other = Join-Path $root 'other'
$src = Join-Path $root 'src'
New-Item -ItemType Directory -Force $work, $other, (Join-Path $src 'tree\d') | Out-Null
[System.IO.File]::WriteAllText((Join-Path $src 'top.txt'), "top`r`n")
[System.IO.File]::WriteAllText((Join-Path $src 'tree\f.txt'), "file`r`n")
[System.IO.File]::WriteAllText((Join-Path $src 'tree\d\g.txt'), "deeper`r`n")
[System.IO.File]::WriteAllText((Join-Path $work 'notes.txt'), "not an archive`r`n")
$tar = 'C:\Windows\System32\tar.exe'
& $tar -cf (Join-Path $work 'a.zip') --format zip -C $src top.txt tree
& $tar -cf (Join-Path $work 'b.7z') --format 7zip -C $src top.txt tree
& $tar -czf (Join-Path $work 'c.tar.gz') -C $src top.txt tree
& $tar -cf (Join-Path $work 'd.zip') --format zip -C $src tree
& $tar -cf (Join-Path $work 'pw.zip') --format zip --options zip:encryption=zipcrypt --passphrase twinfm -C $src top.txt
if ($LASTEXITCODE) { throw 'tar.exe failed' }

# work lists a.zip, b.7z, c.tar.gz, d.zip, notes.txt, pw.zip (rows 0 to 5); a folder made there sorts first.
$wait = 'waitops 60000'
$steps = @(
    'sleep 2500', "nav 0 $work", 'sleep 800',
    # The menu on one zip, its submenu (captured), the folder named after it; the cursor goes to it.
    'click 0 0', 'rclick 0 0', 'waitmenu 2000', 'menuopen Extract to', 'sleep 2001',
    'menupick a\', $wait, 'scrollpos 0', 'undo', $wait,
    # Here, undone (two items to the Recycle Bin).
    'click 0 0', 'extract 0 here', $wait, 'undo', $wait,
    # Other pane: offered once two panes show and the other is on a folder.
    'split', 'sleep 500', "nav 1 $other", 'sleep 800',
    'click 0 0', 'rclick 0 0', 'waitmenu 2000', 'menuopen Extract to', 'sleep 300', 'menupick Other pane', $wait, 'scrollpos 1', 'undo', $wait,
    # Twice into the other pane: the second folder is numbered. Left there.
    'click 0 0', 'extract 0 other', $wait, 'click 0 0', 'extract 0 other', $wait,
    # The other formats, through the shell.
    'click 0 1', 'extract 0 other', $wait, 'click 0 2', 'extract 0 other', $wait,
    # Two zips at once; a zip and a text file.
    'click 0 0', 'ctrlclick 0 3', 'rclick 0 3', 'waitmenu 2000', 'menuopen Extract to', 'sleep 300', 'menupick A folder each', $wait, 'undo', $wait, 'undo', $wait,
    'click 0 0', 'ctrlclick 0 4', 'rclick 0 4', 'waitmenu 2000', 'menuclose', 'sleep 200',
    # The password zip: refused without the script's password, unpacked with it.
    'click 0 5', 'extract 0 other', $wait, 'zippassword twinfm', 'extract 0 other', $wait,
    # Here twice in work itself (tree sorts first, so a.zip is row 1 after it): the replace question,
    # skipped, then replaced; no undo after a replace.
    'click 0 0', 'extract 0 here', $wait,
    'click 0 1', 'extract 0 here', 'waitquestion 5000', 'answer Skip', $wait,
    'click 0 1', 'extract 0 here', 'waitquestion 5000', 'answer Replace', $wait, 'undo', 'sleep 500',
    # Open in other pane on a zip.
    'click 0 1', 'rclick 0 1', 'waitmenu 2000', 'menupick Open in other pane', 'sleep 1200'
)
if (Test-Path $Big) {
    # A large one, stopped by Escape (the files written so far stay in `other`).
    $name = Split-Path $Big -Leaf
    $steps += @("nav 1 $other", 'sleep 800', "nav 0 $(Split-Path $Big)", 'sleep 1500', "filter 0 $name", 'sleep 800', 'click 0 0',
        'extract 0 other', 'sleep 2500', 'key Escape', $wait)
}
$steps += @('sleep 500', 'quit')
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
$taken = $false
$deadline = (Get-Date).AddSeconds(420)
while (-not $proc.HasExited -and (Get-Date) -lt $deadline) {
    Start-Sleep -Milliseconds 200
    $text = if (Test-Path $err) { Get-Content $err -Raw -ErrorAction SilentlyContinue } else { '' }
    if (-not $taken -and $text -and $text.Contains('[auto] Sleep(2001)')) {
        Start-Sleep -Milliseconds 800
        $proc.Refresh()
        $h = $proc.MainWindowHandle
        $r = New-Object P15Shot+RECT
        [P15Shot]::GetWindowRect($h, [ref]$r) | Out-Null
        $bmp = New-Object System.Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        $hdc = $g.GetHdc()
        [P15Shot]::PrintWindow($h, $hdc, 2) | Out-Null
        $g.ReleaseHdc($hdc)
        $png = Join-Path $root 'menu.png'
        $bmp.Save($png, [System.Drawing.Imaging.ImageFormat]::Png)
        $g.Dispose(); $bmp.Dispose()
        $taken = $true
        "menu captured: $png"
    }
}
if (-not $proc.HasExited) { $proc.Kill(); 'timed out' }
$proc.WaitForExit()
Get-Content $err | Where-Object { $_ -match '^\[archive\] (extract|password|refused|\d+ file)|^\[winmenu\] (pane=\d+ seq=\d+ shown|submenu|pane=\d+ runs)|^\[undo\] (done|recorded|refused|cannot)|^\[ask\]|^\[fileops\] (done|Escape|pane=\d+ note: (Cancelling|Extract|Cannot))|^\[menu\] open in other|^\[auto\] (extract|menupick|scrollpos|waitquestion|answer)|^\[nav\] pane=1 gen=\d+ .*a\.zip|panicked' }
"exit $($proc.ExitCode)"
"left in ${other}: $((Get-ChildItem $other -Name) -join ', ')"
