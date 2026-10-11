param(
    [string]$Preparation = 'export/rpv-m3-preview/preparation.json',
    [string]$Godot = 'godot',
    [string]$JitterReview = '',
    [string]$Package = '',
    [string]$ContentDirectory = ''
)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
$previewArguments = @()
if ($Package) {
    if (-not [IO.Path]::IsPathRooted($Package)) { $Package = Join-Path $projectRoot $Package }
    $previewArguments += "--package=$((Resolve-Path -LiteralPath $Package).Path)"
} else {
    if (-not [IO.Path]::IsPathRooted($Preparation)) { $Preparation = Join-Path $projectRoot $Preparation }
    $Preparation = (Resolve-Path -LiteralPath $Preparation).Path
    $previewArguments += "--preparation=$Preparation"
}
if ($ContentDirectory) {
    if (-not [IO.Path]::IsPathRooted($ContentDirectory)) { $ContentDirectory = Join-Path $projectRoot $ContentDirectory }
    $previewArguments += "--content-directory=$((Resolve-Path -LiteralPath $ContentDirectory).Path)"
}
if ($JitterReview) {
    if (-not [IO.Path]::IsPathRooted($JitterReview)) { $JitterReview = Join-Path $projectRoot $JitterReview }
    $previewArguments += "--jitter-review=$((Resolve-Path -LiteralPath $JitterReview).Path)"
}
& $Godot --path (Join-Path $projectRoot 'apps/scenery-preview') -- @previewArguments
