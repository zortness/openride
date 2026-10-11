param([string]$Godot = 'godot')
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Push-Location $root
try {
    & ./tools/sync-scenery-runtime.ps1
    if (-not (Test-Path apps/rider/bin/openride_godot.dll)) { throw 'Run tools/build.ps1 -Release first.' }
    New-Item -ItemType Directory -Force -Path dist,local | Out-Null
    $process=Start-Process $Godot -ArgumentList @('--headless','--path','apps/rider','--export-release','"Windows Desktop"',"`"$root/dist/OpenRide.exe`"") -WindowStyle Hidden -PassThru -RedirectStandardOutput local/export-rider.log -RedirectStandardError local/export-rider.err
    $null=$process.Handle
    $process.WaitForExit()
    $process.Refresh()
    Get-Content local/export-rider.err
    if ($process.ExitCode -ne 0) { throw 'Windows export failed' }
    Copy-Item -LiteralPath apps/rider/bin/device-probe.exe -Destination dist/device-probe.exe
    New-Item -ItemType Directory -Force -Path dist/authoring | Out-Null
    Copy-Item -LiteralPath apps/rider/bin/content-builder.exe -Destination dist/authoring/content-builder.exe
    Get-ChildItem -LiteralPath tools -Filter '*.py' | Copy-Item -Destination dist/authoring
} finally { Pop-Location }
