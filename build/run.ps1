# One-command wrapper for a fresh shell, with arbitrary arguments passed unchanged.
param(
    [Parameter(Mandatory=$true, Position=0)]
    [ValidateSet('python','godot','powershell','bash')][string]$Tool,
    [Parameter(Position=1, ValueFromRemainingArguments=$true)][string[]]$ToolArguments
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'Use-Environment.ps1')
$openrideExecutable = [Environment]::GetEnvironmentVariable('OPENRIDE_' + $Tool.ToUpperInvariant())
if (-not $openrideExecutable) { throw "Tool '$Tool' not found; configure build/environment.local.psd1" }
& $openrideExecutable @ToolArguments
exit $LASTEXITCODE
