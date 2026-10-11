"""Simple OSM building massing for the bounded scenery preview (no textures/assets)."""
import hashlib
import math
from pathlib import Path
import xml.etree.ElementTree as ET


def height_m(tags):
    if 'height' in tags:
        value=float(tags['height'].strip().removesuffix('m').strip());method='height'
    elif 'building:levels' in tags:
        value=float(tags['building:levels'])*3;method='levels-times-3m'
    else:value=6.;method='assumed-6m'
    if not math.isfinite(value) or not 2<=value<=40:raise ValueError('Unsupported height')
    return value,method


def ground_sampler(mesh,width,height):
    v=mesh['vertices']
    if len(v)!=width*height:raise ValueError('Buildings require regular terrain grid')
    x0=v[0][0];z0=v[0][2];dx=v[1][0]-x0;dz=v[width][2]-z0
    def sample(x,z):
        fx=(x-x0)/dx;fz=(z-z0)/dz
        if not 0<=fx<=width-1 or not 0<=fz<=height-1:return None
        ix=min(int(fx),width-2);iz=min(int(fz),height-2);tx=fx-ix;tz=fz-iz
        a=v[iz*width+ix][1];b=v[iz*width+ix+1][1];c=v[(iz+1)*width+ix][1];d=v[(iz+1)*width+ix+1][1]
        return (a*(1-tx)+b*tx)*(1-tz)+(c*(1-tx)+d*tx)*tz
    return sample


def roof_triangles(poly):
    """Ear clipping for a simple footprint, including concave L-shaped houses."""
    def cross(a,b,c):return (b[0]-a[0])*(c[1]-a[1])-(b[1]-a[1])*(c[0]-a[0])
    area=sum(a[0]*b[1]-a[1]*b[0] for a,b in zip(poly,poly[1:]+poly[:1]))
    if abs(area)<0.1:raise ValueError('Degenerate footprint')
    for i,a in enumerate(poly):
        b=poly[(i+1)%len(poly)]
        for j in range(i+2,len(poly)):
            if i==0 and j==len(poly)-1:continue
            c=poly[j];d=poly[(j+1)%len(poly)]
            if cross(a,b,c)*cross(a,b,d)<0 and cross(c,d,a)*cross(c,d,b)<0:raise ValueError('Crossing footprint')
    order=list(range(len(poly))) if area>0 else list(reversed(range(len(poly))))
    triangles=[]
    while len(order)>3:
        found=False
        for i,b in enumerate(order):
            a=order[i-1];c=order[(i+1)%len(order)]
            if cross(poly[a],poly[b],poly[c])<=1e-8:continue
            if any(all(cross(poly[u],poly[v],poly[q])>=-1e-8 for u,v in [(a,b),(b,c),(c,a)]) for q in order if q not in (a,b,c)):continue
            triangles.append((a,b,c));order.pop(i);found=True;break
        if not found:raise ValueError('Cannot triangulate footprint')
    triangles.append(tuple(order))
    return triangles


def near_road(poly,route):
    # Polygon containment and edge distance; road points are <=5 m apart.
    for x,_,z in route:
        if x<min(p[0] for p in poly)-6 or x>max(p[0] for p in poly)+6 or z<min(p[1] for p in poly)-6 or z>max(p[1] for p in poly)+6:continue
        crosses=[]
        for a,b in zip(poly,poly[1:]+poly[:1]):
            dx=b[0]-a[0];dz=b[1]-a[1];n=dx*dx+dz*dz
            if n==0:return True
            t=max(0,min(1,((x-a[0])*dx+(z-a[1])*dz)/n))
            if math.hypot(x-a[0]-t*dx,z-a[1]-t*dz)<6:return True
            crosses.append(dx*(z-a[1])-dz*(x-a[0]))
        inside=False
        for a,b in zip(poly,poly[1:]+poly[:1]):
            if (a[1]>z)!=(b[1]>z) and x<(b[0]-a[0])*(z-a[1])/(b[1]-a[1])+a[0]:inside=not inside
        if inside:return True
    return False


def add_triangle(mesh,a,b,c):
    # Keep the existing <=32 m primitive bound, including roofs of larger buildings.
    if max(math.dist(a,b),math.dist(b,c),math.dist(c,a))>32:
        ab=[(x+y)/2 for x,y in zip(a,b)];bc=[(x+y)/2 for x,y in zip(b,c)];ca=[(x+y)/2 for x,y in zip(c,a)]
        for tri in [(a,ab,ca),(ab,b,bc),(ca,bc,c),(ab,bc,ca)]:add_triangle(mesh,*tri)
    else:
        start=len(mesh['vertices']);mesh['vertices'].extend([a,b,c]);mesh['indices'].extend([start,start+1,start+2])


def prepare(p,terrain,path):
    raw=Path(path).read_bytes()
    if len(raw)>64*1024*1024 or b'<!DOCTYPE' in raw.upper() or b'<!ENTITY' in raw.upper():raise ValueError('Unsupported building XML')
    digest=hashlib.sha256(raw).hexdigest()
    if digest!=p['osm_context']['source_sha256']:raise ValueError('Building OSM does not match preparation')
    root=ET.fromstring(raw);lat0,lon0,_=p['origin_lat_lon_elevation']
    nodes={e.attrib['id']:[6371000*math.radians(float(e.attrib['lon'])-lon0)*math.cos(math.radians(lat0)),-6371000*math.radians(float(e.attrib['lat'])-lat0)] for e in root.findall('node')}
    # Do not fill multipolygon holes by treating member ways as standalone buildings.
    members={m.attrib.get('ref') for r in root.findall('relation') for m in r.findall('member') if m.attrib.get('type')=='way'}
    sample=ground_sampler(terrain,p['terrain']['width'],p['terrain']['height'])
    route=[q['position'] for q in p['route_profile']['points']]
    meshes={k:dict(vertices=[],indices=[]) for k in ['buildings','roofs']}
    report=dict(source_osm_sha256=digest,buildings=[],skipped={},radius_m=250,height_policy='height tag, else levels * 3 m, else assumed 6 m; flat roofs')
    def skip(reason):report['skipped'][reason]=report['skipped'].get(reason,0)+1
    for way in root.findall('way'):
        tags={t.attrib['k']:t.attrib['v'] for t in way.findall('tag')}
        if tags.get('building') in (None,'no'):continue
        ids=[e.attrib['ref'] for e in way.findall('nd')]
        if way.attrib['id'] in members:skip('relation-member');continue
        if len(ids)<4 or ids[0]!=ids[-1] or len(ids)>65 or any(i not in nodes for i in ids):skip('unsupported-footprint');continue
        poly=[nodes[i] for i in ids[:-1]]
        if any(not math.isfinite(v) for q in poly for v in q):raise ValueError('Nonfinite building position')
        cx=sum(q[0] for q in poly)/len(poly);cz=sum(q[1] for q in poly)/len(poly)
        if min(math.hypot(cx-q[0],cz-q[2]) for q in route)>250:skip('outside-corridor');continue
        if len(set(ids[:-1]))!=len(ids)-1:skip('repeated-node');continue
        try:roof_ids=roof_triangles(poly)
        except ValueError:skip('invalid-footprint');continue
        if max(math.dist(a,b) for a in poly for b in poly)>100:skip('large-footprint');continue
        if near_road(poly,route):skip('road-clearance');continue
        grounds=[sample(*q) for q in poly]
        if any(h is None for h in grounds):skip('outside-dem');continue
        if max(grounds)-min(grounds)>8:skip('steep-ground');continue
        try:h,method=height_m(tags)
        except ValueError:skip('unsupported-height');continue
        low=min(grounds)-0.5;high=max(grounds)+h
        for a,b in zip(poly,poly[1:]+poly[:1]):
            aa=[a[0],low,a[1]];bb=[b[0],low,b[1]];at=[a[0],high,a[1]];bt=[b[0],high,b[1]]
            add_triangle(meshes['buildings'],aa,bb,at);add_triangle(meshes['buildings'],bb,bt,at)
        roof=[[q[0],high,q[1]] for q in poly]
        for a,b,c in roof_ids:add_triangle(meshes['roofs'],roof[a],roof[b],roof[c])
        report['buildings'].append(dict(way_id=way.attrib['id'],height_m=h,height_source=method))
        if len(report['buildings'])>2000:raise ValueError('Building budget exceeded')
    return meshes,report
