"""Create a 7 km synthetic summit and overlapping mountain DEMs; no network or private data."""
import datetime
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import struct
import sys

spec=importlib.util.spec_from_file_location('tiled',Path(__file__).parents[1]/'tiled-scenery.py')
tiled=importlib.util.module_from_spec(spec);spec.loader.exec_module(tiled)
root=Path(sys.argv[1]);root.mkdir();review=root/'review';review.mkdir()
xml='<gpx><trk><name>SYNTHETIC 7 km summit</name><trkseg>'
for i in range(721):
    d=i*10;lat=33+math.degrees(d/6371000);h=1000+0.2*min(d,7000-d)
    timestamp=(datetime.datetime(2022,1,1,tzinfo=datetime.timezone.utc)+datetime.timedelta(seconds=i*5)).isoformat().replace('+00:00','Z')
    xml+=f'<trkpt lat="{lat:.12f}" lon="-118"><ele>{h}</ele><time>{timestamp}</time></trkpt>'
xml+='</trkseg></trk></gpx>';raw=xml.encode();digest=hashlib.sha256(raw).hexdigest()
(review/'source-0.gpx').write_bytes(raw)
(review/'review.json').write_text(json.dumps(dict(kind='openride.source-review',schema_version=1,sources=[dict(sha256=digest,name='synthetic.gpx',file='source-0.gpx')],geometry_source=digest,elevation_source=digest,decisions={})),encoding='utf-8')
plan=tiled.create_project(review,root/'project',sys.argv[2],0,7000)
origin=plan['preparation']['origin_lat_lon_elevation']
for tile in plan['tiles']:
    directory=root/'project'/tile['id'];sources=directory/'sources';sources.mkdir()
    preparation=(directory/'preparation.json').read_bytes();p=json.loads(preparation);b=tile['bounds'];w=h=64
    values=[]
    for row in range(h):
        for col in range(w):
            lat=b['north']-(row+0.5)*(b['north']-b['south'])/h;lon=b['west']+(col+0.5)*(b['east']-b['west'])/w
            x=6371000*math.radians(lon-origin[1])*math.cos(math.radians(origin[0]));z=-6371000*math.radians(lat-origin[0])
            values.append(origin[2]+0.2*min(-z,7000+z)+0.01*x)
    tags=[(256,4,w),(257,4,h),(258,3,32),(259,3,1),(262,3,1),(273,4,8+2+10*12+4),(277,3,1),(278,4,h),(279,4,w*h*4),(339,3,3)]
    tiff=b'II*\x00'+struct.pack('<IH',8,len(tags))+b''.join(struct.pack('<HHII',t,k,1,v) for t,k,v in tags)+struct.pack('<I',0)+struct.pack('<'+'f'*len(values),*values)
    export=tiled.encoded(dict(width=w,height=h,extent=dict(xmin=b['west'],ymin=b['south'],xmax=b['east'],ymax=b['north'],spatialReference=dict(wkid=4326))))
    files={'terrain.tif':tiff,'usgs-export.json':export}
    for name,data in files.items():(sources/name).write_bytes(data)
    manifest=dict(kind='openride.geographic-sources',schema_version=1,synthetic_fixture=True,preparation_sha256=tiled.sha(preparation),source_route_sha256=p['source_route_sha256'],
        attribution={'terrain':'SYNTHETIC mountain test terrain; not geographic source data'},requests={'terrain_export':dict(pixelType='F32',imageSR='4326',renderingRule='{"rasterFunction":"None"}')},
        files={name:dict(sha256=tiled.sha(data),bytes=len(data)) for name,data in files.items()})
    (sources/'sources.json').write_bytes(tiled.encoded(manifest))
print(root/'project')
