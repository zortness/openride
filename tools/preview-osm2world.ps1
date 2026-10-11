param(
    [string]$Model = 'local/osm2world-rpv-verified/rpv.glb',
    [string]$Godot = 'godot',
    [double]$Zoom = 3
)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
if (-not [IO.Path]::IsPathRooted($Model)) { $Model = Join-Path $projectRoot $Model }
$Model = (Resolve-Path -LiteralPath $Model).Path
& $Godot --path (Join-Path $projectRoot 'apps/scenery-preview') res://model.tscn -- "--model=$Model" "--zoom=$Zoom"
