"""Build a self-contained, data-only M3 preview package (directory, not executable pack)."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

MAX_BYTES = 16*1024*1024


def read(path, limit=MAX_BYTES):
    with Path(path).open("rb") as stream:
        raw = stream.read(limit+1)
    if len(raw)>limit:
        raise ValueError("File exceeds package size limit")
    return raw


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def build(source, destination, package_id, version):
    if not package_id or any(c not in "abcdefghijklmnopqrstuvwxyz0123456789-_" for c in package_id):
        raise ValueError("Package ID must use lowercase letters, digits, - or _")
    if not version or len(version)>64:
        raise ValueError("Invalid content version")
    source_raw = read(source)
    preparation = json.loads(source_raw)
    if preparation.get("kind")!="openride.scenery-preparation" or preparation.get("schema_version")!=1 or preparation.get("trainer_ready") is not False:
        raise ValueError("Expected supported scenery preparation")
    profile_name = "route_profile" if "route_profile" in preparation else "reviewed_profile" if "reviewed_profile" in preparation else "corrected_profile" if "corrected_profile" in preparation else "points"
    profile = preparation.get(profile_name)
    snapshot = preparation.get("terrain",{}).get("source_snapshot",{})
    attribution = dict(kind="openride.content-attribution",schema_version=1,
        sources=snapshot.get("attribution",{}),
        osm=preparation.get("osm_context",{}).get("attribution"),
        notice="Original application code license does not cover geographic content. OSM data: OpenStreetMap contributors, ODbL 1.0. Terrain source terms remain separate.")
    if not attribution["sources"]:
        raise ValueError("Source attribution is required for geographic packaging")
    # Preview geometry is already embedded; authoring OBJ/query/report sidecars are not runtime dependencies.
    preparation["files"] = []
    payload = json.dumps(preparation,separators=(",",":"),ensure_ascii=False,allow_nan=False).encode()
    credits = json.dumps(attribution,indent=2,ensure_ascii=False,allow_nan=False).encode()
    if len(payload)>MAX_BYTES or len(credits)>65536:
        raise ValueError("Package payload exceeds limits")
    manifest = dict(kind="openride.preview-package",schema_version=1,package_id=package_id,content_version=version,
        name=preparation["name"],required_capabilities=["scenery-preparation-v1"],dependencies=[],
        trainer_ready=False,review_status="authoring-preview-unvalidated",profile=profile_name,
        profile_sha256=sha(json.dumps(profile,sort_keys=True,separators=(",",":"),allow_nan=False).encode()),
        source_preparation_sha256=sha(source_raw),source_route_sha256=preparation["source_route_sha256"],
        bounds=preparation["acquisition_bounds"],origin_lat_lon_elevation=preparation["origin_lat_lon_elevation"],
        files=[dict(path="preparation.json",sha256=sha(payload),bytes=len(payload)),dict(path="attribution.json",sha256=sha(credits),bytes=len(credits))])
    directory = Path(destination)
    directory.mkdir(parents=False)
    (directory/"preparation.json").write_bytes(payload)
    (directory/"attribution.json").write_bytes(credits)
    # Discovery never selects a directory before this completion marker is present and verified.
    (directory/"manifest.json").write_text(json.dumps(manifest,indent=2),encoding="utf-8")
    return manifest


if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("preparation")
    parser.add_argument("new_directory")
    parser.add_argument("--id",required=True)
    parser.add_argument("--version",default="0.1.0")
    args=parser.parse_args()
    try:
        manifest=build(args.preparation,args.new_directory,args.id,args.version)
        print(json.dumps({k:manifest[k] for k in ("package_id","content_version","profile","review_status","files")},indent=2))
    except Exception as error:
        print(error,file=sys.stderr)
        sys.exit(1)
