import importlib.util
from pathlib import Path
import unittest
import hashlib
import tempfile

spec=importlib.util.spec_from_file_location('coast',Path(__file__).parents[1]/'coastal_scenery.py')
coast=importlib.util.module_from_spec(spec);spec.loader.exec_module(coast)

class Coast(unittest.TestCase):
    def test_inland_sources_leave_terrain_unchanged_and_omit_backdrop(self):
        raw=b'<osm><node id="1" lat="36" lon="-115"/><way id="2"><nd ref="1"/><tag k="highway" v="footway"/></way></osm>'
        terrain=dict(vertices=[[0,700,0]],indices=[])
        with tempfile.TemporaryDirectory() as tmp:
            source=Path(tmp)/'map.osm';source.write_bytes(raw)
            p=dict(osm_context=dict(source_sha256=hashlib.sha256(raw).hexdigest()))
            result,backdrop=coast.prepare(p,terrain,source)
            self.assertIs(result,terrain)
            self.assertIsNone(backdrop)
            p['osm_context']['source_sha256']='wrong'
            with self.assertRaisesRegex(ValueError,'match'):coast.prepare(p,terrain,source)
            for raw in [b'<osm><remark>incomplete</remark></osm>',b'<html/>',b'<osm><way id="1"><tag k="natural" v="coastline"/></way></osm>']:
                source.write_bytes(raw);p['osm_context']['source_sha256']=hashlib.sha256(raw).hexdigest()
                with self.assertRaises(ValueError):coast.prepare(p,terrain,source)

    def test_directed_coast_keeps_water_west_and_land_east(self):
        bounds=[0,0,100,100]
        chain=coast.clip_shoreline([[50,-10],[50,50],[50,110]],bounds)
        self.assertEqual(chain,[[50,0],[50,50],[50,100]])
        polygon=coast.water_polygon(chain,[-100,-100,200,200],bounds)
        self.assertTrue(coast.inside(25,50,polygon))
        self.assertFalse(coast.inside(75,50,polygon))
        self.assertTrue(coast.inside(-50,50,polygon))

    def test_reverse_direction_reverses_water_side(self):
        bounds=[0,0,100,100]
        chain=[[50,100],[50,50],[50,0]]
        polygon=coast.water_polygon(chain,[-100,-100,200,200],bounds)
        self.assertFalse(coast.inside(25,50,polygon))
        self.assertTrue(coast.inside(75,50,polygon))

    def test_missing_or_multiple_crossings_refused(self):
        for chain in [[[-10,-10],[-10,110]],[[25,-10],[25,110],[75,110],[75,-10]]]:
            with self.assertRaisesRegex(ValueError,'continuous coastline'):
                coast.clip_shoreline(chain,[0,0,100,100])

    def test_interior_endpoint_not_invented(self):
        with self.assertRaisesRegex(ValueError,'endpoint'):
            coast.water_polygon([[50,20],[50,80]],[-100,-100,200,200],[0,0,100,100])

if __name__=='__main__':unittest.main()
