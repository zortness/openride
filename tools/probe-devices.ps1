param([string]$DeviceName = 'SUITO', [string]$Address, [switch]$AcquireControl, [switch]$SessionCheck, [switch]$ResistanceCheck, [switch]$PedalCheck)
$ErrorActionPreference = 'Stop'
function Show-ProbeProgress([string]$Path, [ref]$Cursor) {
    if (!(Test-Path -LiteralPath $Path)) { return }
    $raw = Get-Content -LiteralPath $Path -Raw
    if (!$raw) { return }
    $lines = $raw -split "`n"
    # Only consume complete lines; redirected output may end mid-record.
    while ($Cursor.Value -lt $lines.Length - 1) {
        $line = $lines[$Cursor.Value]
        $Cursor.Value++
        if ([string]::IsNullOrWhiteSpace($line)) { continue }
        try { $record = $line | ConvertFrom-Json } catch { continue }
        switch ($record.event) {
            'session_check_ready' { Write-Output "Connected: $($record.mode). Acquiring control..." }
            'grade_step_acknowledged' { Write-Output "Grade $($record.grade_percent)% acknowledged -- hold $($record.hold_seconds) seconds." }
            'command_acknowledged' {
                if ($record.opcode -eq 7) { Write-Output 'Start acknowledged.' }
                if ($record.opcode -eq 8) { Write-Output 'Pause acknowledged. You can stop pedaling.' }
            }
            'session_check_complete' { Write-Output "Check finished: $($record.reason)." }
            'session_check_failed' { Write-Output "Check failed: $($record.message). Trainer state: $($record.trainer_state)." }
            'probe_error' { Write-Output $line }
        }
    }
}
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location $projectRoot
try {
    if (([int]$AcquireControl.IsPresent + [int]$SessionCheck.IsPresent + [int]$ResistanceCheck.IsPresent + [int]$PedalCheck.IsPresent) -gt 1) { throw 'Choose one diagnostic mode' }
    $capture = Join-Path $projectRoot ('local/device-probe-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N').Substring(0,8))
    New-Item -ItemType Directory -Path $capture | Out-Null
    $devices = (pnputil /enum-devices /connected /deviceids) -join "`n"
    $devices -split '\r?\n\r?\n' | Where-Object { $_ -match 'SUITO|VID_0FCF|VID_091E|Bluetooth Adapter' } | Set-Content -Encoding UTF8 -LiteralPath "$capture/windows-devices.txt"
    & cargo build -p device-probe --locked --offline
    if ($LASTEXITCODE -ne 0) { throw 'Device probe build failed' }
    if ($DeviceName.Contains('"')) { throw 'Device name cannot contain quotes' }
    $probeArgs = @('"' + $DeviceName + '"')
    if ($Address) {
        if ($Address -notmatch '^[0-9a-fA-F:]{12,17}$') { throw 'Invalid Bluetooth address' }
        $probeArgs = @('--address', $Address)
    }
    if ($AcquireControl) { $probeArgs += '--acquire-control' }
    if ($SessionCheck) { $probeArgs += ('"--session-check=' + "$capture/session.jsonl" + '"') }
    if ($ResistanceCheck) { $probeArgs += ('"--resistance-check=' + "$capture/session.jsonl" + '"') }
    if ($PedalCheck) { $probeArgs += ('"--pedal-check=' + "$capture/session.jsonl" + '"') }
    Write-Output "Capture directory: $capture"
    if ($PedalCheck) { Write-Output 'Pedal easily in one gear. After discovery: 0%, 1%, 2%, 1%, 0%, ten seconds each.' }
    $process = Start-Process -FilePath "$projectRoot/target/debug/device-probe.exe" -ArgumentList $probeArgs -WindowStyle Hidden -PassThru -RedirectStandardOutput "$capture/ble.jsonl" -RedirectStandardError "$capture/ble-errors.txt"
    $null = $process.Handle
    $cursor = 0
    while (!$process.HasExited) {
        Show-ProbeProgress "$capture/ble.jsonl" ([ref]$cursor)
        Start-Sleep -Milliseconds 250
    }
    $process.WaitForExit()
    $process.Refresh()
    Show-ProbeProgress "$capture/ble.jsonl" ([ref]$cursor)
    Get-Content -LiteralPath "$capture/ble.jsonl"
    Get-Content -LiteralPath "$capture/ble-errors.txt"
    Write-Output "Capture saved to: $capture"
    if ($process.ExitCode -ne 0) { throw "BLE probe exited $($process.ExitCode). Capture includes the failure stage." }
} finally { Pop-Location }
