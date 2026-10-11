import copy
import hashlib
import json
import math
from pathlib import Path
import sys
import tempfile
import unittest
sys.path.insert(0,str(Path(__file__).parents[1]))
import road_scenery as roads

class RoadScenery(unittest.TestCase):
    def test_widths_and_directional_lane_selection(self):
        self.assertEqual(roads.dimensions({'width':'8 m','lanes':'2'}),(8.,2,'width'))
        self.assertEqual(roads.dimensions({'lanes':'3'})[0],9.600000000000001)
        self.assertEqual(roads.lane_offset({},True),0)
        self.assertEqual(roads.lane_offset({'lanes':'3'},True),0)
        self.assertGreater(roads.lane_offset({'lanes':'2'},True),0)
        self.assertLess(roads.lane_offset({'lanes':'2'},True,'left'),0)
        self.assertEqual(roads.lane_offset({'lanes':'2','oneway':'yes'},False),0)
        self.assertGreater(roads.lane_offset({'lanes':'2','oneway':'-1'},False),0)
        self.assertLess(roads.lane_offset({'lanes':'2','oneway':'yes','turn:lanes':'through|right'},True),0)
        for bad in ['NaN','inf','0','50','12 feet']:
            self.assertEqual(roads.dimensions({'width':bad})[2],'estimated-from-class')

    def test_import_preserves_route_and_deterministic_clearance(self):
        raw=b'''<osm>
        <node id="1" lat="0" lon="0"/><node id="2" lat="0" lon="0.0005"/>
        <node id="3" lat="-0.0008" lon="-0.0008"/><node id="4" lat="-0.0008" lon="0.0008"/>
        <node id="5" lat="0.0008" lon="0.0008"/><node id="6" lat="0.0008" lon="-0.0008"/>
        <way id="10"><nd ref="1"/><nd ref="2"/><tag k="highway" v="residential"/><tag k="lanes" v="2"/></way>
        <way id="20"><nd ref="3"/><nd ref="4"/><nd ref="5"/><nd ref="6"/><nd ref="3"/><tag k="natural" v="scrub"/></way>
        </osm>'''
        positions=[[i*5.,0.,0.] for i in range(11)]
        p=dict(origin_lat_lon_elevation=[0,0,100],osm_context={'source_sha256':hashlib.sha256(raw).hexdigest()},
               route_profile={'points':[dict(position=q,distance_m=i*5.) for i,q in enumerate(positions)]},terrain={'width':2,'height':2})
        terrain=dict(vertices=[[-100,0,-100],[100,0,-100],[-100,0,100],[100,0,100]],indices=[0,1,2,1,3,2])
        original=copy.deepcopy(p)
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'map.osm';path.write_bytes(raw)
            a=roads.prepare(p,terrain,path);b=roads.prepare(p,terrain,path)
            self.assertEqual(a,b);self.assertEqual(p,original)
            _,meshes,plants,metadata,report=a
            self.assertEqual(report['matched_route_points'],11)
            self.assertGreater(len(meshes['center_markings']['indices']),0)
            self.assertGreater(len(plants),0)
            for plant in plants:self.assertGreaterEqual(roads.nearest(plant[0],plant[2],[0,0],[50,0])[0],10)
            self.assertTrue(all(m['way_id']=='10' and 0<m['lane_offset_m']<m['width_m']/2 for m in metadata))
            path.write_bytes(raw+b' ')
            with self.assertRaisesRegex(ValueError,'identity'):roads.prepare(p,terrain,path)

    def test_triangle_splits_obey_streaming_bound(self):
        mesh={'vertices':[],'indices':[]}
        roads.ribbon(mesh,[0,3,0],[100,4,0],5)
        self.assertGreater(len(mesh['indices']),6)
        for i in range(0,len(mesh['indices']),3):
            tri=[mesh['vertices'][j] for j in mesh['indices'][i:i+3]]
            self.assertLessEqual(max(math.dist(a,b) for a in tri for b in tri),32)

if __name__=='__main__':unittest.main()
