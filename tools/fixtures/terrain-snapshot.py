"""Generate visibly labelled synthetic sources for offline terrain pipeline verification."""
import hashlib
import json
import math
from pathlib import Path
import struct
import sys

preparation = Path(sys.argv[1])
destination = Path(sys.argv[2])
destination.mkdir()
raw = preparation.read_bytes()
data = json.loads(raw)
b = data["acquisition_bounds"]
width = height = 64
values = [80 + 30 * math.sin(x / 14) * math.cos(y / 17) for y in range(height) for x in range(width)]
# Baseline little-endian TIFF, one uncompressed strip, IEEE single-band float32.
tags = [(256, 4, width), (257, 4, height), (258, 3, 32), (259, 3, 1),
        (262, 3, 1), (273, 4, 8 + 2 + 10 * 12 + 4), (277, 3, 1),
        (278, 4, height), (279, 4, width * height * 4), (339, 3, 3)]
tiff = b"II*\x00" + struct.pack("<I", 8) + struct.pack("<H", len(tags))
for tag, kind, value in tags:
    tiff += struct.pack("<HHII", tag, kind, 1, value)
tiff += struct.pack("<I", 0) + struct.pack("<" + "f" * len(values), *values)
export = json.dumps(dict(width=width, height=height, extent=dict(xmin=b["west"], ymin=b["south"],
    xmax=b["east"], ymax=b["north"], spatialReference=dict(wkid=4326)))).encode()
files = {"terrain.tif": tiff, "usgs-export.json": export}
for name, content in files.items():
    (destination / name).write_bytes(content)
manifest = dict(schema_version=1, kind="openride.geographic-sources", synthetic_fixture=True,
    preparation_sha256=hashlib.sha256(raw).hexdigest(), source_route_sha256=data["source_route_sha256"],
    attribution={"terrain": "SYNTHETIC TEST TERRAIN — not USGS or real geography"},
    requests={"terrain_export": {"pixelType": "F32", "imageSR": "4326", "renderingRule": '{"rasterFunction":"None"}'}},
    files={name: {"sha256": hashlib.sha256(content).hexdigest()} for name, content in files.items()})
(destination / "sources.json").write_text(json.dumps(manifest), encoding="utf-8")
print(destination)
