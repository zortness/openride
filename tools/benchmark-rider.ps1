param(
    [ValidateRange(1,300)][int]$Seconds=122,
    [ValidateRange(1,60)][int]$Speed=60,
    [switch]$Visible
)
$ErrorActionPreference='Stop'
$root=Split-Path $PSScriptRoot -Parent
$stamp=Get-Date -Format 'yyyyMMdd-HHmmss'
$output=Join-Path $root "local/benchmark-$stamp"
New-Item -ItemType Directory -Path $output | Out-Null
$style=if ($Visible) { 'Normal' } else { 'Hidden' }
$p=Start-Process (Join-Path $root 'dist/OpenRide.exe') -ArgumentList @('--','--verify-content-library',"--benchmark-seconds=$Seconds","--benchmark-speed=$Speed") -PassThru -WindowStyle $style -RedirectStandardOutput "$output/result.log" -RedirectStandardError "$output/errors.log"
$null=$p.Handle
$working=0L
$private=0L
$deadline=(Get-Date).AddSeconds($Seconds+30)
while (-not $p.HasExited -and (Get-Date) -lt $deadline) {
    $p.Refresh()
    $working=[Math]::Max($working,$p.WorkingSet64)
    $private=[Math]::Max($private,$p.PrivateMemorySize64)
    Start-Sleep -Milliseconds 500
}
if (-not $p.HasExited) { $p.Kill(); throw 'Benchmark timed out' }
$p.Refresh()
[PSCustomObject]@{PeakWorkingSetBytes=$working;PeakPrivateBytes=$private;ExitCode=$p.ExitCode} | ConvertTo-Json | Set-Content "$output/memory.json"
Get-Content "$output/result.log"
Get-Content "$output/errors.log"
Write-Output "Results: $output"
if ($p.ExitCode -ne 0) { throw 'Benchmark failed' }
