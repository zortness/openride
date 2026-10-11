param(
    [string]$Package = 'local/content/rpv-stream-preview.orpack',
    [string]$Godot = 'godot'
)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
if (-not [IO.Path]::IsPathRooted($Package)) { $Package = Join-Path $projectRoot $Package }
$Package = (Resolve-Path -LiteralPath $Package).Path
& $Godot --path (Join-Path $projectRoot 'apps/scenery-preview') res://stream_preview.tscn -- "--stream-package=$Package"
