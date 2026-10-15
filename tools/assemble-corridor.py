"""Build one continuous streamed package from verified regional terrain sources.

Roads and terrain are rebuilt in a common frame, never concatenated from packages.
Default: all regions. --regions explicitly builds a prefix for incremental validation.
"""
import argparse
from collections import OrderedDict
from functools import lru_cache
import importlib.util
import json
import math
from pathlib import Path
import subprocess
import sys
import uuid

sys.path.insert(0, str(Path(__file__).parent))
spec = importlib.util.spec_from_file_location('corridor', Path(__file__).with_name('corridor-scenery.py'))
corridor = importlib.util.module_from_spec(spec)
spec.loader.exec_module(corridor)
tiled = corridor.tiled
chunks = tiled.module('chunk-scenery')
R = 6371000.0
GRID = 8
CELL = 256
BUFFER = 150


class Frame:
    def __init__(self, origin):
        self.origin = origin
        self.lat, self.lon = map(math.radians, origin[:2])
        self.sin, self.cos = math.sin(self.lat), math.cos(self.lat)

    def project(self, lat, lon):
        lat, dl = math.radians(lat), math.radians(lon)-self.lon
        if self.sin*math.sin(lat)+self.cos*math.cos(lat)*math.cos(dl)<=0:
            raise ValueError('Coordinates lie behind the bounded projection hemisphere')
        return R*math.cos(lat)*math.sin(dl), -R*(self.cos*math.sin(lat)-self.sin*math.cos(lat)*math.cos(dl))

    def geographic(self, x, z):
        radius = math.hypot(x,z)
        if radius > 100000: raise ValueError('Assembly exceeds 100 km projection radius')
        if radius == 0: return self.origin[:2]
        c = math.asin(radius/R)
        lat = math.asin(math.cos(c)*self.sin-z*math.sin(c)*self.cos/radius)
        lon = self.lon+math.atan2(x*math.sin(c),radius*self.cos*math.cos(c)+z*self.sin*math.sin(c))
        return math.degrees(lat), math.degrees(lon)


def route(source, end):
    raw = source['points']
    if source.get('issues') or source.get('unresolved_breaks') or not 0 < end <= raw[-1]['distance_m'] <= 200000:
        raise ValueError('Assembly requires a continuous reviewed route up to 200 km')
    origin = [(min(p['latitude'] for p in raw)+max(p['latitude'] for p in raw))/2,
              (min(p['longitude'] for p in raw)+max(p['longitude'] for p in raw))/2, raw[0]['elevation_m']]
    frame = Frame(origin)
    selected = [dict(p) for p in raw if p['distance_m'] <= end]
    if selected[-1]['distance_m'] < end:
        a, b = selected[-1], raw[len(selected)]
        f = (end-a['distance_m'])/(b['distance_m']-a['distance_m'])
        p = dict(a)
        for key in ('latitude','longitude','elevation_m'): p[key] = a[key]+f*(b[key]-a[key])
        p['distance_m'] = end
        selected.append(p)
    if not 2 <= len(selected) <= 10000: raise ValueError('Assembly exceeds 10,000 canonical route points')
    points = []
    for p in selected:
        x,z = frame.project(p['latitude'],p['longitude'])
        if math.hypot(x,z) > 99000: raise ValueError('Route exceeds bounded assembly projection')
        q = {k:p[k] for k in ('distance_m','latitude','longitude','elevation_m','source_sample')}
        q.update(position=[x,p['elevation_m']-origin[2],z],break_before=not points,grade_percent=None)
        if points:
            step = q['distance_m']-points[-1]['distance_m']
            if step <= 0 or p['break_before']: raise ValueError('Non-continuous route')
            q['grade_percent'] = 100*(q['elevation_m']-points[-1]['elevation_m'])/step
            if abs(q['grade_percent']) > 100: raise ValueError('Unresolved raw grade')
        points.append(q)
    return frame, dict(points=points,mode='gps',validated=False,
        projection=dict(kind='spherical_orthographic',radius_m=R,origin=origin))


def dense_road(points):
    result = [points[0]]
    for a,b in zip(points,points[1:]):
        count = max(1,math.ceil((b['distance_m']-a['distance_m'])/10))
        if len(result)+count > 32768: raise ValueError('Road mesh exceeds 32,768 station budget')
        for i in range(1,count+1):
            f = i/count
            result.append(dict(position=[x+f*(y-x) for x,y in zip(a['position'],b['position'])]))
    return result


class Terrain:
    """Verified source catalogue; at most eight decoded grids resident."""
    def __init__(self, project, regions, frame, builder, scratch, reconcile_overlaps=False):
        self.frame, self.builder, self.scratch = frame, builder, scratch
        self.tiles, self.index, self.cache, self.provenance = [], {}, OrderedDict(), []
        self.max_overlap_m = 0.
        self.reconcile_overlaps = reconcile_overlaps
        self.reconciled_samples = 0
        self.max_reconciliation_shift_m = 0.
        for region in regions:
            path = project/region['id']
            corridor.check_receipt(path,region,'terrain')
            plan = tiled.load_project(path)
            for tile in plan['tiles']:
                directory = path/tile['id']
                manifest = corridor.verified_sources(directory/'sources',directory/'preparation.json','terrain')
                origin = plan['preparation']['origin_lat_lon_elevation']
                key = len(self.tiles)
                bounds = tile['bounds']
                self.tiles.append((directory,origin,bounds))
                self.provenance.append(dict(region=region['id'],tile=tile['id'],source_snapshot=manifest))
                corners = [frame.project(lat,lon) for lat in (bounds['south'],bounds['north']) for lon in (bounds['west'],bounds['east'])]
                for ix in range(math.floor((min(p[0] for p in corners)-10)/1000),math.floor((max(p[0] for p in corners)+10)/1000)+1):
                    for iz in range(math.floor((min(p[1] for p in corners)-10)/1000),math.floor((max(p[1] for p in corners)+10)/1000)+1):
                        self.index.setdefault((ix,iz),[]).append(key)

    def grid(self,key):
        if key in self.cache:
            self.cache.move_to_end(key)
            return self.cache[key]
        directory,origin,bounds = self.tiles[key]
        decoded = self.scratch/f'dem-{key:04d}'
        if not decoded.exists():
            subprocess.run([str(self.builder),'--terrain',str(directory/'preparation.json'),str(directory/'sources'),str(decoded)],check=True,timeout=60,stdout=subprocess.DEVNULL)
        terrain = json.loads(tiled.read(decoded/'preparation.json'))['terrain']
        if terrain['nodata_pixels']: raise ValueError('Terrain has missing elevation data')
        w,h = terrain['width'],terrain['height']
        v = terrain['mesh']['vertices']
        if w<2 or h<2 or len(v)!=w*h: raise ValueError('Invalid terrain grid')
        result = (w,h,v[0][0],v[0][2],v[1][0]-v[0][0],v[w][2]-v[0][2],[p[1]+origin[2]-self.frame.origin[2] for p in v])
        self.cache[key] = result
        if len(self.cache)>8: self.cache.popitem(last=False)
        return result

    def __call__(self,x,z):
        lat,lon = self.frame.geographic(x,z)
        values,weights,ranges = [],[],[]
        for key in self.index.get((math.floor(x/1000),math.floor(z/1000)),[]):
            directory,origin,b = self.tiles[key]
            if not b['south']<=lat<=b['north'] or not b['west']<=lon<=b['east']: continue
            w,h,x0,z0,dx,dz,v = self.grid(key)
            lx = R*math.radians(lon-origin[1])*math.cos(math.radians(origin[0]))
            lz = -R*math.radians(lat-origin[0])
            col,row = (lx-x0)/dx,(lz-z0)/dz
            if not 0<=col<=w-1 or not 0<=row<=h-1: continue
            ix,iz = min(w-2,int(col)),min(h-2,int(row))
            fx,fz = col-ix,row-iz
            i = iz*w+ix
            values.append((v[i]*(1-fx)+v[i+1]*fx)*(1-fz)+(v[i+w]*(1-fx)+v[i+w+1]*fx)*fz)
            weights.append(min(col,row,w-1-col,h-1-row,8)**2)
            quad=[v[i],v[i+1],v[i+w],v[i+w+1]]
            ranges.append(max(quad)-min(quad))
        if not values or any(not math.isfinite(y) for y in values): raise ValueError('Missing common-grid terrain coverage')
        spread = max(values)-min(values)
        self.max_overlap_m = max(self.max_overlap_m,spread)
        allowance=max(2.,min(10.,max(ranges))) if self.reconcile_overlaps else 2.
        if spread > allowance:
            raise ValueError(f'Cross-region terrain disagreement {spread:.3f} m exceeds {allowance:.3f} m at ({x}, {z}); local cell ranges={ranges}')
        if self.reconcile_overlaps and sum(weights)>0:
            blended=sum(y*w for y,w in zip(values,weights))/sum(weights)
            self.reconciled_samples+=int(spread>2)
            self.max_reconciliation_shift_m=max(self.max_reconciliation_shift_m,abs(blended-values[0]))
            return blended
        return values[0]  # Stable region/tile ordering gives every grid vertex one owner.


def normal(x,y,z):
    length=math.sqrt(x*x+y*y+z*z)
    return [x/length,y/length,z/length] if length else [0.,1.,0.]


def road_ribbon(points):
    """A reversal has two end caps at the same station, not a singular miter.

    Keep both route occurrences and their heights; only render topology changes.
    """
    result=dict(vertices=[],indices=[])
    cuts=[0]
    for i in range(1,len(points)-1):
        a,b,c=[p['position'] for p in points[i-1:i+2]]
        u,v=(b[0]-a[0],b[2]-a[2]),(c[0]-b[0],c[2]-b[2])
        lu,lv=math.hypot(*u),math.hypot(*v)
        if lu==0 or lv==0: raise ValueError('Zero-length projected road segment')
        if math.hypot(u[0]/lu+v[0]/lv,u[1]/lu+v[1]/lv)<0.001: cuts.append(i)
    cuts.append(len(points)-1)
    for first,last in zip(cuts,cuts[1:]):
        mesh=chunks.riding_road(points[first:last+1])
        offset=len(result['vertices'])
        result['vertices'].extend(mesh['vertices'])
        result['indices'].extend(v+offset for v in mesh['indices'])
    return result


def write_geometry(profile, sample, output):
    road_points = dense_road(profile['points'])
    road = road_ribbon(road_points)
    normals = [[0.,0.,0.] for _ in road['vertices']]
    for i in range(0,len(road['indices']),3):
        ids=road['indices'][i:i+3]
        a,b,c=[road['vertices'][v] for v in ids]
        u,v=[b[j]-a[j] for j in range(3)],[c[j]-a[j] for j in range(3)]
        n=[u[1]*v[2]-u[2]*v[1],u[2]*v[0]-u[0]*v[2],u[0]*v[1]-u[1]*v[0]]
        if n[1]<0: n=[-q for q in n]
        for k in ids:
            normals[k]=[a+b for a,b in zip(normals[k],n)]
    normals=[normal(*n) for n in normals]
    roads,segments,cells={},{},set()
    for i in range(0,len(road['indices']),3):
        ids=road['indices'][i:i+3]
        vertices=[road['vertices'][v] for v in ids]
        if max(math.dist(a,b) for a in vertices for b in vertices)>32: raise ValueError('Road triangle exceeds 32 m overhang')
        key=tuple(math.floor(sum(v[k] for v in vertices)/3/CELL) for k in (0,2))
        roads.setdefault(key,[]).append(ids)
        cells.add(key)
    for a,b in zip(road_points,road_points[1:]):
        a,b=a['position'],b['position']
        for ix in range(math.floor((min(a[0],b[0])-BUFFER-GRID*2)/CELL),math.floor((max(a[0],b[0])+BUFFER+GRID*2)/CELL)+1):
            for iz in range(math.floor((min(a[2],b[2])-BUFFER-GRID*2)/CELL),math.floor((max(a[2],b[2])+BUFFER+GRID*2)/CELL)+1):
                segments.setdefault((ix,iz),[]).append((a,b))
                cells.add((ix,iz))
    if len(cells)>4096: raise ValueError('Assembly exceeds 4096 chunks')

    def closest(x,z):
        best=(float('inf'),0.)
        for a,b in segments.get((math.floor(x/CELL),math.floor(z/CELL)),[]):
            dx,dz=b[0]-a[0],b[2]-a[2]
            f=max(0,min(1,((x-a[0])*dx+(z-a[2])*dz)/(dx*dx+dz*dz)))
            distance=math.hypot(x-a[0]-f*dx,z-a[2]-f*dz)
            if distance<best[0]: best=(distance,a[1]+f*(b[1]-a[1])-.25)
        return best

    @lru_cache(maxsize=16384)
    def height(ix,iz):
        x,z=ix*GRID,iz*GRID
        y=sample(x,z)
        distance,road_height=closest(x,z)
        weight=max(0,min(1,(30-distance)/12))
        weight=weight*weight*(3-2*weight)
        return y+(road_height-y)*weight

    entries=[]
    for number,(cx,cz) in enumerate(sorted(cells)):
        terrain=dict(vertices=[],indices=[],normals=[])
        lookup={}
        def vertex(ix,iz):
            if (ix,iz) not in lookup:
                lookup[ix,iz]=len(terrain['vertices'])
                terrain['vertices'].append([ix*GRID,height(ix,iz),iz*GRID])
                terrain['normals'].append(normal(height(ix-1,iz)-height(ix+1,iz),2*GRID,height(ix,iz-1)-height(ix,iz+1)))
            return lookup[ix,iz]
        for iz in range(cz*32,(cz+1)*32):
            for ix in range(cx*32,(cx+1)*32):
                if closest((ix+.5)*GRID,(iz+.5)*GRID)[0]>BUFFER: continue
                a,b,c,d=vertex(ix,iz),vertex(ix+1,iz),vertex(ix,iz+1),vertex(ix+1,iz+1)
                terrain['indices'].extend([a,b,c,b,d,c])
        ribbon=dict(vertices=[],indices=[],normals=[])
        mapping={}
        for triangle in roads.get((cx,cz),[]):
            for v in triangle:
                if v not in mapping:
                    mapping[v]=len(ribbon['vertices'])
                    ribbon['vertices'].append(road['vertices'][v])
                    ribbon['normals'].append(normals[v])
                ribbon['indices'].append(mapping[v])
        if not terrain['indices'] and not ribbon['indices']: continue
        for mesh in (terrain,ribbon):
            if len(mesh['vertices'])>30000 or len(mesh['indices'])>90000: raise ValueError('Chunk mesh budget exceeded')
            if any(abs(v)>100000 or not math.isfinite(v) for p in mesh['vertices'] for v in p): raise ValueError('Chunk coordinate budget exceeded')
        payload=tiled.encoded(dict(terrain=terrain,road=ribbon,lines=[],vegetation=[]))
        if len(payload)>2*1024*1024: raise ValueError('Chunk exceeds 2 MiB')
        name=f'chunk_{cx}_{cz}.json'
        tiled.write_new(output/name,payload)
        entries.append(dict(x=cx,z=cz,path=name,bytes=len(payload),sha256=tiled.sha(payload)))
        if number%50==0: print(f'Assembled cell {number+1}/{len(cells)}',flush=True)
    return entries


def assemble(project,destination,builder,region_count=None,accept_steep=False,reconcile_overlaps=False):
    project,destination=Path(project),Path(destination)
    if destination.exists(): raise FileExistsError('Choose a new package destination')
    dependencies=[Path(builder),Path(__file__),Path(__file__).with_name('tiled-scenery.py'),Path(__file__).with_name('chunk-scenery.py')]
    recipe={str(p.resolve()):corridor.digest_file(p) for p in dependencies}
    job=corridor.load(project)
    source=corridor.document(project/'reviewed-profile.json')
    count=len(job['regions']) if region_count is None else region_count
    if not 1<=count<=len(job['regions']): raise ValueError('Invalid prefix region count')
    regions=job['regions'][:count]
    if any(r.get('blocked') for r in regions): raise ValueError('Selected regions contain unresolved blockers')
    end=regions[-1]['end_m']
    if any(f['start_m']<end for f in job['grade_findings']) and not accept_steep:
        raise ValueError('Review retained grades; --accept-steep retains them unchanged in this preview')
    frame,profile=route(source,end)
    route_raw=tiled.encoded(profile)
    if len(route_raw)>8*1024*1024: raise ValueError('Canonical route exceeds 8 MiB runtime budget')
    staging_root=destination.parent/'.openride-assembly'
    staging_root.mkdir(exist_ok=True)
    workspace=staging_root/(destination.name+'-'+uuid.uuid4().hex)
    workspace.mkdir()
    output=workspace/'package';output.mkdir()
    terrain=Terrain(project,regions,frame,Path(builder).resolve(),workspace,reconcile_overlaps)
    entries=write_geometry(profile,terrain,output)
    credits=sorted(set(str(v) for t in terrain.provenance for v in t['source_snapshot'].get('attribution',{}).values()))
    provenance=dict(kind='openride.terrain-provenance',schema_version=1,review_revision=job['review_revision'],
        source_profile_sha256=job['profile_sha256'],regions=[r['id'] for r in regions],tiles=terrain.provenance,
        assembly=dict(grid_m=GRID,coverage_m=[0,end],full_route=end==job['length_m'],max_overlap_difference_m=terrain.max_overlap_m,
            overlap_policy='edge-weighted feather; disagreements bounded by local DEM-cell variation and 10 m cap' if reconcile_overlaps else 'stable source order, 2 m disagreement cap',
            reconciled_sample_calls=terrain.reconciled_samples,max_reconciliation_shift_m=terrain.max_reconciliation_shift_m,
            terrain_ownership='One global lattice, stable source order, spatial-cell triangle ownership',
            terrain_conformance='Presentation earthwork only; no bridge/tunnel/stacked-road reconciliation',
            steep_segments_retained=accept_steep,builder_sha256=corridor.digest_file(builder),assembler_sha256=corridor.digest_file(__file__)))
    raw=tiled.encoded(provenance)
    if len(raw)>2*1024*1024: raise ValueError('Provenance exceeds 2 MiB')
    tiled.write_new(output/'route.json',route_raw)
    tiled.write_new(output/'terrain-provenance.json',raw)
    manifest=dict(kind='openride.stream-preview',schema_version=1,trainer_ready=False,
        name=source.get('name','Reviewed route')+f' / continuous 0–{end/1000:.1f} km',
        tile_m=CELL,neighborhood_radius_cells=1,max_live_chunks=9,triangle_overhang_bound_m=32,
        source_preparation_sha256=job['profile_sha256'],review_status='unvalidated-preview',
        route=dict(path='route.json',bytes=len(route_raw),sha256=tiled.sha(route_raw)),chunks=entries,
        terrain_provenance=dict(path='terrain-provenance.json',bytes=len(raw),sha256=tiled.sha(raw)),
        origin_lat_lon_elevation=frame.origin,attribution={'terrain':'; '.join(credits)},
        presentation=dict(terrain_corridor=True,inner_m=18,outer_m=30,clearance_m=.25),
        retained_metadata='Canonical route and spatial chunk index; DEM LRU=8 and height cache=16384 during assembly')
    raw=tiled.encoded(manifest)
    if len(raw)>1024*1024: raise ValueError('Manifest exceeds 1 MiB')
    tiled.write_new(output/'manifest.json',raw)
    # Recheck immutable inputs before publishing; concurrent acquisition may only add other regions.
    corridor.load(project)
    for region in regions: corridor.check_receipt(project/region['id'],region,'terrain')
    if recipe!={str(p.resolve()):corridor.digest_file(p) for p in dependencies}:
        raise ValueError('Assembly tools changed during build; staged output not published')
    if destination.exists(): raise FileExistsError('Publication destination already exists')
    output.rename(destination)
    return dict(package=str(destination),regions=count,length_m=end,chunks=len(entries),route_points=len(profile['points']),max_overlap_m=terrain.max_overlap_m)


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('project');parser.add_argument('destination');parser.add_argument('--builder',required=True)
    parser.add_argument('--regions',type=int);parser.add_argument('--accept-steep',action='store_true');parser.add_argument('--reconcile-overlaps',action='store_true')
    args=parser.parse_args()
    try: print(json.dumps(assemble(args.project,args.destination,args.builder,args.regions,args.accept_steep,args.reconcile_overlaps),indent=2))
    except (Exception,KeyboardInterrupt) as error:
        print(str(error) or 'Interrupted; staged work retained',file=sys.stderr);sys.exit(1)
