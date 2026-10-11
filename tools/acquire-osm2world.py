"""Download portable OSM2World 0.4.0 and Temurin Java 17 into a new local folder."""
import argparse
import datetime
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import shutil
import stat
import sys
import urllib.request
import zipfile

OSM_URL = "https://osm2world.org/download/files/0.4.0/OSM2World-0.4.0-bin.zip"
JAVA_API = "https://api.adoptium.net/v3/assets/latest/17/hotspot?architecture=x64&image_type=jre&os=windows&vendor=eclipse"


def fetch(url, destination, limit, expected=None):
    if not url.startswith("https://"):
        raise ValueError("Downloads require HTTPS")
    digest = hashlib.sha256()
    total = 0
    request = urllib.request.Request(url, headers={"User-Agent": "OpenRide-authoring/0.1"})
    with urllib.request.urlopen(request, timeout=60) as response, destination.open("xb") as output:
        if not response.url.startswith("https://"):
            raise ValueError("Download redirected away from HTTPS")
        while True:
            chunk = response.read(1024*1024)
            if not chunk:
                break
            total += len(chunk)
            if total > limit:
                raise ValueError("Download exceeds size limit")
            digest.update(chunk)
            output.write(chunk)
    checksum = digest.hexdigest()
    if expected is not None and checksum.lower()!=expected.lower():
        raise ValueError("Published Java checksum does not match downloaded archive")
    return dict(url=url,bytes=total,sha256=checksum,published_checksum_verified=expected is not None)


def extract(archive, directory):
    directory = Path(directory).resolve()
    if directory.exists():
        raise FileExistsError(directory)
    with zipfile.ZipFile(archive) as source:
        members = source.infolist()
        if len(members)>50000 or sum(item.file_size for item in members)>2*1024**3:
            raise ValueError("Archive exceeds extraction budget")
        checked = []
        seen = set()
        for item in members:
            path = PurePosixPath(item.filename.replace("\\","/"))
            parts = path.parts
            if path.is_absolute() or not parts or any(part in ("..", ".") or ':' in part or part.endswith((' ','.')) for part in parts):
                raise ValueError("Invalid archive path")
            if stat.S_ISLNK(item.external_attr >> 16):
                raise ValueError("Archive symlinks are unsupported")
            reserved = {"CON","PRN","AUX","NUL"} | {f"{prefix}{i}" for prefix in ("COM","LPT") for i in range(1,10)}
            if any(part.split('.')[0].upper() in reserved for part in parts):
                raise ValueError("Reserved Windows archive path")
            target = directory.joinpath(*parts).resolve()
            target.relative_to(directory)
            key = str(target).casefold()
            if key in seen:
                raise ValueError("Duplicate archive path")
            seen.add(key)
            checked.append((item,target))
        directory.mkdir()
        for item,target in checked:
            if item.is_dir():
                target.mkdir(parents=True,exist_ok=True)
            else:
                target.parent.mkdir(parents=True,exist_ok=True)
                with source.open(item) as stream, target.open("xb") as output:
                    shutil.copyfileobj(stream,output,1024*1024)


def acquire(directory):
    directory = Path(directory)
    directory.mkdir()  # Never overwrite an existing tool installation.
    report = dict(schema_version=1,kind="openride.authoring-tools",complete=False,
                  acquired_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),archives={})
    try:
        print("Fetching Temurin release metadata…",flush=True)
        fetch(JAVA_API,directory/"java-release.json",4*1024**2)
        releases = json.loads((directory/"java-release.json").read_bytes())
        release = next(r for r in releases if r["binary"]["architecture"]=="x64"
                       and r["binary"]["os"]=="windows" and r["binary"]["image_type"]=="jre")
        package = release["binary"]["package"]
        if not re.fullmatch(r"[0-9a-fA-F]{64}",package["checksum"]):
            raise ValueError("Missing published Java SHA-256")
        report["java_release"] = release["release_name"]
        print("Downloading Java archive…",flush=True)
        report["archives"]["java.zip"] = fetch(package["link"],directory/"java.zip",256*1024**2,package["checksum"])
        print("Downloading OSM2World 0.4.0 (approximately 431 MB)…",flush=True)
        report["archives"]["osm2world.zip"] = fetch(OSM_URL,directory/"osm2world.zip",768*1024**2)
        print("Extracting portable tools…",flush=True)
        extract(directory/"java.zip",directory/"java")
        extract(directory/"osm2world.zip",directory/"osm2world")
        java = list((directory/"java").rglob("java.exe"))
        if len(java)!=1:
            raise ValueError("Expected exactly one Java executable")
        report["java_executable"] = java[0].relative_to(directory).as_posix()
        report["converter_jars"] = [p.relative_to(directory).as_posix() for p in (directory/"osm2world").rglob("*.jar") if "osm2world" in p.name.lower()]
        report["license_files"] = [p.relative_to(directory).as_posix() for p in (directory/"osm2world").rglob("*")
                                   if p.is_file() and any(word in p.name.lower() for word in ("license","copying","notice"))]
        report["complete"] = True
    except Exception as error:
        report["error"] = str(error)
    with (directory/"tools.json").open("x",encoding="utf-8") as file:
        json.dump(report,file,indent=2)
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("new_directory",type=Path)
    args = parser.parse_args()
    try:
        result = acquire(args.new_directory)
        print(json.dumps(result,indent=2))
        sys.exit(0 if result["complete"] else 1)
    except Exception as error:
        print(str(error),file=sys.stderr)
        sys.exit(1)
