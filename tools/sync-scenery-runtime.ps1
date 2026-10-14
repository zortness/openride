param()
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$destination = Join-Path $root 'apps/rider/scenery'
New-Item -ItemType Directory -Force -Path $destination | Out-Null
foreach ($name in @('ride_hud.gd','route_detail.gd','ground_detail.gdshader','coarse_terrain.gdshader','hr_settings.gd','live_ride.gd','stream_preview.gd','stream_preview.tscn','route_traversal.gd','chunk_worker.gd','coastal_backdrop.gdshader')) {
    $source = Join-Path $root "apps/scenery-preview/$name"
    $text = [IO.File]::ReadAllText($source)
    foreach ($resource in @('ride_hud.gd','route_detail.gd','ground_detail.gdshader','coarse_terrain.gdshader','hr_settings.gd','live_ride.gd','route_traversal.gd','chunk_worker.gd','coastal_backdrop.gdshader','stream_preview.gd')) {
        $text = $text.Replace("res://$resource", "res://scenery/$resource")
    }
    [IO.File]::WriteAllText((Join-Path $destination $name), $text)
}
$editor = Join-Path $root 'apps/rider/editor'
New-Item -ItemType Directory -Force -Path $editor | Out-Null
$files = @('main.gd','route_traversal.gd','jitter_review.gd','manual_repair.gd','package_library.gd','road_propagation.gd')
foreach ($name in $files) {
    $text = [IO.File]::ReadAllText((Join-Path $root "apps/scenery-preview/$name"))
    foreach ($resource in $files) { $text = $text.Replace("res://$resource", "res://editor/$resource") }
    [IO.File]::WriteAllText((Join-Path $editor $name), $text)
}
