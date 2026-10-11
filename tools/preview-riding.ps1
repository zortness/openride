param(
    [string]$Package = '',
    [string]$Godot = 'godot'
)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
if ([string]::IsNullOrEmpty($Package)) {
    $latest = Get-ChildItem -LiteralPath (Join-Path $projectRoot 'dist/content') -Directory | Where-Object { $_.Name -like 'RPV-detail-*.orpack' } | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    $Package = if ($latest) { $latest.FullName } else { 'local/content/rpv-buildings-v2.orpack' }
}
if (-not [IO.Path]::IsPathRooted($Package)) { $Package = Join-Path $projectRoot $Package }
$Package = (Resolve-Path -LiteralPath $Package).Path
& $Godot --path (Join-Path $projectRoot 'apps/scenery-preview') res://stream_preview.tscn -- "--stream-package=$Package" --riding-view

