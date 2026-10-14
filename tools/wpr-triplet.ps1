# The gate protocol since P2-M5 in one command: three runs per binary, interleaved (reference,
# candidate, reference, candidate, reference, candidate), each through wpr-gate.ps1.
# Elevated PowerShell at the physical console, every TwinFM window closed, Narrator off:
#   powershell -ExecutionPolicy Bypass -File C:\Users\jtkli\projects\twinfm\tools\wpr-triplet.ps1 -Reference p2m5 -Candidate p3m1 -Tag ab10
# Several candidates (P3-M5's two trace variants) are a comma-separated list; every round runs
# the reference first, then each candidate in the order given:
#   ... -Reference p3m4 -Candidate p3m5a,p3m5b -Tag ab14
# A candidate run with a settings file (the gate on a non-default setting) is <name>@<toml path>;
# its traces are named <name>-<toml basename>, so one binary can run with and without it:
#   ... -Reference p3m4 -Candidate p3m5,p3m5@C:\traces\thumbs.toml -Tag ab15
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
# Each entry: name, the settings file (or ""), and the label the trace files carry.
$builds = @()
foreach ($spec in @($Reference) + @($Candidate -split ',' | ForEach-Object { $_.Trim() } | Where-Object { $_ })) {
    $name, $cfg = $spec -split '@', 2
    $label = if ($cfg) { "$name-" + [System.IO.Path]::GetFileNameWithoutExtension($cfg) } else { $name }
    $exe = "$Root\target-$name\release\twinfm.exe"
    if (-not (Test-Path $exe)) { throw "$exe is not built" }
    if ($cfg -and -not (Test-Path $cfg)) { throw "$cfg does not exist" }
    $builds += [pscustomobject]@{ Name = $name; Config = [string]$cfg; Label = $label; Exe = $exe }
}
if (Get-Process -Name twinfm -ErrorAction SilentlyContinue) {
    throw "a TwinFM window is open: close every one before the gate (it would share the machine and appear in the trace)"
}
$started = Get-Date
for ($n = 1; $n -le $Runs; $n++) {
    foreach ($b in $builds) {
        $etl = "C:\traces\twinfm-$Tag-$($b.Label)-$n.etl"
        "=== run $n of $Runs, $($b.Label) ($(Get-Date -Format HH:mm:ss))"
        # The gate script beside this one, not $Root's: a worktree's triplet must run its own
        # wpr-gate.ps1 (a plain script swallows a parameter it does not know, such as -Config).
        & "$PSScriptRoot\wpr-gate.ps1" -Exe $b.Exe -Etl $etl -Config $b.Config | Select-String -Pattern "busy|split\]|peak|panicked"
        Start-Sleep -Seconds 3
    }
}
"done: $($Runs * $builds.Count) runs, $([int]((Get-Date) - $started).TotalMinutes) min; traces C:\traces\twinfm-$Tag-*.etl"
