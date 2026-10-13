# Dot-source this script to configure only the current PowerShell process:
# . ./build/Use-Environment.ps1
param([string]$Config = (Join-Path $PSScriptRoot 'environment.local.psd1'))

$openrideOverrides = @{}
if (Test-Path -LiteralPath $Config) {
    $openrideOverrides = Import-PowerShellDataFile -LiteralPath $Config -ErrorAction Stop
}
$openrideNames = @{ Python = 'python'; Godot = 'godot'; PowerShell = 'powershell'; Bash = 'bash' }
foreach ($openrideKey in $openrideOverrides.Keys) {
    if (-not $openrideNames.ContainsKey($openrideKey)) { throw "Unknown tool in ${Config}: $openrideKey" }
}
# Resolve every entry before changing the session, so invalid overrides fail cleanly.
$openrideResolved = @{}
foreach ($openrideKey in $openrideNames.Keys) {
    $openrideCommand = $openrideNames[$openrideKey]
    $openrideOverride = $openrideOverrides[$openrideKey]
    if ($null -ne $openrideOverride -and $openrideOverride -isnot [string]) {
        throw "$openrideKey override must be an executable path or an empty string"
    }
    if ($openrideOverride) {
        $openrideApplication = Get-Command $openrideOverride -CommandType Application -ErrorAction Stop
    } else {
        $openrideApplication = Get-Command $openrideCommand -CommandType Application -All -ErrorAction SilentlyContinue |
            Where-Object { $_.Source -notmatch '[\\/]WindowsApps[\\/]|[\\/]Thonny[\\/]|[\\/]KiCad[\\/]' } |
            Select-Object -First 1
    }
    if ($openrideApplication) { $openrideResolved[$openrideKey] = $openrideApplication.Source }
}
foreach ($openrideKey in $openrideResolved.Keys) {
    $openrideExecutable = $openrideResolved[$openrideKey]
    Set-Item -LiteralPath "Env:OPENRIDE_$($openrideKey.ToUpperInvariant())" -Value $openrideExecutable
    Set-Alias -Name $openrideNames[$openrideKey] -Value $openrideExecutable -Scope Global
}
$openrideDirectories = @($openrideResolved.Values | ForEach-Object { Split-Path -Parent $_ } | Select-Object -Unique)
$env:PATH = (($openrideDirectories + @($env:PATH -split [IO.Path]::PathSeparator) | Select-Object -Unique) -join [IO.Path]::PathSeparator)
# No machine/user environment variables, shell profiles or execution policies change.
