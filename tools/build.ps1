param([switch]$Release, [switch]$Offline, [string]$Godot = $(if ($env:OPENRIDE_GODOT) { $env:OPENRIDE_GODOT } else { 'godot' }))
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location $projectRoot
try {
    & ./tools/sync-scenery-runtime.ps1
    $cargoArgs = @('build', '-p', 'openride-godot', '-p', 'device-probe', '-p', 'content-builder', '-p', 'openride-author', '--locked')
    if ($Offline) { $cargoArgs += '--offline' }
    $profile = 'debug'
    if ($Release) { $cargoArgs += '--release'; $profile = 'release' }
    & cargo @cargoArgs
    if ($LASTEXITCODE -ne 0) { throw 'Rust extension and native route/tool build failed' }
    New-Item -ItemType Directory -Force -Path apps/rider/bin | Out-Null
    Copy-Item -LiteralPath "target/$profile/openride_godot.dll" -Destination apps/rider/bin/openride_godot.dll
    Copy-Item -LiteralPath "target/$profile/device-probe.exe" -Destination apps/rider/bin/device-probe.exe
    Copy-Item -LiteralPath "target/$profile/content-builder.exe" -Destination apps/rider/bin/content-builder.exe
    Copy-Item -LiteralPath "target/$profile/openride-author.exe" -Destination apps/rider/bin/openride-author.exe
    $versionMatch = Select-String -Path apps/rider/project.godot -Pattern '^config/version="([^"]+)"$'
    if (-not $versionMatch) { throw 'Missing OpenRide application version' }
    $baseVersion = $versionMatch.Matches[0].Groups[1].Value
    $revision = (& git rev-parse --short=8 HEAD).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'Cannot read source revision for build metadata' }
    $dirty = @(& git status --porcelain --untracked-files=normal).Count -gt 0
    if ($LASTEXITCODE -ne 0) { throw 'Cannot read source state for build metadata' }
    $buildInfo = @{
        version = "$baseVersion+$revision$(if ($dirty) { '.dev' })"
        built_utc = (Get-Date).ToUniversalTime().ToString('yyyy-MM-dd HH:mm "UTC"', [Globalization.CultureInfo]::InvariantCulture)
    } | ConvertTo-Json -Compress
    [IO.File]::WriteAllText((Join-Path $projectRoot 'apps/rider/bin/build-info.json'), $buildInfo, [Text.UTF8Encoding]::new($false))
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
