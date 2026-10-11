"""Apply explicit short-span replacements to an offline corrected profile for review."""
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


def repair(preparation, proposal, source_hash):
    if preparation.get("kind") != "openride.scenery-preparation" or preparation.get("trainer_ready") is not False or "reviewed_profile" in preparation:
        raise ValueError("Expected unreviewed corrected preparation")
    if proposal.get("kind") != "openride.profile-repairs" or proposal.get("schema_version") != 1:
        raise ValueError("Unsupported repair document")
    if proposal.get("preparation_sha256") != source_hash:
        raise ValueError("Source conflict: corrected preparation changed")
    points = copy.deepcopy(preparation["corrected_profile"]["points"])
    lookup = {p["source_sample"]:i for i,p in enumerate(points)}
    choices = {c["source_sample"]:c for c in preparation["corrections"]["road_choices"]}
    spans = proposal.get("spans")
    if not isinstance(spans, list):
        raise ValueError("Explicit repair span list required")
    removed, joined, reports, occupied = set(), set(), [], []
    for span in spans:
        method=span.get('method')
        if method not in ('replace_with_chord','manual_remove','trim_start','trim_end') or not isinstance(span.get("reason"), str) or not span["reason"].strip():
            raise ValueError("Repair needs an explicit method and reason")
        start, end = lookup.get(span.get("start_source_sample")), lookup.get(span.get("end_source_sample"))
        if start is None or end is None or end <= start:
            raise ValueError("Repair endpoints must enclose existing samples")
        indices = set(range(start,end+1))
        if any(max(start,a)<min(end,b) for a,b in occupied):
            raise ValueError("Overlapping repairs are unsupported")
        occupied.append((start,end))
        if method in ('trim_start','trim_end'):
            if (method=='trim_start' and start!=0) or (method=='trim_end' and end!=len(points)-1):raise ValueError('Trim must touch the route endpoint')
            cut=range(start,end) if method=='trim_start' else range(start+1,end+1)
            removed.update(cut)
            reports.append(dict(start_source_sample=points[start]['source_sample'],end_source_sample=points[end]['source_sample'],removed_source_samples=[points[i]['source_sample'] for i in cut],replaced_breaks=[],connector_length_m=0,max_deviation_m=0,reason=span['reason'],method=method))
            continue
        if end<=start+1:raise ValueError('Repair endpoints must enclose existing samples')
        segment = points[start:end+1]
        a, b = segment[0]["position"], segment[-1]["position"]
        if any(type(v) not in (int,float) or not math.isfinite(v) for p in segment for v in p["position"]):
            raise ValueError("Cannot repair missing or nonfinite coordinates")
        length = math.hypot(b[0]-a[0],b[2]-a[2])
        if not 1 <= length <= 100:
            raise ValueError("Repair connector must be 1..100 metres")
        anchors=(segment[0],segment[-1]) if method=='manual_remove' else segment
        way_ids = {choices.get(p["source_sample"], {}).get("way_id") for p in anchors}
        if method!='manual_remove' and (None in way_ids or len(way_ids)!=1):
            raise ValueError("Repair requires explicit choices on one OSM way")
        deviations = []
        for point in segment:
            x, _, z = point["position"]
            f = max(0,min(1,((x-a[0])*(b[0]-a[0])+(z-a[2])*(b[2]-a[2]))/length**2))
            deviations.append(math.hypot(x-a[0]-f*(b[0]-a[0]),z-a[2]-f*(b[2]-a[2])))
        limit=25 if method=='manual_remove' else 10
        if max(deviations) > limit:
            raise ValueError(f'Repair exceeds {limit} metre deviation; review a smaller span')
        removed.update(range(start+1,end))
        joined.add(end)
        reports.append(dict(start_source_sample=segment[0]["source_sample"], end_source_sample=segment[-1]["source_sample"],
            removed_source_samples=[p["source_sample"] for p in segment[1:-1]],
            replaced_breaks=[p["source_sample"] for p in segment[1:] if p["break_before"]],
            connector_length_m=length, max_deviation_m=max(deviations), reason=span["reason"], method=span["method"]))
    result = []
    if len(points)-len(removed)<2:raise ValueError('Keep at least two route points')
    for index, point in enumerate(points):
        if index in removed:
            continue
        if index in joined:
            point["break_before"] = False
            point["repair_connector"] = True
        result.append(point)
    result[0]['break_before']=True
    vertices, indices, findings = [], [], []
    distance = 0
    for i, point in enumerate(result):
        point["grade_percent"] = None
        if i:
            a,b = result[i-1]["position"],point["position"]
            length = math.hypot(b[0]-a[0],b[2]-a[2])
            if point["break_before"] or not 1 <= length <= 100 or a[1] is None or b[1] is None:
                findings.append(dict(source_sample=point["source_sample"], issue="disconnected_or_invalid_segment"))
            else:
                distance += length
                point["grade_percent"] = 100*(b[1]-a[1])/length
                if abs(point["grade_percent"]) > 15:
                    findings.append(dict(source_sample=point["source_sample"], issue="grade_over_15_percent"))
                side = [-3*(b[2]-a[2])/length,0,3*(b[0]-a[0])/length]
                base = len(vertices)
                vertices.extend([[p[j]+sign*side[j] for j in range(3)] for p in (a,b) for sign in (1,-1)])
                indices.extend([base,base+1,base+2,base+1,base+3,base+2])
        point["distance_m"] = distance
    return dict(points=result, mesh=dict(vertices=vertices,indices=indices,width_m=6), findings=findings,
                repairs=reports, validated=False, distance_meaning="Sum of valid connected horizontal segments; breaks excluded")


def build(source, proposal_path, destination):
    source = Path(source)
    raw, proposal_raw = read(source),read(proposal_path)
    preparation, proposal = json.loads(raw),json.loads(proposal_raw)
    profile = repair(preparation,proposal,hashlib.sha256(raw).hexdigest())
    assets = {}
    for entry in preparation["files"]:
        name = entry["path"]
        if Path(name).name != name or ":" in name or "\\" in name or name in (".","..","preparation.json","profile-repairs.json"):
            raise ValueError("Unsafe asset name")
        content = read(source.parent/name)
        if hashlib.sha256(content).hexdigest() != entry["sha256"]:
            raise ValueError("Asset checksum mismatch: "+name)
        assets[name] = content
    preparation["reviewed_profile"] = profile
    preparation["profile_repairs"] = proposal
    assets["profile-repairs.json"] = proposal_raw
    preparation["files"].append(dict(path="profile-repairs.json",sha256=hashlib.sha256(proposal_raw).hexdigest()))
    assets["preparation.json"] = json.dumps(preparation,separators=(",",":"),allow_nan=False).encode()
    if len(assets["preparation.json"]) > 16*1024*1024:
        raise ValueError("Output exceeds 16 MiB")
    output = Path(destination)
    output.mkdir()
    for name,content in assets.items():
        (output/name).write_bytes(content)
    return profile


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("preparation")
    parser.add_argument("repairs")
    parser.add_argument("new_directory")
    args = parser.parse_args()
    try:
        profile = build(args.preparation,args.repairs,args.new_directory)
        print(json.dumps(dict(points=len(profile["points"]), findings=profile["findings"],repairs=profile["repairs"]),indent=2))
    except Exception as error:
        print(error,file=sys.stderr)
        sys.exit(1)
