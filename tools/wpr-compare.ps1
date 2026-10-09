# Run the gate scenario under WPR for several build variants in one elevated session.
# Elevated PowerShell at the physical console:
#   powershell -ExecutionPolicy Bypass -File C:\Users\jtkli\projects\rustfm-proto\tools\wpr-compare.ps1
# Each variant produces C:\traces\rustfm-<name>.etl and C:\traces\rustfm-<name>.stderr.txt.
# Analyse afterwards (no elevation needed) with:  python tools\analyze-trace.py C:\traces\rustfm-<name>.etl
param([string]$Root = "C:\Users\jtkli\projects\rustfm-proto")
$elev = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $elev) { throw "Run this elevated (WPR needs administrator)." }

$variants = @(
    @{ name = "master";    exe = "$Root\target-master\release\rustfm-proto.exe";  env = @{} },
    @{ name = "patched50"; exe = "$Root\target-patched\release\rustfm-proto.exe"; env = @{ SLINT_ACCESSKIT_UPDATE_DELAY_MS = "50" } },
    @{ name = "patched0";  exe = "$Root\target-patched\release\rustfm-proto.exe"; env = @{ SLINT_ACCESSKIT_UPDATE_DELAY_MS = "0" } }
)
foreach ($v in $variants) {
    if (-not (Test-Path $v.exe)) { "skipping $($v.name): $($v.exe) not built"; continue }
    "=== $($v.name)"
    foreach ($k in $v.env.Keys) { Set-Item -Path "Env:$k" -Value $v.env[$k] }
    & "$Root\tools\wpr-gate.ps1" -Exe $v.exe -Etl "C:\traces\rustfm-$($v.name).etl" | Select-String -Pattern "busy|split\]|peak|interval"
    foreach ($k in $v.env.Keys) { Remove-Item -Path "Env:$k" -ErrorAction SilentlyContinue }
    Start-Sleep -Seconds 3
}
"done"
