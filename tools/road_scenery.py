"""Offline presentation geometry. Never changes route stationing or elevation.

OSM widths/lanes are evidence; fallback dimensions and vegetation are estimates.
Unsupported bridges/tunnels and multipolygon land cover are reported and skipped.
"""
import hashlib
import math
import random
import xml.etree.ElementTree as ET
from pathlib import Path
from building_scenery import ground_sampler, add_triangle

ROAD_CLASSES = {'motorway','trunk','primary','secondary','tertiary','residential','unclassified',
                'service','living_street','cycleway','path','footway','pedestrian','track'}
COLORS = {'grass':[0.43,0.49,0.27], 'grassland':[0.49,0.51,0.29], 'scrub':[0.39,0.43,0.27],
          'wood':[0.25,0.36,0.22], 'forest':[0.25,0.36,0.22], 'sand':[0.72,0.65,0.45],
          'residential':[0.49,0.48,0.38], 'commercial':[0.49,0.47,0.42], 'brownfield':[0.52,0.43,0.31]}

def dimensions(tags):
    lanes = tags.get('lanes','')
    lanes = int(lanes) if lanes.isdigit() and 1<=int(lanes)<=8 else None
    try:
        raw=tags.get('width','').strip()
        width=float(raw[:-1].strip() if raw.endswith('m') else raw)
        if not math.isfinite(width) or not 1<=width<=30:raise ValueError()
        source='width'
    except ValueError:
        width=lanes*3.2 if lanes else (2.5 if tags.get('highway') in ('path','footway','cycleway','track') else 5 if tags.get('highway')=='service' else 6.4)
        source='estimated-from-lanes' if lanes else 'estimated-from-class'
    return width,lanes,source

def lane_offset(tags, forward, driving_side='right'):
    """Signed offset to rider's right. Ambiguous layouts use the center."""
    width,lanes,_=dimensions(tags)
    if not lanes or tags.get('highway') in ('footway','path','cycleway'):return 0.
    one=tags.get('oneway') in ('yes','1','true','-1')
    if one and forward == (tags.get('oneway')=='-1'):return 0.
    sign=1 if driving_side=='right' else -1
    if one:count=lanes
    else:
        directional=tags.get('lanes:forward' if forward else 'lanes:backward','')
        if directional.isdigit():count=int(directional)
        elif lanes==2:count=1
        else:return 0.
        if count<1 or count>=lanes:return 0.
    # Through lane nearest the riding side; no claim of exact lane widths.
    turn_key='turn:lanes' if one else ('turn:lanes:forward' if forward else 'turn:lanes:backward')
    turns=tags.get(turn_key,'').split('|')
    skip=0
    if len(turns)==count:
        order=list(reversed(turns)) if sign>0 else turns
        for turn in order:
            if turn and 'through' not in turn:skip+=1
            else:break
        if skip==count:return 0.
    offset=sign*(width/2-width/lanes*(0.5+skip))
    side='right' if (forward == (sign>0)) else 'left'
    cycle=tags.get('cycleway:'+side,tags.get('cycleway',''))
    # A cycling lane inside an explicitly known total width is usable; otherwise
    # its width/location is unknown, so retain the through-lane estimate.
    if cycle=='lane' and dimensions(tags)[2]=='width':offset=sign*(width/2-0.8)
    return offset

def nearest(x,z,a,b):
    dx=b[0]-a[0];dz=b[1]-a[1];d=dx*dx+dz*dz
    if d<1e-8:return (1e20,0.)
    t=max(0.,min(1.,((x-a[0])*dx+(z-a[1])*dz)/d))
    return math.hypot(x-a[0]-dx*t,z-a[1]-dz*t),t

def inside(x,z,poly):
    hit=False
    for a,b in zip(poly,poly[1:]+poly[:1]):
        if (a[1]>z)!=(b[1]>z) and x<(b[0]-a[0])*(z-a[1])/(b[1]-a[1])+a[0]:hit=not hit
    return hit

def ribbon(mesh,a,b,half,ground=None,offset=0.,height=0.):
    dx=b[0]-a[0];dz=b[2]-a[2];length=math.hypot(dx,dz)
    if length<0.01:return
    nx=-dz/length;nz=dx/length
    vertices=[]
    for q,side in [(a,offset-half),(a,offset+half),(b,offset-half),(b,offset+half)]:
        x=q[0]+nx*side;z=q[2]+nz*side;y=ground(x,z) if ground else q[1]
        if y is None:return
        vertices.append([x,y+height,z])
    for ids in [(0,1,2),(1,3,2)]:add_triangle(mesh,*(vertices[i] for i in ids))

def prepare(p, terrain, path, driving_side='right'):
    raw=Path(path).read_bytes()
    if len(raw)>64*1024*1024 or b'<!DOCTYPE' in raw.upper() or b'<!ENTITY' in raw.upper():raise ValueError('Unsupported scenery XML')
    if hashlib.sha256(raw).hexdigest()!=p['osm_context']['source_sha256']:raise ValueError('Scenery OSM identity mismatch')
    root=ET.fromstring(raw);lat,lon,_=p['origin_lat_lon_elevation']
    def local(e):return [6371000*math.radians(float(e.attrib['lon'])-lon)*math.cos(math.radians(lat)),-6371000*math.radians(float(e.attrib['lat'])-lat)]
    nodes={e.attrib['id']:local(e) for e in root.findall('node')}
    if any(not math.isfinite(v) for q in nodes.values() for v in q):raise ValueError('Invalid OSM position')
    members={m.attrib.get('ref') for r in root.findall('relation') for m in r.findall('member') if m.attrib.get('type')=='way'}
    roads=[];polygons=[];buildings=[];skipped=0;relations=0
    for w in root.findall('way'):
        tags={e.attrib['k']:e.attrib['v'] for e in w.findall('tag')};ids=[e.attrib['ref'] for e in w.findall('nd')]
        if any(i not in nodes for i in ids):continue
        poly=[nodes[i] for i in ids]
        if tags.get('highway','').removesuffix('_link') in ROAD_CLASSES:
            if tags.get('bridge','no')!='no' or tags.get('tunnel','no')!='no':skipped+=1;continue
            roads.append(dict(id=w.attrib['id'],tags=tags,poly=poly,width=dimensions(tags)[0]))
        if len(ids)>3 and ids[0]==ids[-1]:
            if 'building' in tags:buildings.append(poly[:-1])
            kind=tags.get('natural',tags.get('landuse',''))
            if kind in COLORS or kind=='water':
                if w.attrib['id'] in members:relations+=1;continue
                polygons.append((kind,poly[:-1]))
    points=p['route_profile']['points'];positions=[q['position'] for q in points]
    segments=[(r,a,b) for r in roads for a,b in zip(r['poly'],r['poly'][1:]) if math.dist(a,b)<10000]
    bounds=[min(q[0] for q in positions)-300,min(q[2] for q in positions)-300,max(q[0] for q in positions)+300,max(q[2] for q in positions)+300]
    # Spatial bins bound point-to-road work during vegetation and route attribution.
    bins={}
    for seg in segments:
        r,a,b=seg
        for x in range(math.floor(max(bounds[0],min(a[0],b[0])-20)/64),math.floor(min(bounds[2],max(a[0],b[0])+20)/64)+1):
            for z in range(math.floor(max(bounds[1],min(a[1],b[1])-20)/64),math.floor(min(bounds[3],max(a[1],b[1])+20)/64)+1):bins.setdefault((x,z),[]).append(seg)
    def nearby(x,z):return bins.get((math.floor(x/64),math.floor(z/64)),[])
    def route_near(x,z):
        best=(1e20,0.)
        for a,b in zip(positions,positions[1:]):
            d,t=nearest(x,z,[a[0],a[2]],[b[0],b[2]])
            if d<best[0]:best=(d,a[1]+(b[1]-a[1])*t)
        return best
    sample=ground_sampler(terrain,p['terrain']['width'],p['terrain']['height'])
    metadata=[];matched=0
    for i,q in enumerate(points):
        x,_,z=q['position'];a=positions[max(0,i-1)];b=positions[min(len(points)-1,i+1)];dx=b[0]-a[0];dz=b[2]-a[2];length=math.hypot(dx,dz)
        candidates=[]
        for r,aa,bb in nearby(x,z):
            d,_=nearest(x,z,aa,bb);ex=bb[0]-aa[0];ez=bb[1]-aa[1];n=math.hypot(ex,ez)
            dot=(dx*ex+dz*ez)/(length*n) if length*n else 0
            if d<1.5 and abs(dot)>.85:candidates.append((d,r,dot>0))
        candidates.sort(key=lambda c:c[0]);r=None
        if candidates:
            _,r,forward=candidates[0]
            # Parallel nearby ways are ambiguous; shared-node transitions use fallback.
            if any(c[1]['id']!=r['id'] and abs(c[0]-candidates[0][0])<0.15 and c[1]['tags']!=r['tags'] for c in candidates[1:]):r=None
        tags=r['tags'] if r else {};width,lanes,source=dimensions(tags)
        meta=dict(distance_m=q['distance_m'],width_m=width,lane_offset_m=lane_offset(tags,forward,driving_side) if r else 0.,way_id=r['id'] if r else None,tags=tags,width_source=source)
        metadata.append(meta);matched+=r is not None
    # Limit lateral speed versus distance at attribute transitions in both directions.
    for order in [range(1,len(metadata)),range(len(metadata)-2,-1,-1)]:
        for i in order:
            j=i-1 if order.step>0 else i+1;limit=abs(metadata[i]['distance_m']-metadata[j]['distance_m'])*.08
            if metadata[i]['lane_offset_m']!=0:metadata[i]['lane_offset_m']=max(metadata[j]['lane_offset_m']-limit,min(metadata[j]['lane_offset_m']+limit,metadata[i]['lane_offset_m']))
    meshes={k:dict(vertices=[],indices=[]) for k in ['context_roads','unpaved','markings','center_markings']}
    # Route markings follow exact accepted heights. Lane divider offsets are estimates.
    for i,(a,b) in enumerate(zip(positions,positions[1:])):
        meta=metadata[i];tags=meta['tags'];width=meta['width_m'];_,lanes,_=dimensions(tags)
        if tags.get('lane_markings')=='no' or not lanes or tags.get('surface','asphalt') not in ('asphalt','paved','concrete'):continue
        for edge in [-1,1]:ribbon(meshes['markings'],a,b,.06,offset=edge*(width/2-.2),height=.035)
        one=tags.get('oneway') in ('yes','1','true','-1')
        if not one and lanes==2:ribbon(meshes['center_markings'],a,b,.065,height=.04)
        if one and int(meta['distance_m']/4)%2==0:
            for lane in range(1,lanes):ribbon(meshes['markings'],a,b,.06,offset=-width/2+lane*width/lanes,height=.04)
    drawn=0
    for r in roads:
        used=False
        for a,b in zip(r['poly'],r['poly'][1:]):
            n=max(1,math.ceil(math.dist(a,b)/5))
            if n>2000:raise ValueError('Road segment too large')
            for j in range(n):
                aa=[a[k]+(b[k]-a[k])*j/n for k in range(2)];bb=[a[k]+(b[k]-a[k])*(j+1)/n for k in range(2)]
                cx=(aa[0]+bb[0])/2;cz=(aa[1]+bb[1])/2;distance,_=route_near(cx,cz)
                if distance>300 or distance<1.0:continue
                kind='unpaved' if r['tags'].get('surface') in ('dirt','gravel','ground','sand','unpaved') or r['tags'].get('highway') in ('path','track','footway') else 'context_roads'
                ribbon(meshes[kind],[aa[0],0,aa[1]],[bb[0],0,bb[1]],r['width']/2,ground=sample,height=.28);used=True
        drawn+=used
    colors=[]
    for x,_,z in terrain['vertices']:
        color=[.46,.48,.33]
        for kind,poly in polygons:
            if kind in COLORS and inside(x,z,poly):color=COLORS[kind]
        colors.append(color)
    terrain=dict(terrain,colors=colors)
    rng=random.Random(71023);plants=[];rejected=0
    def plant(x,z,kind,scale):
        nonlocal rejected
        y=sample(x,z)
        if y is None or y<=-p['origin_lat_lon_elevation'][2]+1:return
        if any(nearest(x,z,a,b)[0]<r['width']/2+3 for r,a,b in nearby(x,z)):rejected+=1;return
        if route_near(x,z)[0]<10:return
        if any(inside(x,z,poly) for poly in buildings):return
        if any(k=='water' and inside(x,z,poly) for k,poly in polygons):return
        if route_near(x,z)[0]>250:return
        plants.append([x,y,z,scale,kind,rng.random()*math.tau])
    for kind,poly in polygons:
        if kind not in ('grass','grassland','scrub','wood','forest'):continue
        lo=[min(q[k] for q in poly) for k in range(2)];hi=[max(q[k] for q in poly) for k in range(2)]
        lo=[max(lo[0],bounds[0]),max(lo[1],bounds[1])];hi=[min(hi[0],bounds[2]),min(hi[1],bounds[3])]
        for x in range(math.floor(lo[0]/12),math.ceil(hi[0]/12)):
            for z in range(math.floor(lo[1]/12),math.ceil(hi[1]/12)):
                xx=x*12+rng.random()*12;zz=z*12+rng.random()*12
                if inside(xx,zz,poly):plant(xx,zz,1 if kind in ('wood','forest') else 0,rng.uniform(.7,1.4))
    for node in root.findall('node'):
        if any(t.attrib.get('k')=='natural' and t.attrib.get('v')=='tree' for t in node.findall('tag')):
            x,z=nodes[node.attrib['id']];plant(x,z,1,rng.uniform(.8,1.3))
    if len(plants)>15000:raise ValueError('Vegetation budget exceeded')
    return terrain,meshes,plants,metadata,dict(source_osm_sha256=hashlib.sha256(raw).hexdigest(),road_ways=drawn,matched_route_points=matched,route_points=len(points),vegetation_instances=len(plants),skipped_bridge_tunnel_ways=skipped,skipped_landcover_relation_members=relations,driving_side=driving_side,vegetation_policy='Mapped trees; seeded inferred shrubs/trees within simple mapped land-cover polygons; road/building/water clearance',width_policy='Explicit width, else lanes * 3.2m, else class estimate')
