"""Acquire bounded authoring sources. Python standard library only; no rider dependency."""
import argparse
import datetime
import hashlib
import http.client
import json
import math
from pathlib import Path
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import xml.etree.ElementTree as ET

USGS = "https://elevation.nationalmap.gov/arcgis/rest/services/3DEPElevation/ImageServer"
OVERPASS = "https://overpass-api.de/api/interpreter"
LIMIT = 64 * 1024 * 1024


def checked_json(raw):
    value = json.loads(raw)
    if not isinstance(value, dict) or "error" in value:
        raise ValueError(f"Service returned an error: {value}")
    return value


def download(url, fields=None):
    print('Download: '+url.split('?')[0],flush=True)
    body = urllib.parse.urlencode(fields).encode() if fields is not None else None
    request = urllib.request.Request(url, data=body, headers={"User-Agent": "OpenRide-authoring/0.1"})
    for attempt in range(3):
        try:
            with urllib.request.urlopen(request, timeout=60) as response:
                raw = response.read(LIMIT + 1)
            if len(raw) > LIMIT:
                raise ValueError("Response exceeds 64 MiB")
            return raw
        except (TimeoutError, urllib.error.URLError, http.client.IncompleteRead) as error:
            if isinstance(error, urllib.error.HTTPError):
                error.close()
                if error.code not in (408,429,500,502,503,504): raise
            if attempt == 2: raise
            delay = 2 ** (attempt + 1)
            print(f'Temporary download failure: {error}; retrying same request in {delay}s',flush=True)
            time.sleep(delay)


def plan(preparation):
    if preparation.get("kind") != "openride.scenery-preparation" or preparation.get("schema_version") != 1:
        raise ValueError("Unsupported preparation format")
    b = preparation["acquisition_bounds"]
    s, w, n, e = (b[key] for key in ("south", "west", "north", "east"))
    if any(isinstance(v, bool) or not isinstance(v, (int, float)) or not math.isfinite(v) for v in (s, w, n, e)):
        raise ValueError("Non-finite bounds")
    if not (-80 < s < n < 80 and -180 <= w < e <= 180 and n-s <= 0.1 and e-w <= 0.15):
        raise ValueError("Expected a small local section without antimeridian crossing")
    bbox = ",".join(str(v) for v in (w, s, e, n))
    # Raw float elevations, not the service's default hillshade visualization.
    raster = dict(f="json", bbox=bbox, bboxSR="4326", imageSR="4326", size="256,256",
                  format="tiff", pixelType="F32", compression="LZ77", adjustAspectRatio="false",
                  interpolation="RSP_BilinearInterpolation", renderingRule='{"rasterFunction":"None"}')
    osm_bbox = ",".join(str(v) for v in (s, w, n, e))
    # Preserve relevant multipolygon members and tagged vegetation nodes as well as roads.
    query = '[out:xml][timeout:60];\n(\n' + "\n".join(
        f'nwr["{key}"]({osm_bbox});' for key in ("highway", "building", "landuse", "natural", "waterway")
    ) + '\n);\n(._;>>;);\nout meta;\n'
    return raster, query


def acquire(preparation_path, output, fetch=download, stage=None):
    raw = Path(preparation_path).read_bytes()
    if len(raw) > LIMIT:
        raise ValueError("Preparation exceeds 64 MiB")
    preparation = checked_json(raw)
    raster, query = plan(preparation)
    output = Path(output)
    output.mkdir(exist_ok=stage is not None)
    manifest = dict(schema_version=1, kind="openride.geographic-sources",
                    preparation_sha256=hashlib.sha256(raw).hexdigest(),
                    source_route_sha256=preparation["source_route_sha256"],
                    requested_bounds=preparation["acquisition_bounds"],
                    acquired_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                    complete=False, files={}, errors={},
                    attribution={"terrain": "USGS National Map 3D Elevation Program (3DEP)",
                                 "osm": "© OpenStreetMap contributors; ODbL 1.0; https://www.openstreetmap.org/copyright"},
                    vertical_reference="Unverified: inspect saved USGS catalog metadata before correcting roads",
                    requests={"terrain_service": USGS, "terrain_export": raster, "osm_endpoint": OVERPASS})
    marker=output/'sources.json'
    if marker.exists():
        previous=checked_json(marker.read_bytes())
        if previous['preparation_sha256']!=manifest['preparation_sha256']:raise ValueError('Source cache belongs to another preparation')
        for name,entry in previous['files'].items():
            if Path(name).name!=name or ':' in name or '\\' in name:raise ValueError('Unsafe cached asset')
            if hashlib.sha256((output/name).read_bytes()).hexdigest()!=entry['sha256']:raise ValueError('Cached source checksum mismatch: '+name)
        manifest=previous

    def save(name, content):
        if isinstance(content, str):
            content = content.encode("utf-8")
        with (output / name).open("wb") as file:
            file.write(content)
        manifest["files"][name] = dict(sha256=hashlib.sha256(content).hexdigest(), bytes=len(content))

    save("osm-query.overpass", query)
    try:
        if stage=='osm' or 'terrain.tif' in manifest['files']:raise StopIteration()
        service = fetch(USGS + "?f=json")
        checked_json(service)
        save("usgs-service.json", service)
        catalog = fetch(USGS + "/query?" + urllib.parse.urlencode(dict(
            f="json", geometry=raster["bbox"], geometryType="esriGeometryEnvelope", inSR="4326",
            spatialRel="esriSpatialRelIntersects", outFields="*", returnGeometry="false")))
        checked_json(catalog)
        save("usgs-catalog.json", catalog)
        response = fetch(USGS + "/exportImage?" + urllib.parse.urlencode(raster))
        image = checked_json(response)
        extent = image["extent"]
        sr = extent["spatialReference"]
        if sr.get("latestWkid", sr.get("wkid")) != 4326 or image.get("width") != 256 or image.get("height") != 256:
            raise ValueError("Unexpected raster CRS or dimensions")
        actual = [extent[k] for k in ("xmin", "ymin", "xmax", "ymax")]
        expected = [float(v) for v in raster["bbox"].split(",")]
        if any(not math.isfinite(a) or abs(a-b) > 1e-7 for a, b in zip(actual, expected)):
            raise ValueError("Returned raster extent differs from requested bounds")
        save("usgs-export.json", response)
        href = image["href"]
        parsed = urllib.parse.urlsplit(href)
        if parsed.scheme != "https" or parsed.hostname != "elevation.nationalmap.gov" or parsed.username or parsed.password:
            raise ValueError("Unexpected USGS image download URL")
        tiff = fetch(href)
        if tiff[:4] not in (b"II*\x00", b"MM\x00*", b"II+\x00", b"MM\x00+"):
            raise ValueError("Elevation response is not TIFF")
        save("terrain.tif", tiff)
        manifest['errors'].pop('terrain',None)
    except StopIteration:pass
    except Exception as error:
        manifest["errors"]["terrain"] = str(error)
    try:
        if stage=='terrain' or 'map.osm' in manifest['files']:raise StopIteration()
        osm = fetch(OVERPASS, {"data": query})
        if b"<!DOCTYPE" in osm.upper() or b"<!ENTITY" in osm.upper():
            raise ValueError("Unexpected XML declarations")
        root = ET.fromstring(osm)
        if root.tag != "osm" or root.find("remark") is not None or root.find("way") is None:
            raise ValueError("Overpass response is empty, incomplete, or contains an error remark")
        save("map.osm", osm)
        manifest['errors'].pop('osm',None)
    except StopIteration:pass
    except Exception as error:
        manifest["errors"]["osm"] = str(error)
    manifest["complete"] = not manifest["errors"] and 'terrain.tif' in manifest['files'] and 'map.osm' in manifest['files']
    with (output / "sources.json.tmp").open("w", encoding="utf-8") as file:
        json.dump(manifest, file, indent=2, ensure_ascii=False)
    (output/'sources.json.tmp').replace(marker)
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("preparation", type=Path)
    parser.add_argument("new_directory", type=Path)
    args = parser.parse_args()
    try:
        report = acquire(args.preparation, args.new_directory)
        print(json.dumps({"directory": str(args.new_directory), "complete": report["complete"], "errors": report["errors"]}, indent=2))
        return 0 if report["complete"] else 1
    except Exception as error:
        print(str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
