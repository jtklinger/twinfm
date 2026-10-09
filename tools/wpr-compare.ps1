# Run the gate scenario under WPR for several build variants in one elevated session.
# Elevated PowerShell at the physical console:
#   powershell -ExecutionPolicy Bypass -File C:\Users\jtkli\projects\rustfm-proto\tools\wpr-compare.ps1 [-Set accessibility|renderer]
# Each variant produces C:\traces\rustfm-<name>.etl and C:\traces\rustfm-<name>.stderr.txt.
# Analyse afterwards (no elevation needed) with:  python tools\analyze-trace.py C:\traces\rustfm-<name>.etl --symbols
#
# -Set accessibility (default): verified unpatched master vs the week-1 patches at 50 ms and 0 ms.
# -Set renderer: the same patched code with femtovg (OpenGL) vs Skia (Direct3D), one binary
#   built with `cargo build --release --features skia --target-dir target-skia`.
param([string]$Root = "C:\Users\jtkli\projects\rustfm-proto", [string]$Set = "accessibility")
$elev = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $elev) { throw "Run this elevated (WPR needs administrator)." }

$sets = @{
    accessibility = @(
        @{ name = "master";    exe = "$Root\target-master\release\rustfm-proto.exe";  env = @{} },
        @{ name = "patched50"; exe = "$Root\target-patched\release\rustfm-proto.exe"; env = @{ SLINT_ACCESSKIT_UPDATE_DELAY_MS = "50" } },
        @{ name = "patched0";  exe = "$Root\target-patched\release\rustfm-proto.exe"; env = @{ SLINT_ACCESSKIT_UPDATE_DELAY_MS = "0" } }
    )
    renderer = @(
        @{ name = "femtovg"; exe = "$Root\target-skia\release\rustfm-proto.exe"; env = @{ SLINT_BACKEND = "winit-femtovg" } },
        @{ name = "skia";    exe = "$Root\target-skia\release\rustfm-proto.exe"; env = @{ SLINT_BACKEND = "winit-skia" } }
    )
}
$variants = $sets[$Set]
if (-not $variants) { throw "unknown -Set $Set (accessibility | renderer)" }

foreach ($v in $variants) {
    if (-not (Test-Path $v.exe)) { "skipping $($v.name): $($v.exe) not built"; continue }
    "=== $($v.name)"
    foreach ($k in $v.env.Keys) { Set-Item -Path "Env:$k" -Value $v.env[$k] }
    & "$Root\tools\wpr-gate.ps1" -Exe $v.exe -Etl "C:\traces\rustfm-$($v.name).etl" | Select-String -Pattern "busy|split\]|peak|interval"
    foreach ($k in $v.env.Keys) { Remove-Item -Path "Env:$k" -ErrorAction SilentlyContinue }
    Start-Sleep -Seconds 3
}
"done"
