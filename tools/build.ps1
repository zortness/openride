param([switch]$Release, [switch]$Offline)
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
    $godot = (Get-Command godot -ErrorAction Stop).Source
    $process = Start-Process -FilePath $godot -ArgumentList @('--headless', '--path', 'apps/rider', '--editor', '--import', '--quit') -WorkingDirectory $projectRoot -WindowStyle Hidden -Wait -PassThru -RedirectStandardOutput "$projectRoot/local/build-godot.stdout.log" -RedirectStandardError "$projectRoot/local/build-godot.stderr.log"
    Get-Content local/build-godot.stdout.log
    Get-Content local/build-godot.stderr.log
    if ($process.ExitCode -ne 0) { throw "Godot import failed: $($process.ExitCode)" }
} finally { Pop-Location }
