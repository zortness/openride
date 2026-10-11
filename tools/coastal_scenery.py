"""Bounded coastal presentation from a single connected, directed OSM shoreline."""
import hashlib
import math
import xml.etree.ElementTree as ET
from pathlib import Path


def clip_shoreline(chain, bounds):
    xmin,zmin,xmax,zmax=bounds
    runs=[];run=[]
    for a,b in zip(chain,chain[1:]):
        dx=b[0]-a[0];dz=b[1]-a[1];lo=0.;hi=1.
        for direction,offset in [(-dx,a[0]-xmin),(dx,xmax-a[0]),(-dz,a[1]-zmin),(dz,zmax-a[1])]:
            if direction==0:
                if offset<0:hi=-1;break
            elif direction<0:lo=max(lo,offset/direction)
            else:hi=min(hi,offset/direction)
        if lo<=hi:
            start=[a[0]+lo*dx,a[1]+lo*dz];end=[a[0]+hi*dx,a[1]+hi*dz]
            if not run:run=[start]
            elif math.dist(run[-1],start)>0.001:runs.append(run);run=[start]
            if math.dist(run[-1],end)>0.001:run.append(end)
        elif run:runs.append(run);run=[]
    if run:runs.append(run)
    runs=[r for r in runs if len(r)>1]
    if len(runs)!=1:raise ValueError('Expected one continuous coastline crossing the DEM bounds')
    return runs[0]


def water_polygon(chain, bounds, source_bounds=None):
    """Extend open endpoints to a rectangle and close on the water side (XZ)."""
    xmin,zmin,xmax,zmax=bounds
    def extend(p,q):
        dx=p[0]-q[0];dz=p[1]-q[1]
        candidates=[]
        if dx: candidates.extend([(xmin-p[0])/dx,(xmax-p[0])/dx])
        if dz: candidates.extend([(zmin-p[1])/dz,(zmax-p[1])/dz])
        for t in sorted(v for v in candidates if v>0):
            x,z=p[0]+t*dx,p[1]+t*dz
            if xmin-1e-6<=x<=xmax+1e-6 and zmin-1e-6<=z<=zmax+1e-6:return [x,z]
        raise ValueError('Cannot extend coastline endpoint')
    def outward(p):
        if source_bounds is None:return None
        for axis,index in [(0,0),(1,1),(0,2),(1,3)]:
            if abs(p[axis]-source_bounds[index])<0.001:
                q=p.copy();q[axis]=bounds[index];return q
        raise ValueError('Coastline endpoint must meet source bounds')
    start=outward(chain[0]) if source_bounds else extend(chain[0],chain[min(10,len(chain)-1)])
    end=outward(chain[-1]) if source_bounds else extend(chain[-1],chain[max(0,len(chain)-11)])
    width=xmax-xmin;height=zmax-zmin;perimeter=2*(width+height)
    def station(p):
        x,z=p
        if abs(z-zmin)<1e-5:return x-xmin
        if abs(x-xmax)<1e-5:return width+z-zmin
        if abs(z-zmax)<1e-5:return width+height+xmax-x
        return 2*width+height+zmax-z
    a=station(end);b=station(start)
    if b<=a:b+=perimeter
    corners=[(0,[xmin,zmin]),(width,[xmax,zmin]),(width+height,[xmax,zmax]),(2*width+height,[xmin,zmax])]
    between=sorted((s+k*perimeter,p) for k in (0,1) for s,p in corners if a<s+k*perimeter<b)
    return [start]+chain+[end]+[p for _,p in between]


def inside(x,z,polygon):
    result=False
    for a,b in zip(polygon,polygon[1:]+polygon[:1]):
        if (a[1]>z)!=(b[1]>z) and x<(b[0]-a[0])*(z-a[1])/(b[1]-a[1])+a[0]:result=not result
    return result


def prepare(preparation, terrain, osm_path):
    raw=Path(osm_path).read_bytes()
    if len(raw)>64*1024*1024 or b'<!DOCTYPE' in raw.upper() or b'<!ENTITY' in raw.upper():raise ValueError('Unsupported coastal XML')
    digest=hashlib.sha256(raw).hexdigest()
    if digest!=preparation['osm_context']['source_sha256']:raise ValueError('Coastline OSM does not match preparation')
    root=ET.fromstring(raw)
    if root.tag!='osm' or root.find('remark') is not None:raise ValueError('Not a complete OSM response')
    nodes={e.attrib['id']:(float(e.attrib['lat']),float(e.attrib['lon'])) for e in root.findall('node')}
    ways=[]
    for e in root.findall('way'):
        if any(t.attrib.get('k')=='natural' and t.attrib.get('v')=='coastline' for t in e.findall('tag')):
            ids=[n.attrib['ref'] for n in e.findall('nd')]
            if len(ids)<2 or any(i not in nodes for i in ids):raise ValueError('Incomplete coastline')
            ways.append(ids)
    count=len(ways)
    if not ways:
        # Inland data is valid; absence of coastline requires no ocean mask or backdrop.
        return terrain,None
    while True:
        joined=False
        for a in ways:
            for b in ways:
                if a is not b and a[-1]==b[0]:
                    a.extend(b[1:]);ways.remove(b);joined=True;break
            if joined:break
        if not joined:break
    if len(ways)!=1 or ways[0][0]==ways[0][-1] or len(set(ways[0]))!=len(ways[0]):
        raise ValueError('Coastal preview needs one unbranched open shoreline; islands/multiple chains need polygon processing')
    lat0,lon0,h0=preparation['origin_lat_lon_elevation']
    chain=[[6371000*math.radians(nodes[i][1]-lon0)*math.cos(math.radians(lat0)),-6371000*math.radians(nodes[i][0]-lat0)] for i in ways[0]]
    if len(chain)>5000 or any(not math.isfinite(v) or abs(v)>50000 for p in chain for v in p):raise ValueError('Coastline extent exceeds preview budget')
    source_bounds=[min(p[0] for p in terrain['vertices']),min(p[2] for p in terrain['vertices']),max(p[0] for p in terrain['vertices']),max(p[2] for p in terrain['vertices'])]
    chain=clip_shoreline(chain,source_bounds)
    bounds=[source_bounds[0]-12000,source_bounds[1]-12000,source_bounds[2]+12000,source_bounds[3]+12000]
    polygon=water_polygon(chain,bounds,source_bounds)
    if any(inside(p['position'][0],p['position'][2],polygon) for p in preparation['route_profile']['points']):
        raise ValueError('Coastline classifies route points as ocean; review source alignment')
    sea=-h0
    # Build a row-wise mask once; source DEM positions have a regular grid.
    crossings={}
    for _,_,z in terrain['vertices']:
        if z in crossings:continue
        crossings[z]=sorted(a[0]+(b[0]-a[0])*(z-a[1])/(b[1]-a[1]) for a,b in zip(polygon,polygon[1:]+polygon[:1]) if (a[1]>z)!=(b[1]>z))
    from bisect import bisect_right
    vertices=[];ocean_count=0
    for x,y,z in terrain['vertices']:
        water=bisect_right(crossings[z],x)%2==1
        if water:y=min(y,sea-0.5);ocean_count+=1
        vertices.append([x,y,z])
    width=preparation['terrain']['width'];height=preparation['terrain']['height']
    if width*height!=len(vertices):raise ValueError('Backdrop requires a regular terrain grid')
    xs=list(range(0,width-1,4))+[width-1];zs=list(range(0,height-1,4))+[height-1]
    distant=[[vertices[z*width+x][0],vertices[z*width+x][1]-4,vertices[z*width+x][2]] for z in zs for x in xs]
    indices=[];stride=len(xs)
    for z in range(len(zs)-1):
        for x in range(stride-1):
            a=z*stride+x;indices.extend([a,a+stride,a+1,a+1,a+stride,a+stride+1])
    backdrop=dict(terrain=dict(vertices=distant,indices=indices),water_polygon=polygon,sea_level_y=sea,
        source_osm_sha256=digest,source_bounds_xz=source_bounds,coastline_ways=count,coastline_points=len(chain),masked_terrain_vertices=ocean_count,
        sea_level_reference='Nominal 0 m source elevation; unvalidated vertical datum',
        boundary_policy='Shoreline clipped to DEM bounds; endpoints extended outward perpendicular to bounds to a far rectangle. No geographic claims beyond source coverage.',
        terrain_policy='Full source extent sampled every fourth grid vertex, lowered 4 m beneath streamed terrain')
    return dict(vertices=vertices,indices=terrain['indices']),backdrop
