"""Offline OSM context and nearest-road diagnostics; never snaps or corrects a route."""
import math
import statistics
import xml.etree.ElementTree as ET
import argparse
import hashlib
import json
from pathlib import Path
import sys

EARTH_M = 6371000.0
HIGHWAYS = {"motorway", "motorway_link", "trunk", "trunk_link", "primary", "primary_link",
            "secondary", "secondary_link", "tertiary", "tertiary_link", "residential",
            "unclassified", "service", "living_street", "cycleway", "path", "footway", "pedestrian", "track", "steps"}


def local(lat, lon, origin):
    return (EARTH_M * math.radians(lon-origin[1]) * math.cos(math.radians(origin[0])),
            -EARTH_M * math.radians(lat-origin[0]))


def nearest(point, start, end):
    dx, dz = end[0]-start[0], end[1]-start[1]
    squared = dx*dx+dz*dz
    if squared < 0.0001:
        return None
    fraction = max(0.0, min(1.0, ((point[0]-start[0])*dx+(point[1]-start[1])*dz)/squared))
    distance = math.hypot(point[0]-start[0]-fraction*dx, point[1]-start[1]-fraction*dz)
    return distance, fraction


def analyze(preparation, osm_bytes):
    if len(osm_bytes) > 64*1024*1024 or b"<!DOCTYPE" in osm_bytes.upper() or b"<!ENTITY" in osm_bytes.upper():
        raise ValueError("OSM input exceeds limits or contains unsupported XML declarations")
    root = ET.fromstring(osm_bytes)
    if root.tag != "osm" or root.find("remark") is not None:
        raise ValueError("Not a complete OSM response")
    origin = preparation["origin_lat_lon_elevation"]
    nodes = {}
    for node in root.findall("node"):
        lat, lon = float(node.attrib["lat"]), float(node.attrib["lon"])
        if not (math.isfinite(lat) and math.isfinite(lon) and -90 <= lat <= 90 and -180 <= lon <= 180):
            raise ValueError("Invalid OSM node coordinate")
        if node.attrib["id"] in nodes:
            raise ValueError("Duplicate OSM node")
        nodes[node.attrib["id"]] = (lat, lon)
    ways = []
    for way in root.findall("way"):
        tags = {tag.attrib["k"]: tag.attrib["v"] for tag in way.findall("tag")}
        if tags.get("highway") not in HIGHWAYS:
            continue
        references = [ref.attrib["ref"] for ref in way.findall("nd")]
        missing = [ref for ref in references if ref not in nodes]
        if missing:
            raise ValueError(f"Highway way {way.attrib['id']} references missing node {missing[0]}")
        positions = [local(*nodes[ref], origin) for ref in references]
        ways.append(dict(way_id=way.attrib["id"], tags=tags, references=references, positions=positions))
    matches = []
    for point in preparation["points"]:
        position = local(point["latitude"], point["longitude"], origin)
        candidates = []
        for way in ways:
            best = None
            for index, (a,b) in enumerate(zip(way["positions"], way["positions"][1:])):
                result = nearest(position,a,b)
                if result is not None and (best is None or result[0] < best[0]):
                    best = (*result,index)
            if best is None:
                continue
            distance, fraction, index = best
            if distance > 30:
                continue
            tags = way["tags"]
            a, b = way["positions"][index:index+2]
            candidates.append(dict(way_id=way["way_id"], name=tags.get("name"), highway=tags["highway"],
                projected_xz=[a[0]+fraction*(b[0]-a[0]), a[1]+fraction*(b[1]-a[1])],
                distance_m=distance, fraction=fraction, node_ids=way["references"][index:index+2],
                tags={key:tags[key] for key in ("bridge","tunnel","layer","access","bicycle","oneway","surface") if key in tags}))
        candidates.sort(key=lambda c:(c["distance_m"],c["way_id"]))
        # Candidates from distinct OSM ways can still belong to the same physical street.
        ambiguous = len(candidates)>1 and candidates[1]["distance_m"]-candidates[0]["distance_m"] <= 5
        matches.append(dict(source_sample=point["source_sample"],
            status="unmatched" if not candidates else "ambiguous" if ambiguous else "candidate",
            candidates=candidates))
    distances = [m["candidates"][0]["distance_m"] for m in matches if m["candidates"]]
    return dict(schema_version=1, candidate_version=2, kind="openride.osm-context", matches=matches,
        summary=dict(highway_ways=len(ways), samples=len(matches),
            matched=sum(bool(m["candidates"]) for m in matches), ambiguous=sum(m["status"]=="ambiguous" for m in matches),
            median_nearest_distance_m=statistics.median(distances) if distances else None,
            max_nearest_distance_m=max(distances) if distances else None),
        roads=ways, warnings=["Nearest geometry only: not a connected path match, direction/access decision, or correction.",
            "30 m search radius; alternatives within 5 m of best are flagged. Split ways and junctions may be ambiguous.",
            "Present-day OSM may differ from the historical activity. No route points or elevations changed."])


def terrain_sampler(preparation):
    terrain = preparation["terrain"]
    extent = terrain["extent"]
    width, height = terrain["width"], terrain["height"]
    vertices = terrain["mesh"]["vertices"]
    valid = set(terrain["mesh"]["indices"])
    origin = preparation["origin_lat_lon_elevation"]
    dx = (extent["xmax"]-extent["xmin"])/width
    dy = (extent["ymax"]-extent["ymin"])/height

    def sample(x,z):
        lon = origin[1]+math.degrees(x/(EARTH_M*math.cos(math.radians(origin[0]))))
        lat = origin[0]-math.degrees(z/EARTH_M)
        col, row = (lon-extent["xmin"])/dx-0.5, (extent["ymax"]-lat)/dy-0.5
        if not (0<=col<=width-1 and 0<=row<=height-1):
            return None
        c, r = min(int(col),width-2), min(int(row),height-2)
        i = r*width+c
        if any(v not in valid for v in (i,i+1,i+width,i+width+1)):
            return None
        fx, fy = col-c, row-r
        return ((vertices[i][1]*(1-fx)+vertices[i+1][1]*fx)*(1-fy)
                +(vertices[i+width][1]*(1-fx)+vertices[i+width+1][1]*fx)*fy)
    return sample


def context_lines(preparation, roads):
    sample = terrain_sampler(preparation)
    lines = []
    structures = []
    for road in roads:
        if any(road["tags"].get(key,"no") not in ("no","0") for key in ("bridge","tunnel")):
            structures.append(road["way_id"])
            continue  # Bare-earth draping is not valid for these structures.
        for a,b in zip(road["positions"],road["positions"][1:]):
            length = math.hypot(b[0]-a[0],b[1]-a[1])
            if length > 10000:
                continue
            steps = max(1,math.ceil(length/10))
            previous = None
            for step in range(steps+1):
                f = step/steps
                x,z = a[0]+(b[0]-a[0])*f, a[1]+(b[1]-a[1])*f
                y = sample(x,z)
                current = None if y is None else [x,y+0.5,z]
                if previous is not None and current is not None:
                    lines.extend([previous,current])
                    if len(lines)>200000:
                        raise ValueError("OSM preview exceeds 200,000 line vertices")
                previous = current
    return dict(vertices=lines, omitted_structure_ways=structures,
                meaning="OSM centrelines draped on DEM, +0.5 m display offset; not corrected road surfaces")


def read(path):
    with Path(path).open("rb") as stream:
        raw = stream.read(64*1024*1024+1)
    if len(raw)>64*1024*1024:
        raise ValueError("File exceeds 64 MiB")
    return raw


def build(preparation_path, source_directory, new_directory):
    preparation_path, sources, output = map(Path,(preparation_path,source_directory,new_directory))
    preparation_bytes = read(preparation_path)
    preparation = json.loads(preparation_bytes)
    manifest = json.loads(read(sources/"sources.json"))
    if preparation.get("kind")!="openride.scenery-preparation" or preparation.get("schema_version")!=1 or preparation.get("trainer_ready") is not False:
        raise ValueError("Unsupported preparation")
    if manifest.get("kind")!="openride.geographic-sources" or manifest.get("schema_version")!=1 or preparation.get("terrain",{}).get("source_snapshot")!=manifest:
        raise ValueError("OSM sources must match the terrain's source snapshot")
    osm = read(sources/"map.osm")
    if hashlib.sha256(osm).hexdigest()!=manifest["files"]["map.osm"]["sha256"]:
        raise ValueError("OSM source checksum mismatch")
    context = analyze(preparation,osm)
    roads = context.pop("roads")
    context["display_lines"] = context_lines(preparation,roads)
    context["source_sha256"] = hashlib.sha256(osm).hexdigest()
    context["input_preparation_sha256"] = hashlib.sha256(preparation_bytes).hexdigest()
    context["attribution"] = manifest["attribution"]["osm"]
    report = {key:value for key,value in context.items() if key!="display_lines"}
    assets = {}
    for name in ("road-preview.obj","bounds.geojson","osm-query.overpass"):
        content = read(preparation_path.parent/name)
        expected = next(a["sha256"] for a in preparation["files"] if a["path"]==name)
        if hashlib.sha256(content).hexdigest()!=expected:
            raise ValueError(f"Asset checksum mismatch: {name}")
        assets[name] = content
    assets["osm-report.json"] = json.dumps(report,indent=2,ensure_ascii=False,allow_nan=False).encode()
    preparation["osm_context"] = context
    preparation["files"] = [entry for entry in preparation["files"] if entry["path"]!="osm-report.json"]
    preparation["files"].append(dict(path="osm-report.json",sha256=hashlib.sha256(assets["osm-report.json"]).hexdigest()))
    assets["preparation.json"] = json.dumps(preparation,separators=(",",":"),ensure_ascii=False,allow_nan=False).encode()
    if len(assets["preparation.json"])>16*1024*1024:
        raise ValueError("Combined preview exceeds 16 MiB")
    output.mkdir()
    for name,content in assets.items():
        with (output/name).open("xb") as file:
            file.write(content)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("preparation")
    parser.add_argument("source_directory")
    parser.add_argument("new_directory")
    args = parser.parse_args()
    try:
        report = build(args.preparation,args.source_directory,args.new_directory)
        print(json.dumps(report["summary"],indent=2))
        return 0
    except Exception as error:
        print(str(error),file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
