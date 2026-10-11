"""Bounded optional USGS NAIP preview snapshot; never used by the riding runtime."""
import argparse
import datetime
import hashlib
import json
import math
from pathlib import Path
import struct
import sys
import time
import http.client
import urllib.error
import urllib.parse
import urllib.request

SERVICE='https://imagery.nationalmap.gov/arcgis/rest/services/USGSNAIPPlus/ImageServer'

def fetch(url, timeout=120, attempts=3):
    request=urllib.request.Request(url,headers={'User-Agent':'OpenRide-authoring/0.1'})
    for attempt in range(1,attempts+1):
        print(f'  Request {attempt}/{attempts}, socket timeout {timeout}s',flush=True)
        try:
            with urllib.request.urlopen(request,timeout=timeout) as response:raw=response.read(16*1024*1024+1)
            if len(raw)>16*1024*1024:raise ValueError('Imagery response exceeds limit')
            return raw
        except (TimeoutError,urllib.error.URLError,http.client.IncompleteRead) as error:
            if isinstance(error,urllib.error.HTTPError):
                error.close()
                if error.code not in (408,429,500,502,503,504):raise
            if attempt==attempts:raise
            delay=2**attempt
            print(f'  Temporary request failure: {error}. Retrying in {delay}s...',flush=True)
            time.sleep(delay)

def acquire(preparation,destination,timeout=120,attempts=3,size=2048):
    if not 1<=timeout<=600 or not 1<=attempts<=5 or size not in (512,1024,2048):raise ValueError('Invalid imagery request limits')
    p=json.loads(Path(preparation).read_bytes());e=p['terrain']['extent'];origin=p['origin_lat_lon_elevation']
    b=[e[k] for k in ('xmin','ymin','xmax','ymax')]
    if not all(math.isfinite(v) for v in b) or not (-180<=b[0]<b[2]<=180 and -90<=b[1]<b[3]<=90) or b[2]-b[0]>.1 or b[3]-b[1]>.1:raise ValueError('Expected small route imagery extent')
    output=Path(destination);output.mkdir()
    def download(stage,url):
        print(f'{stage}: {url}',flush=True)
        try:return fetch(url,timeout,attempts)
        except Exception as error:
            (output/'failure.json').write_text(json.dumps(dict(stage=stage,url=url,error=str(error),timeout_seconds=timeout,attempts=attempts),indent=2))
            raise RuntimeError(f'{stage} failed: {error}. Details: {output / "failure.json"}') from error
    service=download('Read service metadata',SERVICE+'?f=json');(output/'service.json').write_bytes(service)
    service_info=json.loads(service)
    if 'error' in service_info:raise ValueError(str(service_info['error']))
    params=dict(f='json',bbox=','.join(map(str,b)),bboxSR=4326,imageSR=4326,size=f'{size},{size}',format='png',adjustAspectRatio='false',interpolation='RSP_BilinearInterpolation')
    response=download(f'Generate {size} x {size} geographic image',SERVICE+'/exportImage?'+urllib.parse.urlencode(params));(output/'response.json').write_bytes(response)
    info=json.loads(response)
    if 'error' in info:raise ValueError(str(info['error']))
    actual=info['extent']
    if any(abs(actual[k]-e[k])>1e-7 for k in ('xmin','xmax','ymin','ymax')):raise ValueError('Imagery extent changed')
    sr=actual['spatialReference']
    if sr.get('latestWkid',sr.get('wkid'))!=4326:raise ValueError('Unexpected imagery CRS')
    url=urllib.parse.urlsplit(info['href'])
    if url.scheme!='https' or url.hostname!='imagery.nationalmap.gov' or url.username or url.password:raise ValueError('Unexpected imagery host')
    raw=download('Download generated PNG',info['href'])
    if len(raw)<24 or not raw.startswith(b'\x89PNG\r\n\x1a\n') or struct.unpack('>II',raw[16:24])!=(size,size):raise ValueError('Unexpected imagery format/dimensions')
    (output/'imagery.png').write_bytes(raw)
    def x(lon):return 6371000*math.radians(lon-origin[1])*math.cos(math.radians(origin[0]))
    def z(lat):return -6371000*math.radians(lat-origin[0])
    metadata=dict(schema=1,sha256=hashlib.sha256(raw).hexdigest(),bounds=e,origin_lat_lon_elevation=origin,
        local_bounds=[x(b[0]),z(b[3]),x(b[2])-x(b[0]),z(b[1])-z(b[3])],
        attribution='USGS National Map / NAIP public-domain aerial imagery',source=SERVICE,
        acquired_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
        acquisition_date='Mosaic service; individual source dates not resolved. Acquisition timestamp is download time, not photography date.',request=params)
    (output/'imagery.json').write_text(json.dumps(metadata,indent=2))
    print(output/'imagery.json')

def acquire_tiled(preparation,destination,timeout=120,attempts=3,grid=4):
    """Resume a source-bound row-major cache; completed files are hash verified."""
    import png_tiles
    p=json.loads(Path(preparation).read_bytes());e=p['terrain']['extent'];origin=p['origin_lat_lon_elevation']
    if grid not in (1,2,4) or not 1<=timeout<=600 or not 1<=attempts<=5:raise ValueError('Invalid grid or request limits')
    b=[e[k] for k in ('xmin','ymin','xmax','ymax')]
    if not all(math.isfinite(v) for v in b) or not (-180<=b[0]<b[2]<=180 and -80<=b[1]<b[3]<=80) or b[2]-b[0]>.1 or b[3]-b[1]>.1:raise ValueError('Expected bounded local imagery extent')
    output=Path(destination);output.mkdir(exist_ok=True)
    plan=dict(schema=1,source=SERVICE,bounds=e,origin_lat_lon_elevation=origin,grid=grid,tile_size=512)
    plan_path=output/'tile-plan.json'
    if plan_path.exists():
        if json.loads(plan_path.read_bytes())!=plan:raise ValueError('Imagery cache belongs to different bounds/origin/grid')
    else:
        if any(output.iterdir()):raise ValueError('Not an imagery tile cache')
        plan_path.write_text(json.dumps(plan,indent=2))
    tiles=[];sources=[]
    for row in range(grid):
        for col in range(grid):
            tile=output/f'tile-{row}-{col}';tile.mkdir(exist_ok=True)
            print(f'Tile {row*grid+col+1}/{grid*grid} (north-to-south row {row+1}, column {col+1})',flush=True)
            west=b[0]+(b[2]-b[0])*col/grid;east=b[0]+(b[2]-b[0])*(col+1)/grid
            north=b[3]-(b[3]-b[1])*row/grid;south=b[3]-(b[3]-b[1])*(row+1)/grid
            extent=dict(e,xmin=west,xmax=east,ymin=south,ymax=north)
            tile_preparation=dict(p,terrain=dict(p['terrain'],extent=extent))
            source=tile/'preparation.json';source.write_text(json.dumps(tile_preparation))
            cached=None
            # Each network attempt is isolated; only a fully verified image is committed.
            if (tile/'complete.json').exists():
                cached=json.loads((tile/'complete.json').read_bytes());raw=(tile/'imagery.png').read_bytes()
                if cached.get('bounds')!=extent or hashlib.sha256(raw).hexdigest()!=cached.get('sha256'):raise ValueError('Cached imagery checksum/bounds conflict')
                if png_tiles.decode(raw)[:2]!=(512,512):raise ValueError('Cached tile dimensions')
                print('  Using verified cached tile',flush=True)
            else:
                import uuid
                job=tile/('attempt-'+uuid.uuid4().hex[:8])
                acquire(source,job,timeout,attempts,512)
                raw=(job/'imagery.png').read_bytes();png_tiles.decode(raw)
                cached=json.loads((job/'imagery.json').read_bytes())
                (tile/'imagery.png').write_bytes(raw)
                (tile/'complete.json').write_text(json.dumps(cached,indent=2))
            tiles.append(raw);sources.append(cached)
    image=png_tiles.assemble(tiles,grid)
    def x(lon):return 6371000*math.radians(lon-origin[1])*math.cos(math.radians(origin[0]))
    def z(lat):return -6371000*math.radians(lat-origin[0])
    info=dict(plan,sha256=hashlib.sha256(image).hexdigest(),local_bounds=[x(b[0]),z(b[3]),x(b[2])-x(b[0]),z(b[1])-z(b[3])],
              acquired_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),tiles=sources,
              attribution='USGS National Map / NAIP public-domain aerial imagery',
              acquisition_date='Per-tile download times retained; photography dates unresolved',request={'size':f'{grid*512},{grid*512}'})
    # The final metadata is the completion marker; a failed run retains only its tiles.
    (output/'imagery.png.tmp').write_bytes(image);(output/'imagery.png.tmp').replace(output/'imagery.png')
    (output/'imagery.json.tmp').write_text(json.dumps(info,indent=2));(output/'imagery.json.tmp').replace(output/'imagery.json')
    print(f'IMAGERY_COMPLETE {output / "imagery.json"}',flush=True)
    return info


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('preparation');parser.add_argument('new_directory')
    parser.add_argument('--grid',type=int,choices=[1,2,4]);parser.add_argument('--timeout',type=int,default=120);parser.add_argument('--attempts',type=int,default=3);parser.add_argument('--size',type=int,choices=[512,1024,2048],default=2048);args=parser.parse_args()
    try:
        if args.grid:acquire_tiled(args.preparation,args.new_directory,args.timeout,args.attempts,args.grid)
        else:acquire(args.preparation,args.new_directory,args.timeout,args.attempts,args.size)
    except Exception as error:print(f"Imagery acquisition failed: {error}",file=sys.stderr);sys.exit(1)
