"""Detect bounded stationary/backtracking clusters; emit review proposals, never apply them."""
import argparse
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import statistics
import sys

spec = importlib.util.spec_from_file_location("repair_profile",Path(__file__).with_name("repair-profile.py"))
repair_module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(repair_module)


def distance(a,b):
    return math.hypot(a[0]-b[0],a[2]-b[2])


def detect(preparation, evidence, preparation_hash):
    if evidence.get("kind") != "openride.activity-evidence" or evidence.get("schema_version") != 1 or evidence.get("source_activity_sha256") != preparation.get("source_activity_sha256"):
        raise ValueError("Activity evidence source conflict")
    if "reviewed_profile" in preparation or "corrected_profile" not in preparation:
        raise ValueError("Detection requires corrected, unrepaired preparation")
    points = preparation["points"]
    if len(points)>10000:
        raise ValueError("Detection is limited to 10,000 preparation points")
    samples = {s["source_sample"]:s for s in evidence["samples"]}
    signals = []
    for point in points:
        s = samples.get(point["source_sample"])
        if s is None or any(type(s.get(key)) not in (int,float) or not math.isfinite(s[key]) or abs(s[key]-point[key])>1e-10 for key in ("latitude","longitude")):
            raise ValueError("Activity sample mapping conflict")
        signals.append(s)
    proposals, rejected = [], []
    start = 1
    while start < len(points)-2:
        # Bound work and avoid swallowing whole rides with repeated laps.
        end = start
        for j in range(start+1,min(len(points)-1,start+300)):
            if distance(points[start]["position"],points[j]["position"])>10:
                break
            end = j
        if end-start < 4:
            start += 1
            continue
        times = [s.get("time_ms") for s in signals[start:end+1]]
        if any(type(t) not in (int,float) or not math.isfinite(t) for t in times) or any(b<=a for a,b in zip(times,times[1:])):
            start += 1
            continue
        duration = (times[-1]-times[0])/1000
        steps = [distance(a["position"],b["position"]) for a,b in zip(points[start:end],points[start+1:end+1])]
        intervals = [(b-a)/1000 for a,b in zip(times,times[1:])]
        path = sum(steps)
        net = distance(points[start]["position"],points[end]["position"])
        speeds = [d/t for d,t in zip(steps,intervals) if t<=60]
        recorded = [s.get("speed_mps") for s in signals[start:end+1] if s.get("speed_mps") is not None]
        if any(type(v) not in (int,float) or not math.isfinite(v) or v<0 for v in recorded):
            raise ValueError("Invalid recorded speed")
        if duration<30 or path<5 or path/max(net,1)<2 or len(speeds)<3 or statistics.median(speeds)>0.5:
            start += 1
            continue
        # Nonzero recorded motion can indicate a real small loop rather than a stop.
        moving = bool(recorded) and sum(v>1.5 for v in recorded)/len(recorded)>0.2
        gaps = sum(t>60 for t in intervals)
        report = dict(cluster_start_source_sample=points[start]["source_sample"],cluster_end_source_sample=points[end]["source_sample"],
            duration_s=duration, gps_path_m=path,net_progress_m=net,median_gps_speed_mps=statistics.median(speeds),
            recorded_speed_samples=len(recorded),recorded_speed_median_mps=statistics.median(recorded) if recorded else None,
            recording_gaps_over_60s=gaps, confidence="ambiguous_motion" if moving else "stop_candidate_with_recording_gaps" if gaps else "stop_candidate",
            requires_review=True)
        span = dict(start_source_sample=points[start-1]["source_sample"],end_source_sample=points[end+1]["source_sample"],
            method="replace_with_chord",reason="Reviewed stationary-cluster candidate; GPS-derived motion is not wheel speed.")
        proposal = dict(kind="openride.profile-repairs",schema_version=1,preparation_sha256=preparation_hash,spans=[span])
        try:
            if moving:
                raise ValueError("Recorded movement conflicts with stationary interpretation")
            profile = repair_module.repair(preparation,proposal,preparation_hash)
            report.update(span=span,preview=profile)
            proposals.append(report)
            start = end+2  # Keep independently selectable repair spans disjoint.
        except ValueError as error:
            report["reason"] = str(error)
            rejected.append(report)
            start = end+1
    return dict(kind="openride.jitter-review",schema_version=1,preparation_sha256=preparation_hash,
        source_activity_sha256=evidence["source_activity_sha256"],
        parameters=dict(radius_m=10,min_duration_s=30,min_samples=5,min_path_m=5,min_path_to_net_ratio=2,
                        max_median_gps_speed_mps=0.5,max_cluster_samples=300,recording_gap_s=60),
        evidence=signals,candidates=proposals,rejected=rejected,
        limitations="Review only. Sparse recordings, deliberate tight loops and stops cannot always be distinguished. No pause-event or wheel-sensor identity inferred.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("preparation")
    parser.add_argument("activity_evidence")
    parser.add_argument("new_report")
    args = parser.parse_args()
    try:
        raw = repair_module.read(args.preparation)
        result = detect(json.loads(raw),json.loads(repair_module.read(args.activity_evidence)),hashlib.sha256(raw).hexdigest())
        output = json.dumps(result,separators=(",",":"),allow_nan=False)
        if len(output.encode())>16*1024*1024:
            raise ValueError("Review report exceeds 16 MiB")
        with Path(args.new_report).open("x",encoding="utf-8") as stream:
            stream.write(output)
        print(json.dumps({"candidates":len(result["candidates"]),"rejected":len(result["rejected"]),"evidence_samples":len(result["evidence"]) }))
    except Exception as error:
        print(error,file=sys.stderr)
        sys.exit(1)
