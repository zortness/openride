param([switch]$Release, [switch]$Offline, [string]$Godot = $(if ($env:OPENRIDE_GODOT) { $env:OPENRIDE_GODOT } else { 'godot' }))
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location $projectRoot
try {
    & ./tools/sync-scenery-runtime.ps1
    $cargoArgs = @('build', '-p', 'openride-godot', '-p', 'device-probe', '-p', 'content-builder', '--locked')
    if ($Offline) { $cargoArgs += '--offline' }
    $profile = 'debug'
    if ($Release) { $cargoArgs += '--release'; $profile = 'release' }
    & cargo @cargoArgs
    if ($LASTEXITCODE -ne 0) { throw 'Rust extension build failed' }
    New-Item -ItemType Directory -Force -Path apps/rider/bin | Out-Null
    Copy-Item -LiteralPath "target/$profile/openride_godot.dll" -Destination apps/rider/bin/openride_godot.dll
    Copy-Item -LiteralPath "target/$profile/device-probe.exe" -Destination apps/rider/bin/device-probe.exe
    Copy-Item -LiteralPath "target/$profile/content-builder.exe" -Destination apps/rider/bin/content-builder.exe
    New-Item -ItemType Directory -Force -Path local | Out-Null
    $godotExecutable = (Get-Command $Godot -ErrorAction Stop).Source
    $process = Start-Process -FilePath $godotExecutable -ArgumentList @('--headless', '--path', 'apps/rider', '--editor', '--import') -WorkingDirectory $projectRoot -WindowStyle Hidden -PassThru -RedirectStandardOutput "$projectRoot/local/build-godot.stdout.log" -RedirectStandardError "$projectRoot/local/build-godot.stderr.log"
    $null = $process.Handle
    if (-not $process.WaitForExit(120000)) {
        $process.Kill()
        throw 'Godot import timed out; see local/build-godot.*.log'
    }
    $process.Refresh()
    Get-Content local/build-godot.stdout.log
    Get-Content local/build-godot.stderr.log
    if ($process.ExitCode -ne 0) { throw "Godot import failed: $($process.ExitCode)" }
    # Godot can report resource/script errors while returning exit code zero.
    if (Select-String -Path local/build-godot.stdout.log,local/build-godot.stderr.log -Pattern '^(ERROR:|SCRIPT ERROR:)' -Quiet) {
        throw 'Godot reported import errors; see local/build-godot.*.log'
    }
} finally { Pop-Location }
