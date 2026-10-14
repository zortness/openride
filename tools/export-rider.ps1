param([switch]$Offline, [string]$Godot = $(if ($env:OPENRIDE_GODOT) { $env:OPENRIDE_GODOT } else { 'godot' }))
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Push-Location $root
try {
    # Always package freshly built release tools, including the native route CLI.
    & ./tools/build.ps1 -Release -Offline:$Offline -Godot $Godot
    foreach ($component in @('openride_godot.dll', 'device-probe.exe', 'content-builder.exe', 'openride-author.exe')) {
        if (-not (Test-Path -LiteralPath "apps/rider/bin/$component" -PathType Leaf)) {
            throw "Missing native component after build: $component"
        }
    }
    New-Item -ItemType Directory -Force -Path dist,local | Out-Null
    $process=Start-Process $Godot -ArgumentList @('--headless','--path','apps/rider','--export-release','"Windows Desktop"',"`"$root/dist/OpenRide.exe`"") -WindowStyle Hidden -PassThru -RedirectStandardOutput local/export-rider.log -RedirectStandardError local/export-rider.err
    $null=$process.Handle
    if (-not $process.WaitForExit(120000)) {
        $process.Kill()
        throw 'Windows export timed out; see local/export-rider.*'
    }
    $process.Refresh()
    Get-Content local/export-rider.err
    if ($process.ExitCode -ne 0) { throw 'Windows export failed' }
    if (Select-String -Path local/export-rider.log,local/export-rider.err -Pattern '^(ERROR:|SCRIPT ERROR:)' -Quiet) { throw 'Godot reported export errors' }
    Copy-Item -LiteralPath apps/rider/bin/device-probe.exe -Destination dist/device-probe.exe
    New-Item -ItemType Directory -Force -Path dist/authoring | Out-Null
    Copy-Item -LiteralPath apps/rider/bin/content-builder.exe -Destination dist/authoring/content-builder.exe
    Copy-Item -LiteralPath apps/rider/bin/openride-author.exe -Destination dist/authoring/openride-author.exe
    Get-ChildItem -LiteralPath tools -Filter '*.py' | Copy-Item -Destination dist/authoring
} finally { Pop-Location }
