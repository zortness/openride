"""Run the saved RPV OSM through a portable OSM2World installation and record evidence."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import sys
import time


def sha(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024*1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def run(tools, sources, output):
    tools, sources, output = (Path(p).resolve() for p in (tools,sources,output))
    installation = json.loads((tools/"tools.json").read_bytes())
    manifest = json.loads((sources/"sources.json").read_bytes())
    if not installation.get("complete") or not manifest.get("complete"):
        raise ValueError("Require complete tool installation and source snapshot")
    java = (tools/installation["java_executable"]).resolve()
    java.relative_to(tools)
    jar = tools/"osm2world/OSM2World.jar"
    map_path = sources/"map.osm"
    input_hash = sha(map_path)
    if input_hash!=manifest["files"]["map.osm"]["sha256"]:
        raise ValueError("OSM source checksum mismatch")
    output.mkdir()
    arguments = [str(java),"-Dfile.encoding=UTF-8","-Xmx2G","-jar",str(jar),
        "--input",str(map_path),"--output",str(output/"rpv.glb"),"--lod","2",
        "--config",str(jar.parent/"standard.properties"),"--logDir",str(output)]
    report = dict(schema_version=1,kind="openride.converter-comparison",input_sha256=input_hash,
        tool_installation=installation,jar_sha256=sha(jar),
        config_sha256=sha(jar.parent/"standard.properties"),command=arguments,
        working_directory=str(jar.parent),usgs_elevation_supplied=False)
    flags = subprocess.CREATE_NO_WINDOW if sys.platform=="win32" else 0
    start = time.perf_counter()
    try:
        with (output/"stdout.log").open("xb") as stdout, (output/"stderr.log").open("xb") as stderr:
            process = subprocess.run(arguments,cwd=jar.parent,stdout=stdout,stderr=stderr,
                                     timeout=300,creationflags=flags)
        report["exit_code"] = process.returncode
        report["wall_seconds"] = time.perf_counter()-start
        model = output/"rpv.glb"
        blob = model.read_bytes()
        magic,version,total = struct.unpack_from("<III",blob)
        length,kind = struct.unpack_from("<II",blob,12)
        if magic!=0x46546C67 or version!=2 or total!=len(blob) or kind!=0x4E4F534A or 20+length>len(blob):
            raise ValueError("Invalid GLB header or JSON chunk")
        gltf = json.loads(blob[20:20+length].decode("utf-8"))
        report["model"] = dict(bytes=len(blob),sha256=sha(model),
            **{key:len(gltf.get(key,[])) for key in ("meshes","materials","images","textures","nodes")})
        report["model"]["triangles"] = sum(gltf["accessors"][primitive.get("indices",primitive["attributes"]["POSITION"])]["count"]//3
            for mesh in gltf["meshes"] for primitive in mesh["primitives"] if primitive.get("mode",4)==4)
        report["model"]["external_images"] = sum("uri" in image for image in gltf.get("images",[]))
        logs = list(output.glob("osm2world_log*.json"))
        report["converter_metrics"] = json.loads(logs[0].read_bytes()) if len(logs)==1 else None
        errors = []
        for log in output.glob("*.txt.gz"):
            text = gzip.decompress(log.read_bytes()).decode("utf-8")
            errors.extend(line for line in text.splitlines() if line.startswith("ERROR:"))
        report["converter_error_count"] = len(errors)
        report["converter_error_headings"] = sorted(set(errors))
        report["source_unchanged"] = sha(map_path)==input_hash
        report["status"] = "converted_with_errors" if errors else "converted"
        if process.returncode!=0 or not report["source_unchanged"]:
            report["status"] = "failed"
    except Exception as error:
        report["status"] = "failed"
        report["error"] = str(error)
    (output/"comparison.json").write_text(json.dumps(report,indent=2),encoding="utf-8")
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tools_directory")
    parser.add_argument("sources_directory")
    parser.add_argument("new_output_directory")
    args = parser.parse_args()
    try:
        report = run(args.tools_directory,args.sources_directory,args.new_output_directory)
        print(json.dumps(report,indent=2))
        sys.exit(0 if report["status"]=="converted" else 1)
    except Exception as error:
        print(str(error),file=sys.stderr)
        sys.exit(1)
