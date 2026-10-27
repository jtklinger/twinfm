# P12-M7's in-app comparison on OneDrive (the plan's gate item): the same read-only scenario on two
# builds, reference then candidate, then swapped (the second slot reads high, ab53 and ab54), summing
# each run's once-a-second [icons] `cloud N` (property reads) and `slow N` (renders and lookups of
# 100 ms or more), its `[cloud] state read` lines and its `[cloud] ... not in sync` lines. It only
# lists OneDrive folders and scrolls one; nothing is opened, downloaded or written there. Unelevated,
# no trace; run on a quiet machine (the coordinating session's slot), every TwinFM window closed.
#   powershell -ExecutionPolicy Bypass -File tools\check-p12m7-onedrive.ps1 -Reference <exe> -Candidate <exe>
param(
    [Parameter(Mandatory)][string]$Reference,
    [Parameter(Mandatory)][string]$Candidate,
    [string]$Out = "$env:TEMP\twinfm-p12m7-onedrive"
)
$ErrorActionPreference = 'Stop'
if (Get-Process -Name twinfm -ErrorAction SilentlyContinue) { throw 'A TwinFM window is open: close every one first.' }
foreach ($exe in $Reference, $Candidate) { if (-not (Test-Path $exe)) { throw "$exe is missing." } }
$od = 'C:\Users\jtkli\OneDrive'
foreach ($d in $od, "$od\Documents", "$od\Pictures\Camera Roll") { if (-not (Test-Path $d)) { throw "$d is missing." } }
New-Item -ItemType Directory -Force $Out | Out-Null
$steps = @(
    "nav 0 $od", 'sleep 3000',
    "nav 0 $od\Documents", 'sleep 3000', 'key F5', 'sleep 2500',
    "nav 0 $od\Pictures\Camera Roll", 'sleep 3000',
    'wheel 0 -10 150', 'sleep 2000', 'wheel 0 -10 150', 'sleep 2000', 'wheel 0 20 150', 'sleep 2000',
    'key F5', 'sleep 3000', 'quit'
)
$script = Join-Path $Out 'scenario.txt'
($steps -join ';') | Set-Content -Encoding ascii $script
Remove-Item Env:TWINFM_SOAK, Env:TWINFM_CONFIG -ErrorAction SilentlyContinue
$env:TWINFM_SCRIPT = "@$script"
$rows = @()
$order = @(@('ref', $Reference), @('cand', $Candidate), @('cand', $Candidate), @('ref', $Reference))
$n = 0
foreach ($o in $order) {
    $n++
    $err = Join-Path $Out "run$n-$($o[0]).stderr.txt"
    $p = Start-Process -FilePath $o[1] -RedirectStandardError $err -PassThru -Wait
    $log = Get-Content $err
    $icons = @($log | Select-String -Pattern '^\[icons\] \+\d+ requests.*slow (\d+),.*cloud (\d+)' | ForEach-Object { $_.Matches[0] })
    $rows += [pscustomobject]@{
        Run = "$n $($o[0])"
        Exit = $p.ExitCode
        CloudReads = ($icons | ForEach-Object { [int]$_.Groups[2].Value } | Measure-Object -Sum).Sum
        Slow = ($icons | ForEach-Object { [int]$_.Groups[1].Value } | Measure-Object -Sum).Sum
        StateReads = @($log | Select-String -SimpleMatch '[cloud] state read').Count
        NotInSync = (@($log | Select-String -SimpleMatch 'not in sync:' | ForEach-Object { $_.Line -replace '^.*not in sync: ', '' }) -join '; ')
        Panics = @($log | Select-String -SimpleMatch 'panicked').Count
    }
}
$rows | Format-Table -AutoSize | Out-String -Width 250
"logs: $Out"
