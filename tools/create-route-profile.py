"""Choose GPS geometry or a connected OSM road centreline for an offline preview route."""
import argparse
import bisect
import copy
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import sys

spec=importlib.util.spec_from_file_location("osm_context",Path(__file__).with_name("osm-context.py"))
osm=importlib.util.module_from_spec(spec)
spec.loader.exec_module(osm)


def distance(a,b): return math.hypot(b[0]-a[0],b[1]-a[1])

def access_notes(preparation):
    """Record outdoor restrictions on explicitly selected geometry for indoor use."""
    kept={p['source_sample'] for p in preparation.get('reviewed_profile',preparation['corrected_profile'])['points']}
    matches={m['source_sample']:m['candidates'] for m in preparation['osm_context']['matches']}
    notes={}
    for choice in preparation['corrections']['road_choices']:
        sample=choice['source_sample']
        if sample not in kept:continue
        for candidate in matches.get(sample,[]):
            if candidate['way_id']!=choice['way_id'] or candidate['node_ids']!=choice['node_ids']:continue
            tags=candidate.get('tags',{})
            restrictions={k:tags[k] for k in ('access','bicycle') if tags.get(k) in ('no','private')}
            if restrictions:
                note=notes.setdefault(choice['way_id'],dict(way_id=choice['way_id'],name=candidate.get('name'),tags=restrictions,source_samples=[]))
                note['source_samples'].append(sample)
    return list(notes.values())

def mixed_profile(preparation, osm_bytes, connect_gaps=False):
    """Follow selected road runs; retain GPS at defaults and their transitions."""
    base=preparation.get('reviewed_profile',preparation['corrected_profile'])['points']
    choice_map={c['source_sample']:c for c in preparation['corrections']['road_choices']}
    choices=set(choice_map)
    if hashlib.sha256(osm_bytes).hexdigest()!=preparation['osm_context']['source_sha256']:raise ValueError('OSM source checksum conflict')
    # Partition selected ways by shared OSM nodes. Never guess a route through unselected ways.
    selected={choice_map[p['source_sample']]['way_id'] for p in base if p['source_sample'] in choices}
    parents={w:w for w in selected};owners={}
    def component(w):
        while parents[w]!=w:
            parents[w]=parents[parents[w]];w=parents[w]
        return w
    for road in osm.analyze(preparation,osm_bytes)['roads']:
        w=road['way_id']
        if w not in selected:continue
        for node in road['references']:
            if node in owners:parents[component(w)]=component(owners[node])
            else:owners[node]=w
    def group(point):
        return component(choice_map[point['source_sample']]['way_id']) if point['source_sample'] in choices else None
    gaps=[]
    numbers={p['source_sample']:i+1 for i,p in enumerate(preparation['points'])}
    for a,b in zip(base,base[1:]):
        if group(a) is None or group(b) is None or group(a)==group(b):continue
        length=distance(a['position'][::2],b['position'][::2])
        gaps.append(dict(start_source_sample=a['source_sample'],end_source_sample=b['source_sample'],start_point=numbers[a['source_sample']],end_point=numbers[b['source_sample']],length_m=length))
    if gaps and not connect_gaps:
        detail='; '.join(f"points {g['start_point']}–{g['end_point']} ({g['length_m']:.1f} m)" for g in gaps[:8])
        raise ValueError('Selected roads have disconnected joins: '+detail+'. Review road choices or enable Connect short gaps between selected roads (straight connectors up to 100 m).')
    for gap in gaps:
        if not 0.001<gap['length_m']<=100:
            raise ValueError(f"Cannot connect points {gap['start_point']}–{gap['end_point']}: gap {gap['length_m']:.1f} m exceeds the 100 m connector limit or has coincident endpoints")
    if not gaps and all(p['source_sample'] in choices for p in base):return road_profile(preparation,osm_bytes)
    anchors=[];sections=[];i=0
    while i<len(base):
        end=i+1
        selected=base[i]['source_sample'] in choices
        if selected:
            while end<len(base) and base[end]['source_sample'] in choices and group(base[end])==group(base[i]):end+=1
        if selected and end-i>=2:
            subset=dict(preparation,reviewed_profile=dict(points=base[i:end]))
            try:run=road_profile(subset,osm_bytes)
            except ValueError as error:raise ValueError(f"Selected road section at source samples {base[i]['source_sample']}–{base[end-1]['source_sample']}: {error}") from error
            generated=copy.deepcopy(run['points'])
            generated[0]['break_before']=base[i].get('break_before',False)
            anchors.extend(generated)
            sections.append(dict(mode='road',start_source_sample=base[i]['source_sample'],end_source_sample=base[end-1]['source_sample']))
        else:
            anchors.append(copy.deepcopy(base[i]))
            sections.append(dict(mode='snapped-point' if selected else 'gps',start_source_sample=base[i]['source_sample'],end_source_sample=base[i]['source_sample']))
        i=end
    points=[];vertices=[];indices=[];findings=[];station=0.0
    def append(position,height):
        nonlocal station
        p=dict(position=position,elevation_m=height,distance_m=station,break_before=not points,grade_percent=None)
        if points:
            a=points[-1]['position'];dx=position[0]-a[0];dz=position[2]-a[2];length=math.hypot(dx,dz)
            station+=length;p['distance_m']=station;p['grade_percent']=100*(position[1]-a[1])/length
            if abs(p['grade_percent'])>15:findings.append(dict(point=len(points),issue='grade_over_15_percent'))
            side=[-3*dz/length,0,3*dx/length];offset=len(vertices)
            vertices.extend([[q[j]+sign*side[j] for j in range(3)] for q in (a,position) for sign in (1,-1)])
            indices.extend([offset,offset+1,offset+2,offset+1,offset+3,offset+2])
        points.append(p)
    previous=None
    for anchor in anchors:
        pos=anchor['position'];height=anchor.get('elevation_m')
        if height is None or not math.isfinite(height) or any(v is None or not math.isfinite(v) for v in pos):raise ValueError('Missing/nonfinite route elevation or position; correct heights before building')
        if previous is None:append(list(pos),height)
        else:
            a=previous['position'];length=math.hypot(pos[0]-a[0],pos[2]-a[2])
            if anchor.get('break_before') or not 0.001<length<=100:
                raise ValueError(f"GPS/road connection is disconnected or outside 0.001–100 m near source sample {anchor.get('source_sample','road section')}; repair the join")
            count=math.ceil(length/5)
            for step in range(1,count+1):
                t=step/count;append([a[j]+t*(pos[j]-a[j]) for j in range(3)],previous['elevation_m']+t*(height-previous['elevation_m']))
        previous=anchor
    if not 1<=station<=6000:raise ValueError('Route outside 1..6000 m preview limit')
    return dict(points=points,mesh=dict(vertices=vertices,indices=indices,width_m=6),findings=findings,validated=False,
                mode='mixed' if choices else 'gps',length_m=station,sections=sections,road_gap_connectors=gaps,
                elevation_method='Corrected heights; road runs use road-distance interpolation; GPS transitions use linear interpolation',
                access_notes=access_notes(preparation),
                warnings=['Default points retain corrected GPS positions. Road/GPS transitions are straight connectors; inspect the preview.'])


def road_profile(preparation, osm_bytes):
    context=preparation["osm_context"]
    if hashlib.sha256(osm_bytes).hexdigest()!=context["source_sha256"]:
        raise ValueError("OSM source checksum conflict")
    base=preparation.get("reviewed_profile",preparation["corrected_profile"])["points"]
    choices={c["source_sample"]:c for c in preparation["corrections"]["road_choices"]}
    matches={m["source_sample"]:m["candidates"] for m in context["matches"]}
    missing=[int(p['source_sample']) for p in base if p['source_sample'] not in choices]
    if missing:
        shown=', '.join(map(str,missing[:12]))+(' ...' if len(missing)>12 else '')
        raise ValueError(f'Missing road choice at {len(missing)} retained points (source samples: {shown}). Open road review and use Next missing road choice, or choose Use corrected GPS points.')
    anchors=[]
    for point in base:
        choice=choices.get(point["source_sample"])
        if not choice: raise ValueError("Missing road choice; review or explicitly use GPS fallback")
        candidates=[c for c in matches[point["source_sample"]] if c["way_id"]==choice["way_id"] and c["node_ids"]==choice["node_ids"]]
        if len(candidates)!=1: raise ValueError("Road choice conflict")
        if point["elevation_m"] is None: raise ValueError("Missing elevation")
        anchors.append(candidates[0])
    selected={c["way_id"] for c in anchors}
    roads=[w for w in osm.analyze(preparation,osm_bytes)["roads"] if w["way_id"] in selected]
    if len(roads)!=len(selected): raise ValueError("Selected OSM way unavailable")
    coordinates={}
    edges={}
    for road in roads:
        tags=road["tags"]
        if any(tags.get(k,"no") not in ("no","0") for k in ("bridge","tunnel")):
            raise ValueError("Bridge/tunnel needs an explicit elevation review")
        # Explicit choices define indoor scenery; outdoor access tags are recorded below.
        coordinates.update(zip(road["references"],road["positions"]))
        for a,b in zip(road["references"],road["references"][1:]):
            if a==b: continue
            key=tuple(sorted((a,b)))
            if key in edges: raise ValueError("Overlapping selected ways need review")
            edges[key]=(a,b,tags)
    if len(edges)>2000: raise ValueError("Road graph exceeds preview limit")
    cuts={}
    for key,candidate in (("@start",anchors[0]),("@end",anchors[-1])):
        a,b=candidate["node_ids"]
        edge=tuple(sorted((a,b)))
        if edge not in edges: raise ValueError("Candidate edge missing from OSM")
        f=candidate["fraction"] if edges[edge][0]==a else 1-candidate["fraction"]
        cuts.setdefault(edge,[]).append((f,key))
    graph={}
    directions={}
    for edge,(a,b,tags) in edges.items():
        parts=[(0,a)]+sorted(cuts.get(edge,[]))+[(1,b)]
        for f,node in parts:
            coordinates[node]=(coordinates[a][0]+f*(coordinates[b][0]-coordinates[a][0]),coordinates[a][1]+f*(coordinates[b][1]-coordinates[a][1]))
        for (_,u),(_,v) in zip(parts,parts[1:]):
            graph.setdefault(u,[]).append(v); graph.setdefault(v,[]).append(u)
            directions[(u,v)]=tags.get("oneway","no") not in ("-1",)
            directions[(v,u)]=tags.get("oneway","no") not in ("yes","1","true")
    def find_path(blocked=None):
        pending=["@start"]; previous={"@start":None}
        for node in pending:
            if node=="@end": break
            for next_node in graph.get(node,[]):
                if frozenset((node,next_node))==blocked or next_node in previous: continue
                previous[next_node]=node; pending.append(next_node)
        if "@end" not in previous: return None
        path=[]; node="@end"
        while node is not None: path.append(node); node=previous[node]
        return path[::-1]
    path=find_path()
    if path is None: raise ValueError("Selected roads do not form a connected route")
    for a,b in zip(path,path[1:]):
        if find_path(frozenset((a,b))) is not None:
            raise ValueError("Multiple connected paths; additional route constraints required")
        if not directions[(a,b)]: raise ValueError("Route conflicts with OSM one-way direction")
    line=[coordinates[path[0]]]
    for node in path[1:]:
        if distance(line[-1],coordinates[node])>0.001: line.append(coordinates[node])
    stations=[0.0]
    for a,b in zip(line,line[1:]): stations.append(stations[-1]+distance(a,b))
    if stations[-1]<1 or stations[-1]>6000: raise ValueError("Road route outside 1..6000 m preview limit")
    heights=[]; ignored=[]
    for point,candidate in zip(base,anchors):
        xz=candidate["projected_xz"]
        projections=[]
        for i,(a,b) in enumerate(zip(line,line[1:])):
            nearest=osm.nearest(xz,a,b)
            if nearest is None: continue
            d,f=nearest
            projections.append((d,stations[i]+f*(stations[i+1]-stations[i])))
        deviation,station=min(projections)
        if deviation>0.5: raise ValueError(f"Selected source sample {point['source_sample']} lies off the connected path (possible turn or loop); review its road choice or keep its original position")
        if heights and station<heights[-1][0]-10:
            raise ValueError("Significant backtracking/U-turn; split or explicitly review the route")
        if heights and station<heights[-1][0]+5:
            ignored.append(point["source_sample"]); continue
        heights.append((station,point["elevation_m"]))
    # Always preserve both endpoint heights; nearby intermediate anchors are redundant.
    while len(heights)>1 and heights[-1][0]>=stations[-1]-1: heights.pop()
    heights.append((stations[-1],base[-1]["elevation_m"]))
    heights[0]=(0,base[0]["elevation_m"])
    points=[]; vertices=[]; indices=[]; findings=[]
    height_stations=[x[0] for x in heights]
    for i,(a,b) in enumerate(zip(line,line[1:])):
        length=stations[i+1]-stations[i]
        count=math.ceil(length/5)
        for step in range(count+1):
            if i and step==0: continue
            f=step/count; s=stations[i]+length*f
            hi=min(max(1,bisect.bisect_right(height_stations,s)),len(heights)-1)
            left,right=heights[hi-1],heights[hi]
            h=left[1]+(right[1]-left[1])*(s-left[0])/(right[0]-left[0])
            position=[a[0]+f*(b[0]-a[0]),h-preparation["origin_lat_lon_elevation"][2],a[1]+f*(b[1]-a[1])]
            p=dict(position=position,elevation_m=h,distance_m=s,break_before=not points,grade_percent=None)
            if points:
                prev=points[-1]; delta=s-prev["distance_m"]
                p["grade_percent"]=100*(h-prev["elevation_m"])/delta
                if abs(p["grade_percent"])>15: findings.append(dict(point=len(points),issue="grade_over_15_percent"))
                x,y,z=prev["position"]; dx,dz=position[0]-x,position[2]-z
                side=[-3*dz/delta,0,3*dx/delta]; offset=len(vertices)
                vertices.extend([[q[j]+sign*side[j] for j in range(3)] for q in (prev["position"],position) for sign in (1,-1)])
                indices.extend([offset,offset+1,offset+2,offset+1,offset+3,offset+2])
            points.append(p)
    return dict(points=points,mesh=dict(vertices=vertices,indices=indices,width_m=6),findings=findings,validated=False,
        mode="road",osm_vertices=len(line),length_m=stations[-1],ignored_nearby_elevation_anchors=ignored,access_notes=access_notes(preparation),
        elevation_method="Corrected activity heights interpolated by monotonic road distance; anchors under 5 m apart skipped, endpoints retained",
        warnings=["Source recording breaks do not interrupt connected road geometry. Not proof of real-world access or vertical accuracy."])


def build(source,osm_path,destination,mode,fallback=False,connect_gaps=False):
    raw=osm.read(source); preparation=json.loads(raw)
    if preparation.get("kind")!="openride.scenery-preparation" or preparation.get("schema_version")!=1 or preparation.get("trainer_ready") is not False:
        raise ValueError("Unsupported preparation")
    base=preparation.get("reviewed_profile",preparation.get("corrected_profile"))
    if base is None: raise ValueError("A corrected preparation is required")
    reasons=[]
    if mode=="road":
        try: profile=mixed_profile(preparation,osm.read(osm_path),connect_gaps)
        except ValueError as error:
            if not fallback: raise
            reasons.append(str(error)); profile=copy.deepcopy(base); profile["mode"]="gps"
    else: profile=copy.deepcopy(base); profile["mode"]="gps"
    profile["requested_mode"]=mode; profile["fallback_reasons"]=reasons
    preparation["route_profile"]=profile
    preparation["route_profile_source_sha256"]=hashlib.sha256(raw).hexdigest()
    preparation["files"]=[]  # This preview output uses embedded data only.
    payload=json.dumps(preparation,separators=(",",":"),allow_nan=False).encode()
    if len(payload)>16*1024*1024: raise ValueError("Output exceeds 16 MiB preview limit")
    output=Path(destination); output.mkdir()
    (output/"preparation.json").write_bytes(payload)
    return profile


if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("preparation"); parser.add_argument("new_directory")
    parser.add_argument("--mode",choices=("gps","road"),default="gps")
    parser.add_argument("--osm"); parser.add_argument("--fallback-gps",action="store_true")
    args=parser.parse_args()
    try:
        if args.mode=="road" and not args.osm: raise ValueError("Road mode requires --osm")
        p=build(args.preparation,args.osm,args.new_directory,args.mode,args.fallback_gps)
        print(json.dumps(dict(mode=p["mode"],points=len(p["points"]),length_m=p["points"][-1]["distance_m"],findings=len(p["findings"]),fallback_reasons=p["fallback_reasons"]),indent=2))
    except Exception as error: print(error,file=sys.stderr); sys.exit(1)
