"""Apply explicit, source-bound authoring corrections to a new offline preview bundle."""
import argparse
import copy
import hashlib
import json
import math
from pathlib import Path
import sys


def read(path):
    with Path(path).open("rb") as stream:
        raw = stream.read(16*1024*1024+1)
    if len(raw) > 16*1024*1024:
        raise ValueError("Input exceeds 16 MiB")
    return raw


def apply(preparation, corrections, source_hash):
    if preparation.get("kind") != "openride.scenery-preparation" or preparation.get("schema_version") != 1 or preparation.get("trainer_ready") is not False or "corrections" in preparation:
        raise ValueError("Expected an original, uncorrected preparation")
    if corrections.get("kind") != "openride.scenery-corrections" or corrections.get("schema_version") != 1:
        raise ValueError("Unsupported corrections")
    if corrections.get("preparation_sha256") != source_hash:
        raise ValueError("Source conflict: preparation changed; review choices against the new sources")
    offset = corrections.get("elevation_offset_m")
    if type(offset) not in (int, float) or not math.isfinite(offset) or abs(offset) > 100:
        raise ValueError("Elevation offset must be finite and within +/-100 m")
    choices = corrections.get("road_choices")
    if not isinstance(choices, list) or len(choices) > len(preparation["points"]):
        raise ValueError("Invalid road choices")
    matches = {m["source_sample"]:m["candidates"] for m in preparation.get("osm_context", {}).get("matches", [])}
    samples = {p["source_sample"] for p in preparation["points"]}
    selected = {}
    for choice in choices:
        sample = choice["source_sample"]
        if sample not in samples or sample in selected:
            raise ValueError("Unknown or duplicate source sample")
        candidates = [c for c in matches.get(sample, []) if c["way_id"] == choice["way_id"] and c["node_ids"] == choice["node_ids"]]
        if len(candidates) != 1 or "projected_xz" not in candidates[0]:
            raise ValueError(f"Candidate conflict at sample {sample}; regenerate OSM context")
        selected[sample] = candidates[0]
    points = copy.deepcopy(preparation["points"])
    vertices, indices, findings = [], [], []
    distance = 0.0
    origin = preparation["origin_lat_lon_elevation"]
    for i, point in enumerate(points):
        position = point["position"]
        candidate = selected.get(point["source_sample"])
        if candidate:
            position[0], position[2] = candidate["projected_xz"]
        if position[1] is not None:
            position[1] += offset
            point["elevation_m"] += offset
        point["latitude"] = origin[0]-math.degrees(position[2]/6371000)
        point["longitude"] = origin[1]+math.degrees(position[0]/(6371000*math.cos(math.radians(origin[0]))))
        point["grade_percent"] = None
        point.pop("raw_grade_percent", None)
        if i:
            a, b = points[i-1]["position"], position
            length = math.hypot(b[0]-a[0], b[2]-a[2])
            # Breaks, missing heights and large jumps never acquire rideable geometry/grade.
            valid = not point["break_before"] and 1 <= length <= 100 and a[1] is not None and b[1] is not None
            if valid:
                distance += length
                point["grade_percent"] = 100*(b[1]-a[1])/length
                side = [-3*(b[2]-a[2])/length, 0, 3*(b[0]-a[0])/length]
                base = len(vertices)
                vertices.extend([[p[j]+sign*side[j] for j in range(3)] for p in (a,b) for sign in (1,-1)])
                indices.extend([base,base+1,base+2,base+1,base+3,base+2])
                if abs(point["grade_percent"]) > 15:
                    findings.append(dict(source_sample=point["source_sample"], issue="corrected_grade_over_15_percent"))
            else:
                findings.append(dict(source_sample=point["source_sample"], issue="disconnected_or_invalid_segment"))
        point["distance_m"] = distance
    return dict(points=points, mesh=dict(vertices=vertices, indices=indices, width_m=6), findings=findings,
                distance_meaning="Sum of valid connected horizontal segments; breaks excluded",
                validated=False)


def build(preparation_path, corrections_path, output_path):
    source = Path(preparation_path)
    raw = read(source)
    preparation = json.loads(raw)
    corrections_raw = read(corrections_path)
    corrections = json.loads(corrections_raw)
    profile = apply(preparation, corrections, hashlib.sha256(raw).hexdigest())
    assets = {}
    for entry in preparation["files"]:
        name = entry["path"]
        if Path(name).name != name or ":" in name or "\\" in name or name in (".", "..", "preparation.json", "corrections.json"):
            raise ValueError("Unsafe asset name")
        content = read(source.parent/name)
        if hashlib.sha256(content).hexdigest() != entry["sha256"]:
            raise ValueError(f"Asset checksum mismatch: {name}")
        assets[name] = content
    preparation["corrections"] = corrections
    preparation["corrected_profile"] = profile
    assets["corrections.json"] = corrections_raw
    preparation["files"].append(dict(path="corrections.json", sha256=hashlib.sha256(corrections_raw).hexdigest()))
    assets["preparation.json"] = json.dumps(preparation, separators=(",", ":"), allow_nan=False).encode()
    if len(assets["preparation.json"]) > 16*1024*1024:
        raise ValueError("Output exceeds preview limit")
    output = Path(output_path)
    output.mkdir()
    for name, content in assets.items():
        (output/name).write_bytes(content)
    return len(profile["points"])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("preparation")
    parser.add_argument("corrections")
    parser.add_argument("new_directory")
    args = parser.parse_args()
    try:
        print(f"Prepared {build(args.preparation, args.corrections, args.new_directory)} corrected points; unvalidated, trainer_ready=false")
    except Exception as error:
        print(error, file=sys.stderr)
        sys.exit(1)
