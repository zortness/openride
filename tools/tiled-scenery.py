"""Bounded M7 multi-tile section proof; unchanged recordings, shared terrain lattice."""
import argparse
import bisect
import copy
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import subprocess
import sys
import uuid

EARTH=6371000.0
TILE=1000
GRID=10
BUFFER=150
HALO=40

def module(name):
    spec=importlib.util.spec_from_file_location(name,Path(__file__).with_name(name+'.py'))
    result=importlib.util.module_from_spec(spec);spec.loader.exec_module(result);return result

def encoded(value):return json.dumps(value,separators=(',',':'),allow_nan=False).encode()
def sha(raw):return hashlib.sha256(raw).hexdigest()
def read(path,limit=64*1024*1024):
    with Path(path).open('rb') as stream:raw=stream.read(limit+1)
    if len(raw)>limit:raise ValueError('Input exceeds bounded section budget')
    return raw
def write_new(path,raw):
    with Path(path).open('xb') as stream:stream.write(raw)

def plan(source,start,end):
    if source.get('kind')!='openride.reviewed-profile' or source.get('schema_version')!=1 or source.get('trainer_ready') is not False:raise ValueError('Expected a reviewed profile snapshot')
    raw=source['points']
    if not 2<=len(raw)<=100000:raise ValueError('Invalid source point count')
    for p in raw:
        if any(isinstance(p[k],bool) or not isinstance(p[k],(int,float)) or not math.isfinite(p[k]) for k in ('distance_m','latitude','longitude','elevation_m')):raise ValueError('Non-finite route sample')
    if any(b['distance_m']<a['distance_m'] for a,b in zip(raw,raw[1:])):raise ValueError('Unordered route distance')
    distances=[p['distance_m'] for p in raw]
    if not all(math.isfinite(v) for v in (start,end)) or not 0<=start<end<=distances[-1] or end-start>12000:raise ValueError('Select a section up to 12 km within the recording')
    def at(d):
        i=max(0,bisect.bisect_right(distances,d)-1);a=raw[i]
        if a['distance_m']==d:return copy.deepcopy(a)
        b=raw[i+1]
        if b['break_before']:raise ValueError('Section boundary is in an unresolved break')
        f=(d-a['distance_m'])/(b['distance_m']-a['distance_m'])
        p=copy.deepcopy(a)
        for k in ('latitude','longitude','elevation_m'):p[k]=a[k]+f*(b[k]-a[k])
        p.update(distance_m=d,break_before=False,source_fraction=f)
        return p
    selected=[at(start)]+[copy.deepcopy(p) for p in raw if start<p['distance_m']<end]+[at(end)]
    if any(p['break_before'] for p in selected[1:]):raise ValueError('Selected section contains unresolved breaks; repair or select a continuous interval')
    origin=[selected[0]['latitude'],selected[0]['longitude'],selected[0]['elevation_m']]
    if abs(origin[0])>=80:raise ValueError('Section latitude must be below 80 degrees')
    points=[];steep=[]
    def append(p):
        lat,lon,height=p['latitude'],p['longitude'],p['elevation_m']
        if abs(lat)>=80 or abs(lon-origin[1])>1:raise ValueError('Section exceeds local coordinate budget')
        xyz=[EARTH*math.radians(lon-origin[1])*math.cos(math.radians(origin[0])),height-origin[2],-EARTH*math.radians(lat-origin[0])]
        result=dict(p,position=xyz,source_distance_m=p['distance_m'],distance_m=p['distance_m']-start,break_before=not points,grade_percent=None)
        if points:
            prev=points[-1];step=result['distance_m']-prev['distance_m']
            horizontal=math.hypot(xyz[0]-prev['position'][0],xyz[2]-prev['position'][2])
            if step<=0 or abs(horizontal-step)>0.01:raise ValueError('Section projection exceeds current runtime stationing tolerance')
            result['grade_percent']=100*(height-prev['elevation_m'])/step
            if abs(result['grade_percent'])>100:raise ValueError('Raw grade exceeds simulation validity; repair source elevations first')
            if abs(result['grade_percent'])>15:steep.append(dict(distance_m=result['distance_m'],grade_percent=result['grade_percent'],source_sample=p['source_sample']))
        points.append(result)
    append(selected[0])
    for a,b in zip(selected,selected[1:]):
        count=max(1,math.ceil((b['distance_m']-a['distance_m'])/10))
        if len(points)+count>10000:raise ValueError('Section exceeds current runtime point budget')
        for j in range(1,count+1):
            f=j/count;p=copy.deepcopy(b if j==count else a)
            for k in ('latitude','longitude','elevation_m','distance_m'):p[k]=a[k]+f*(b[k]-a[k])
            p['source_fraction']=f;append(p)
    cells=set()
    for p in points:
        x,_,z=p['position']
        for ix in range(math.floor((x-BUFFER)/TILE),math.floor((x+BUFFER)/TILE)+1):
            for iz in range(math.floor((z-BUFFER)/TILE),math.floor((z+BUFFER)/TILE)+1):cells.add((ix,iz))
    if len(cells)>40:raise ValueError('Section exceeds 40 source tiles')
    def geographic(x,z):return origin[0]-math.degrees(z/EARTH),origin[1]+math.degrees(x/EARTH)/math.cos(math.radians(origin[0]))
    tiles=[]
    for ix,iz in sorted(cells):
        north,west=geographic(ix*TILE-HALO,iz*TILE-HALO)
        south,east=geographic((ix+1)*TILE+HALO,(iz+1)*TILE+HALO)
        tiles.append(dict(id=f'tile_{ix}_{iz}',x=ix,z=iz,bounds=dict(south=south,west=west,north=north,east=east)))
    bounds=dict(south=min(t['bounds']['south'] for t in tiles),west=min(t['bounds']['west'] for t in tiles),north=max(t['bounds']['north'] for t in tiles),east=max(t['bounds']['east'] for t in tiles))
    preparation=dict(kind='openride.scenery-preparation',schema_version=1,trainer_ready=False,name=source.get('name','Reviewed section'),source_route_sha256=sha(encoded(source)),
        source_activity_sha256=source.get('geometry_source'),review_revision=source['review_revision'],requested_distance_m=[start,end],origin_lat_lon_elevation=origin,acquisition_bounds=bounds,
        route_profile=dict(mode='gps',points=points,findings=[]),points=points,files=[],elevation_provenance='Unmodified reviewed activity heights; no per-tile filtering')
    return dict(kind='openride.tiled-section-plan',schema_version=1,preparation=preparation,tiles=tiles,steep_segments=steep,grid_m=GRID,tile_m=TILE,buffer_m=BUFFER,halo_m=HALO)

def create_project(review,destination,builder,start,end):
    destination=Path(destination);destination.mkdir()
    snapshot=destination/'reviewed-profile.json'
    subprocess.run([str(builder),'--review-profile',str(review),str(snapshot)],check=True,timeout=60)
    result=plan(json.loads(read(snapshot)),start,end)
    write_plan(destination,result)
    return result

def write_plan(destination,result):
    """Materialize one already validated bounded plan (also used by corridor jobs)."""
    destination=Path(destination)
    for tile in result['tiles']:
        directory=destination/tile['id'];directory.mkdir()
        p=copy.deepcopy(result['preparation']);p['points']=[];p.pop('route_profile');p['acquisition_bounds']=tile['bounds']
        assets={'road-preview.obj':b'# Terrain acquisition tile\n','bounds.geojson':b'{}','osm-query.overpass':b''}
        p['files']=[dict(path=name,sha256=sha(data)) for name,data in assets.items()]
        for name,data in assets.items():write_new(directory/name,data)
        payload=encoded(p);write_new(directory/'preparation.json',payload);tile['preparation_sha256']=sha(payload)
    write_new(destination/'plan.json',encoded(result))

def load_project(project):
    project=Path(project);p=json.loads(read(project/'plan.json'))
    if p.get('kind')!='openride.tiled-section-plan' or p.get('schema_version')!=1:raise ValueError('Unsupported tiled plan')
    expected=plan(json.loads(read(project/'reviewed-profile.json')),*p['preparation']['requested_distance_m'])
    clean=copy.deepcopy(p)
    for tile in clean['tiles']:tile.pop('preparation_sha256',None)
    if clean!=expected:raise ValueError('Plan no longer matches reviewed profile')
    for tile in p['tiles']:
        raw=read(project/tile['id']/'preparation.json')
        if sha(raw)!=tile['preparation_sha256']:raise ValueError('Tile preparation changed')
    return p

def acquire(project):
    p=load_project(project);acquisition=module('acquire-scenery')
    for i,tile in enumerate(p['tiles']):
        print(f"Terrain tile {i+1}/{len(p['tiles'])}: {tile['id']}",flush=True)
        directory=Path(project)/tile['id']
        result=acquisition.acquire(directory/'preparation.json',directory/'sources',stage='terrain')
        if result['errors'].get('terrain') or 'terrain.tif' not in result['files']:raise ValueError(str(result['errors']))

def mosaic(p,grids):
    """One globally indexed lattice; a shared edge is sampled and stored only once."""
    if set(grids)!=set(t['id'] for t in p['tiles']):raise ValueError('Missing terrain tile')
    rasters={}
    for tile in p['tiles']:
        grid=grids[tile['id']]
        if grid['nodata_pixels']:raise ValueError('Terrain tile contains missing elevation; coverage must be repaired')
        w,h=grid['width'],grid['height'];v=grid['mesh']['vertices']
        if not 2<=w<=256 or not 2<=h<=256 or len(v)!=w*h:raise ValueError('Invalid decoded grid')
        dx=v[1][0]-v[0][0];dz=v[w][2]-v[0][2]
        if dx<=0 or dz<=0:raise ValueError('Invalid north-up terrain lattice')
        rasters[tile['x'],tile['z']]=(w,h,v,dx,dz)
    segments={}
    for a,b in zip(p['preparation']['route_profile']['points'],p['preparation']['route_profile']['points'][1:]):
        a,b=a['position'],b['position']
        for x in range(math.floor((min(a[0],b[0])-BUFFER-GRID)/128),math.floor((max(a[0],b[0])+BUFFER+GRID)/128)+1):
            for z in range(math.floor((min(a[2],b[2])-BUFFER-GRID)/128),math.floor((max(a[2],b[2])+BUFFER+GRID)/128)+1):segments.setdefault((x,z),[]).append((a,b))
    def near(x,z):
        for a,b in segments.get((math.floor(x/128),math.floor(z/128)),[]):
            dx,dz=b[0]-a[0],b[2]-a[2];length=dx*dx+dz*dz
            f=max(0,min(1,((x-a[0])*dx+(z-a[2])*dz)/length)) if length else 0
            if math.hypot(x-a[0]-f*dx,z-a[2]-f*dz)<=BUFFER+GRID:return True
        return False
    def sample(raster,x,z):
        w,h,v,dx,dz=raster;col=(x-v[0][0])/dx;row=(z-v[0][2])/dz
        if not 0<=col<=w-1 or not 0<=row<=h-1:return None
        ix=min(w-2,int(col));iz=min(h-2,int(row));fx,fz=col-ix,row-iz;i=iz*w+ix
        return (v[i][1]*(1-fx)+v[i+1][1]*fx)*(1-fz)+(v[i+w][1]*(1-fx)+v[i+w+1][1]*fx)*fz
    vertices=[];indices=[];lookup={};shared=0;difference=0
    def vertex(ix,iz):
        nonlocal shared,difference
        key=ix,iz
        if key in lookup:return lookup[key]
        x,z=ix*GRID,iz*GRID;cx,cz=math.floor(x/TILE),math.floor(z/TILE)
        values=[]
        for tx in range(cx-1,cx+2):
            for tz in range(cz-1,cz+2):
                raster=rasters.get((tx,tz))
                if raster is not None:
                    y=sample(raster,x,z)
                    if y is not None:values.append(y)
        if not values or any(not math.isfinite(v) for v in values):raise ValueError('Incomplete terrain coverage at shared grid vertex')
        spread=max(values)-min(values);difference=max(difference,spread)
        if spread>2:raise ValueError(f'Terrain overlap differs by {spread:.3f} m; source reconciliation required')
        if len(values)>1 and (ix%100==0 or iz%100==0):shared+=1
        if len(vertices)>=100000:raise ValueError('Section exceeds 100,000 terrain vertices')
        lookup[key]=len(vertices);vertices.append([x,values[0],z]);return lookup[key]
    for tile in p['tiles']:
        for iz in range(tile['z']*100,(tile['z']+1)*100):
            for ix in range(tile['x']*100,(tile['x']+1)*100):
                if not near((ix+0.5)*GRID,(iz+0.5)*GRID):continue
                a,b,c,d=vertex(ix,iz),vertex(ix+1,iz),vertex(ix,iz+1),vertex(ix+1,iz+1)
                indices.extend([a,b,c,b,d,c])
    if not indices:raise ValueError('No terrain corridor generated')
    return dict(vertices=vertices,indices=indices),dict(shared_boundary_vertices=shared,max_overlap_difference_m=difference,terrain_vertices=len(vertices),terrain_triangles=len(indices)//3)

def build(project,destination,builder,accept_steep=False):
    project=Path(project);destination=Path(destination)
    if destination.exists():raise FileExistsError('Choose a new package directory')
    p=load_project(project)
    if p['steep_segments'] and not accept_steep:raise ValueError('Review steep_segments in plan.json; --accept-steep explicitly retains those grades without flattening')
    workspace=project/('build-'+uuid.uuid4().hex);workspace.mkdir()
    grids={};provenance=[]
    for i,tile in enumerate(p['tiles']):
        print(f"Decode terrain {i+1}/{len(p['tiles'])}: {tile['id']}",flush=True)
        directory=project/tile['id'];sources=directory/'sources';manifest=json.loads(read(sources/'sources.json'))
        if manifest['preparation_sha256']!=tile['preparation_sha256']:raise ValueError('Source cache belongs to a different tile')
        export=json.loads(read(sources/'usgs-export.json'));bounds=tile['bounds']
        if any(abs(export['extent'][key]-bounds[name])>1e-7 for key,name in [('xmin','west'),('ymin','south'),('xmax','east'),('ymax','north')]):raise ValueError('Source extent does not match planned tile')
        decoded=workspace/tile['id']
        subprocess.run([str(builder),'--terrain',str(directory/'preparation.json'),str(sources),str(decoded)],check=True,timeout=60,stdout=subprocess.DEVNULL)
        terrain=json.loads(read(decoded/'preparation.json'))['terrain']
        grids[tile['id']]=dict(width=terrain['width'],height=terrain['height'],nodata_pixels=terrain['nodata_pixels'],mesh=dict(vertices=terrain['mesh']['vertices']))
        provenance.append(dict(tile=tile['id'],source_snapshot=terrain['source_snapshot']))
    mesh,report=mosaic(p,grids)
    preparation=copy.deepcopy(p['preparation']);chunks=module('chunk-scenery')
    preparation['route_profile']['mesh']=chunks.riding_road(preparation['route_profile']['points'])
    credits=sorted(set(str(v) for entry in provenance for v in entry['source_snapshot']['attribution'].values()))
    preparation['terrain']=dict(mesh=mesh,source_snapshot=dict(attribution={'terrain':'; '.join(credits),'provenance':'Per-tile snapshots and vertical-reference metadata: terrain-provenance.json'}))
    preparation['tiled_build']=dict(report,tiles=len(p['tiles']),grid_m=GRID,steep_segments_accepted=bool(p['steep_segments'] and accept_steep),vertical_reference='Unreconciled; recorded road heights unchanged; terrain is bare earth',terrain_conformed=True,conformance_notice='Existing presentation-only corridor earthwork; not bridge/tunnel or stacked-road reconciliation')
    raw=encoded(preparation)
    if len(raw)>16*1024*1024:raise ValueError('Section exceeds current preparation format budget')
    prepared=workspace/'preparation.json';write_new(prepared,raw)
    # Road geometry is continuous and built once, never independently smoothed per terrain tile.
    staging=workspace/'package'
    result=chunks.build(prepared,staging,riding_surface=True)
    # Finish all payloads in a private staging directory before publishing by rename.
    provenance_raw=encoded(dict(kind='openride.terrain-provenance',schema_version=1,tiles=provenance,review_revision=preparation['review_revision'],report=preparation['tiled_build']))
    if len(provenance_raw)>2*1024*1024:raise ValueError('Provenance exceeds package payload budget')
    write_new(staging/'terrain-provenance.json',provenance_raw)
    manifest=json.loads(read(staging/'manifest.json'))
    start,end=p['preparation']['requested_distance_m']
    manifest['name']+=f' / {start/1000:.1f}-{end/1000:.1f} km tiled section'
    manifest['terrain_provenance']=dict(path='terrain-provenance.json',bytes=len(provenance_raw),sha256=sha(provenance_raw))
    temporary=staging/'manifest.json.tmp';write_new(temporary,encoded(manifest));temporary.replace(staging/'manifest.json')
    if destination.exists():raise FileExistsError('Publication destination already exists')
    staging.rename(destination)
    result['bytes']=sum(path.stat().st_size for path in destination.iterdir() if path.is_file())
    print(json.dumps(dict(result,**report,tiles=len(p['tiles'])),indent=2))
    return result

def main():
    parser=argparse.ArgumentParser(description=__doc__);sub=parser.add_subparsers(dest='action',required=True)
    create=sub.add_parser('plan');create.add_argument('review');create.add_argument('destination');create.add_argument('--builder',required=True);create.add_argument('--start',type=float,required=True);create.add_argument('--end',type=float,required=True)
    get=sub.add_parser('acquire');get.add_argument('project')
    bake=sub.add_parser('build');bake.add_argument('project');bake.add_argument('destination');bake.add_argument('--builder',required=True);bake.add_argument('--accept-steep',action='store_true')
    args=parser.parse_args()
    if args.action=='plan':
        result=create_project(args.review,args.destination,args.builder,args.start,args.end)
        print(json.dumps(dict(tiles=len(result['tiles']),steep_segments=len(result['steep_segments']),remaining_full_route_issues=json.loads(read(Path(args.destination)/'reviewed-profile.json'))['issues']),indent=2))
    elif args.action=='acquire':acquire(args.project)
    elif args.action=='build':build(args.project,args.destination,args.builder,args.accept_steep)

if __name__=='__main__':
    try:main()
    except Exception as error:print(error,file=sys.stderr);sys.exit(1)
