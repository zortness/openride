param(
    [ValidateSet('adapter','hr','cadence','speed','speed-cadence')][string]$Profile='adapter',
    [ValidateRange(1,120)][int]$Seconds=20,
    [ValidateRange(0,65535)][int]$DeviceNumber=0,
    [string]$KeyFile='local/ant-network-key.txt'
)
$ErrorActionPreference='Stop'
$root=Split-Path $PSScriptRoot -Parent
$previousKey=$env:OPENRIDE_ANT_NETWORK_KEY
Push-Location $root
try {
    if ($Profile -ne 'adapter' -and !$env:OPENRIDE_ANT_NETWORK_KEY) {
        if (!(Test-Path -LiteralPath $KeyFile)) {
            throw 'Set OPENRIDE_ANT_NETWORK_KEY or put the 16 hexadecimal digits from Garmin ANT+ network-key documentation in local/ant-network-key.txt.'
        }
        $env:OPENRIDE_ANT_NETWORK_KEY=(Get-Content -LiteralPath $KeyFile -Raw).Trim()
    }
    & cargo build -p device-probe --locked --offline
    if ($LASTEXITCODE -ne 0) {throw 'ANT probe build failed'}
    $capture=Join-Path $root ('local/ant-probe-'+(Get-Date -Format 'yyyyMMdd-HHmmss')+'-'+[guid]::NewGuid().ToString('N').Substring(0,8))
    New-Item -ItemType Directory -Path $capture | Out-Null
    $probeArgs=@('--ant-journal="'+$capture+'/ant.jsonl"',"--seconds=$Seconds","--device-number=$DeviceNumber")
    if ($Profile -ne 'adapter') {$probeArgs+="--ant-profile=$Profile"}
    Write-Output "ANT check: $Profile. Close other ANT applications. Capture: $capture"
    $process=Start-Process ./target/debug/device-probe.exe -ArgumentList $probeArgs -PassThru -WindowStyle Hidden -RedirectStandardOutput "$capture/stdout.jsonl" -RedirectStandardError "$capture/errors.txt"
    $null=$process.Handle
    if (!$process.WaitForExit(185000)) {$process.Kill();throw 'ANT watchdog expired; adapter cleanup unconfirmed'}
    $process.Refresh()
    Get-Content -LiteralPath "$capture/errors.txt"
    Get-Content -LiteralPath "$capture/ant.jsonl" | ForEach-Object {
        $row=$_ | ConvertFrom-Json
        if ($row.event -in @('ant_adapter_ready','ant_sensor_identity','ant_capture_summary','ant_probe_finished','ant_probe_failed')) {Write-Output $_}
    }
    Write-Output "Capture saved to: $capture"
    if ($process.ExitCode -ne 0) {throw "ANT probe failed ($($process.ExitCode)); inspect capture"}
} finally {
    $env:OPENRIDE_ANT_NETWORK_KEY=$previousKey
    Pop-Location
}
