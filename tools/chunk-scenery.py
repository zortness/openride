"""Bake road/terrain preview geometry into 256 m spatial cells; route metadata stays resident."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import sys


def riding_road(points, widths=None):
    """Shared edge vertices prevent cracks between individual segment ribbons."""
    positions=[p['position'] for p in points]
    normals=[]
    for a,b in zip(positions,positions[1:]):
        dx=b[0]-a[0];dz=b[2]-a[2];length=math.hypot(dx,dz)
        if length==0: raise ValueError('Zero-length road segment')
        normals.append((-dz/length,dx/length))
    vertices=[];indices=[]
    for i,(x,y,z) in enumerate(positions):
        before=normals[max(0,i-1)];after=normals[min(i,len(normals)-1)]
        nx=before[0]+after[0];nz=before[1]+after[1];length=math.hypot(nx,nz)
        if length<0.001: raise ValueError('Road reverses at a ribbon join')
        nx/=length;nz/=length
        scale=(widths[i]/2 if widths else 3)/max(0.5,nx*after[0]+nz*after[1])
        vertices.extend([[x+nx*scale,y,z+nz*scale],[x-nx*scale,y,z-nz*scale]])
        if i: indices.extend([2*i-2,2*i-1,2*i,2*i-1,2*i+1,2*i])
    return dict(vertices=vertices,indices=indices)


def conform_terrain(mesh, points):
    """Presentation-only earthwork: retain route heights, blend a 18?30 m corridor.

    The wide inner band accommodates the coarse DEM triangles. This is not
    surveyed terrain, bridge handling, or a new source of route elevation.
    """
    segments={}
    for a,b in zip(points,points[1:]):
        a=a['position'];b=b['position']
        for x in range(math.floor((min(a[0],b[0])-30)/64),math.floor((max(a[0],b[0])+30)/64)+1):
            for z in range(math.floor((min(a[2],b[2])-30)/64),math.floor((max(a[2],b[2])+30)/64)+1):
                segments.setdefault((x,z),[]).append((a,b))
    vertices=[];changed=0
    for x,y,z in mesh['vertices']:
        nearest=30.0; height=y
        for a,b in segments.get((math.floor(x/64),math.floor(z/64)),[]):
            dx=b[0]-a[0];dz=b[2]-a[2];length=dx*dx+dz*dz
            if length==0: continue
            t=max(0,min(1,((x-a[0])*dx+(z-a[2])*dz)/length))
            distance=math.hypot(x-a[0]-t*dx,z-a[2]-t*dz)
            if distance<nearest:
                nearest=distance; height=a[1]+t*(b[1]-a[1])-0.25
        if nearest<30:
            weight=max(0,min(1,(30-nearest)/12))
            weight=weight*weight*(3-2*weight)
            y=y+(height-y)*weight;changed+=1
        vertices.append([x,y,z])
    return dict(vertices=vertices,indices=mesh['indices']),changed


def bounded_terrain(mesh):
    """Subdivide coarse DEM faces without moving the sampled surface or colours."""
    result=dict(vertices=list(mesh['vertices']),indices=[])
    if 'colors' in mesh:result['colors']=list(mesh['colors'])
    midpoints={}
    for i in range(0,len(mesh['indices']),3):
        pending=[mesh['indices'][i:i+3]]
        while pending:
            ids=pending.pop()
            length,edge=max((math.dist(result['vertices'][ids[j]],result['vertices'][ids[(j+1)%3]]),j) for j in range(3))
            if length>128:raise ValueError('Terrain triangle exceeds bounded subdivision / chunk-overhang budget')
            if length<=32:result['indices'].extend(ids);continue
            a,b,c=ids[edge],ids[(edge+1)%3],ids[(edge+2)%3];key=tuple(sorted((a,b)))
            if key not in midpoints:
                midpoints[key]=len(result['vertices'])
                result['vertices'].append([(x+y)/2 for x,y in zip(result['vertices'][a],result['vertices'][b])])
                if 'colors' in result:result['colors'].append([(x+y)/2 for x,y in zip(result['colors'][a],result['colors'][b])])
            mid=midpoints[key];pending.extend([[a,mid,c],[mid,b,c]])
    return result


def build(source,destination,riding_surface=False,coastline_osm=None,buildings_osm=None, detail_osm=None, imagery=None, driving_side="right"):
    raw=Path(source).read_bytes()
    if len(raw)>16*1024*1024: raise ValueError("Preparation exceeds 16 MiB")
    p=json.loads(raw)
    if p.get("kind")!='openride.scenery-preparation' or p.get('trainer_ready') is not False:
        raise ValueError('Expected preview preparation')
    profile=p['route_profile']
    if profile['mode'] not in ('road','gps','mixed') or profile['findings']: raise ValueError('Requires a continuous profile without unresolved findings')
    terrain=p['terrain']['mesh']; adjusted=0
    road=riding_road(profile['points']) if riding_surface else profile['mesh']
    if riding_surface: terrain,adjusted=conform_terrain(terrain,profile['points'])
    backdrop=None
    if coastline_osm:
        import importlib.util
        spec=importlib.util.spec_from_file_location('coastal_scenery',Path(__file__).with_name('coastal_scenery.py'))
        coast=importlib.util.module_from_spec(spec);spec.loader.exec_module(coast)
        terrain,backdrop=coast.prepare(p,terrain,coastline_osm)
    scenery={};building_report=None
    if buildings_osm:
        import importlib.util
        spec=importlib.util.spec_from_file_location('building_scenery',Path(__file__).with_name('building_scenery.py'))
        buildings=importlib.util.module_from_spec(spec);spec.loader.exec_module(buildings)
        scenery,building_report=buildings.prepare(p,terrain,buildings_osm)
    plants=[];road_metadata=None;detail_report=None
    if detail_osm:
        if not riding_surface:raise ValueError('Detailed scenery requires riding surface')
        import road_scenery
        terrain,detail,plants,road_metadata,detail_report=road_scenery.prepare(p,terrain,detail_osm,driving_side)
        scenery.update(detail)
        road=riding_road(profile['points'],[m['width_m'] for m in road_metadata])
    terrain=bounded_terrain(terrain)
    kinds=['terrain','road','buildings','roofs']+list(k for k in scenery if k not in ('buildings','roofs'))
    cells={}; tile=256
    def cell(positions):
        x=math.floor(sum(v[0] for v in positions)/len(positions)/tile)
        z=math.floor(sum(v[2] for v in positions)/len(positions)/tile)
        return cells.setdefault((x,z),dict({kind:dict(vertices=[],indices=[],colors=[]) for kind in kinds},lines=[],vegetation=[]))
    totals={}
    for kind,mesh in [('terrain',terrain),('road',road)]+list(scenery.items()):
        maps={}
        for i in range(0,len(mesh['indices']),3):
            ids=mesh['indices'][i:i+3]; triangle=[mesh['vertices'][v] for v in ids]
            # Centroid ownership preserves every triangle exactly once. Overhang is bounded.
            if any(math.dist(a,b)>32 for a in triangle for b in triangle): raise ValueError('Triangle exceeds 32 m chunk-overhang bound')
            target=cell(triangle)[kind]; mapping=maps.setdefault(id(target),{})
            for index in ids:
                if index not in mapping:
                    mapping[index]=len(target['vertices']);target['vertices'].append(mesh['vertices'][index])
                    if 'colors' in mesh:target['colors'].append(mesh['colors'][index])
                target['indices'].append(mapping[index])
        totals[kind+'_triangles']=len(mesh['indices'])//3
    for plant in plants:cell([plant[:3]])['vegetation'].append(plant)
    lines=p.get('osm_context',{}).get('display_lines',{}).get('vertices',[])
    for i in range(0,len(lines),2):
        segment=lines[i:i+2]
        if math.dist(*segment)>32: raise ValueError('Context segment exceeds overhang bound')
        cell(segment)['lines'].extend(segment)
    if len(cells)>4096: raise ValueError('Too many chunks')
    files={}; entries=[]
    for (x,z),geometry in sorted(cells.items()):
        for kind in kinds:
            if len(geometry[kind]['vertices'])>30000 or len(geometry[kind]['indices'])>90000: raise ValueError('Chunk geometry budget exceeded')
        if len(geometry['lines'])>30000: raise ValueError('Chunk line budget exceeded')
        name=f'chunk_{x}_{z}.json'; data=json.dumps(geometry,separators=(',',':'),allow_nan=False).encode()
        if len(data)>2*1024*1024: raise ValueError('Chunk exceeds 2 MiB')
        files[name]=data
        entries.append(dict(x=x,z=z,path=name,bytes=len(data),sha256=hashlib.sha256(data).hexdigest()))
    route=dict(points=profile['points'],mode=profile['mode'],validated=False)
    route_raw=json.dumps(route,separators=(',',':'),allow_nan=False).encode()
    if len(route_raw)>2*1024*1024: raise ValueError('Route metadata exceeds 2 MiB')
    files['route.json']=route_raw
    manifest=dict(kind='openride.stream-preview',schema_version=1,trainer_ready=False,name=p['name']+(' - detailed scenery' if detail_osm else ''),tile_m=tile,
        neighborhood_radius_cells=1,max_live_chunks=9,triangle_overhang_bound_m=32,
        source_preparation_sha256=hashlib.sha256(raw).hexdigest(),review_status='unvalidated-preview',
        route=dict(path='route.json',bytes=len(route_raw),sha256=hashlib.sha256(route_raw).hexdigest()),chunks=entries,
        attribution=p['terrain']['source_snapshot']['attribution'],origin_lat_lon_elevation=p['origin_lat_lon_elevation'],
        presentation=dict(terrain_corridor=riding_surface,adjusted_vertices=adjusted,inner_m=18,outer_m=30,clearance_m=0.25),
        totals=totals,retained_metadata='Full generated route points and chunk index only; no source activity or full-resolution meshes')
    if road_metadata is not None:
        data=json.dumps(dict(schema=1,road_points=road_metadata,summary=detail_report),separators=(',',':'),allow_nan=False).encode()
        if len(data)>2*1024*1024:raise ValueError('Road metadata exceeds limit')
        files['presentation.json']=data
        manifest['detail']=dict(path='presentation.json',bytes=len(data),sha256=hashlib.sha256(data).hexdigest())
        manifest['detail_summary']=detail_report
    if imagery:
        info=json.loads(Path(imagery).read_bytes());image_path=Path(imagery).parent/'imagery.png';data=image_path.read_bytes()
        if len(data)>16*1024*1024 or not data.startswith(b'\x89PNG\r\n\x1a\n') or hashlib.sha256(data).hexdigest()!=info['sha256']:raise ValueError('Invalid imagery snapshot')
        if info['origin_lat_lon_elevation']!=p['origin_lat_lon_elevation']:raise ValueError('Imagery origin mismatch')
        if info['bounds']!=p['terrain']['extent']:raise ValueError('Imagery bounds mismatch')
        files['imagery.png']=data
        manifest['imagery']=dict(info,path='imagery.png',bytes=len(data))
    if building_report is not None:manifest['building_summary']=building_report
    if backdrop is not None:
        data=json.dumps(backdrop,separators=(',',':'),allow_nan=False).encode()
        if len(data)>2*1024*1024:raise ValueError('Coastal backdrop exceeds 2 MiB')
        files['coast.json']=data
        manifest['coast']=dict(path='coast.json',bytes=len(data),sha256=hashlib.sha256(data).hexdigest())
        manifest['retained_metadata']+='; bounded coastal backdrop mesh and water polygon remain resident'
    manifest_raw=json.dumps(manifest,indent=2,allow_nan=False).encode()
    if len(manifest_raw)>1024*1024: raise ValueError('Manifest exceeds 1 MiB')
    output=Path(destination);output.mkdir()
    for name,data in files.items():(output/name).write_bytes(data)
    (output/'manifest.json').write_bytes(manifest_raw)
    return dict(chunks=len(entries),bytes=sum(map(len,files.values()))+len(manifest_raw),route_bytes=len(route_raw),**totals)


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('preparation');parser.add_argument('new_directory');parser.add_argument('--riding-surface',action='store_true');parser.add_argument('--coastline-osm');parser.add_argument('--buildings-osm');parser.add_argument('--detail-osm');parser.add_argument('--imagery');parser.add_argument('--driving-side',choices=['right','left'],default='right');args=parser.parse_args()
    try: print(json.dumps(build(args.preparation,args.new_directory,args.riding_surface,args.coastline_osm,args.buildings_osm,args.detail_osm,args.imagery,args.driving_side),indent=2))
    except Exception as error: print(error,file=sys.stderr);sys.exit(1)
