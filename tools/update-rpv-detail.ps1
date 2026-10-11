param(
    [string]$Python='python',
    [switch]$AcquireImagery,
    [ValidateRange(1,600)][int]$ImageryTimeoutSeconds=120,
    [ValidateRange(1,5)][int]$ImageryAttempts=3,
    [ValidateSet(512,1024,2048)][int]$ImagerySize=2048,
    [string]$ImageryCache=''
)
$ErrorActionPreference='Stop'
$root=Split-Path $PSScriptRoot -Parent
Push-Location $root
try {
    $stamp=Get-Date -Format 'yyyyMMdd-HHmmss'
    $name="RPV-detail-$stamp.orpack"
    $destination=Join-Path $root "dist/content/$name"
    $arguments=@('tools/chunk-scenery.py','export/rpv-m3-road-route/preparation.json',$destination,'--riding-surface','--coastline-osm','local/rpv-sources/map.osm','--buildings-osm','local/rpv-sources/map.osm','--detail-osm','local/rpv-sources/map.osm')
    if($AcquireImagery){
        $snapshot=if($ImageryCache){$ImageryCache}else{"local/rpv-imagery-tiles-$ImagerySize"}
        $grid=[int]($ImagerySize/512)
        & $Python tools/acquire-imagery.py export/rpv-m3-road-route/preparation.json $snapshot --timeout $ImageryTimeoutSeconds --attempts $ImageryAttempts --grid $grid
        if($LASTEXITCODE -ne 0){throw "Imagery download failed; details are in $snapshot. Retry the same command to resume completed tiles. No package was installed."}
        $arguments+=@('--imagery',"$snapshot/imagery.json")
    }
    & $Python @arguments
    if($LASTEXITCODE -ne 0){throw 'Detailed scenery build failed'}
    Write-Output "Installed $destination. Refresh Offline routes and select the detailed RPV package."
} finally {Pop-Location}
