# Gate run under WPR. Must run ELEVATED at the console (not over RDP: the Remote Display
# Adapter presents at 32 Hz, which caps the frame cadence and hides the real 60 Hz behavior).
#   Right-click PowerShell > Run as administrator, then:
#   powershell -ExecutionPolicy Bypass -File C:\Users\jtkli\projects\rustfm-proto\tools\wpr-gate.ps1
# Produces <Etl> and <Etl basename>-stderr.txt (the in-app timers). Pass -Exe and -Etl to compare
# variants, e.g. the accessibility-off build (cargo build --release --no-default-features --target-dir target-noa11y):
#   ... wpr-gate.ps1 -Exe C:\Users\jtkli\projects\rustfm-proto\target-noa11y\release\rustfm-proto.exe -Etl C:\traces\rustfm-noa11y.etl
# Open the .etl in WPA: CPU Usage (Sampled) > Utilization by Process, Thread, Stack; filter to
# rustfm-proto.exe, main thread, and select the 10 s after the "[mark] F3: split requested" line.
param(
    [string]$Exe = "C:\Users\jtkli\projects\rustfm-proto\target\release\rustfm-proto.exe",
    [string]$Dir = "C:\traces\bigdir",
    [string]$Etl = "C:\traces\rustfm-proto.etl"
)
$Stderr = [System.IO.Path]::ChangeExtension($Etl, "stderr.txt")
$elev = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $elev) { throw "Run this elevated (WPR needs administrator)." }
if (-not $env:_NT_SYMBOL_PATH) { $env:_NT_SYMBOL_PATH = "srv*C:\Symbols*https://msdl.microsoft.com/download/symbols" }

# The scripted scenario: open the 40k folder, split after 2 s, scroll both panes by wheel and by
# dragging the scrollbar thumb, then sit idle so the 10 s busy window after the split completes.
$env:RUSTFM_SCRIPT = "sleep 2000; split; sleep 500; wheel 1 -120 16; wheel 0 -120 16; drag 1 400 1000; drag 0 400 1000; sleep 6000; sort 0 1; sleep 1000; sort 1 2; sleep 3000; quit"

wpr -start CPU -filemode
Start-Sleep -Seconds 1
$p = Start-Process -FilePath $Exe -ArgumentList $Dir -PassThru -RedirectStandardError $Stderr
$peak = 0
while (-not $p.HasExited) { $p.Refresh(); if ($p.WorkingSet64 -gt $peak) { $peak = $p.WorkingSet64 }; Start-Sleep -Milliseconds 250 }
wpr -stop $Etl
"peak working set MB: $([int]($peak / 1MB))"
"in-app timers:"
Get-Content $Stderr
