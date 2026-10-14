# The gate protocol since P2-M5 in one command: three runs per binary, interleaved (reference,
# candidate, reference, candidate, reference, candidate), each through wpr-gate.ps1.
# Elevated PowerShell at the physical console, every TwinFM window closed, Narrator off:
#   powershell -ExecutionPolicy Bypass -File C:\Users\jtkli\projects\twinfm\tools\wpr-triplet.ps1 -Reference p2m5 -Candidate p3m1 -Tag ab10
# Several candidates (P3-M5's two trace variants) are a comma-separated list; every round runs
# the reference first, then each candidate in the order given:
#   ... -Reference p3m4 -Candidate p3m5a,p3m5b -Tag ab14
# Builds are named by their target dir (target-<name>\release\twinfm.exe). Traces land at
# C:\traces\twinfm-<tag>-<name>-<n>.etl with the in-app lines beside them (.stderr.txt).
# Analyse afterwards without elevation:
#   python tools\analyze-trace.py C:\traces\twinfm-<tag>-<name>-<n>.etl --symbols --exe target-<name>\release\twinfm.exe
param(
    [Parameter(Mandatory = $true)][string]$Reference,
    [Parameter(Mandatory = $true)][string]$Candidate,
    [Parameter(Mandatory = $true)][string]$Tag,
    [int]$Runs = 3,
    [string]$Root = "C:\Users\jtkli\projects\twinfm"
)
$elev = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $elev) { throw "Run this elevated (WPR needs administrator)." }
$builds = @($Reference) + @($Candidate -split ',' | ForEach-Object { $_.Trim() } | Where-Object { $_ })
foreach ($b in $builds) {
    $exe = "$Root\target-$b\release\twinfm.exe"
    if (-not (Test-Path $exe)) { throw "$exe is not built" }
}
if (Get-Process -Name twinfm -ErrorAction SilentlyContinue) {
    throw "a TwinFM window is open: close every one before the gate (it would share the machine and appear in the trace)"
}
$started = Get-Date
for ($n = 1; $n -le $Runs; $n++) {
    foreach ($b in $builds) {
        $exe = "$Root\target-$b\release\twinfm.exe"
        $etl = "C:\traces\twinfm-$Tag-$b-$n.etl"
        "=== run $n of $Runs, $b ($(Get-Date -Format HH:mm:ss))"
        & "$Root\tools\wpr-gate.ps1" -Exe $exe -Etl $etl | Select-String -Pattern "busy|split\]|peak|panicked"
        Start-Sleep -Seconds 3
    }
}
"done: $($Runs * $builds.Count) runs, $([int]((Get-Date) - $started).TotalMinutes) min; traces C:\traces\twinfm-$Tag-*.etl"
