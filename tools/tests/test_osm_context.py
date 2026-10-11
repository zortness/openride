import importlib.util
from pathlib import Path
import unittest
import tempfile
import json
import hashlib

spec = importlib.util.spec_from_file_location("osm_context", Path(__file__).parents[1] / "osm-context.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class OsmContextTests(unittest.TestCase):
    def test_intersection_does_not_hide_fourth_or_later_road(self):
        nodes=''.join(f'<node id="{i*2}" lat="{i*0.00001}" lon="-0.001"/><node id="{i*2+1}" lat="{i*0.00001}" lon="0.001"/>' for i in range(5))
        ways=''.join(f'<way id="{i}"><nd ref="{i*2}"/><nd ref="{i*2+1}"/><tag k="highway" v="residential"/></way>' for i in range(5))
        prep=dict(origin_lat_lon_elevation=[0,0,0],points=[dict(latitude=0,longitude=0,source_sample=0)])
        result=module.analyze(prep,('<osm>'+nodes+ways+'</osm>').encode())
        self.assertEqual(len(result['matches'][0]['candidates']),5)
        self.assertEqual(result['candidate_version'],2)

    def test_matches_segment_interior_and_retains_named_alternatives(self):
        osm = b'''<osm version="0.6">
          <node id="1" lat="33.8" lon="-118.401"/><node id="2" lat="33.8" lon="-118.399"/>
          <node id="3" lat="33.8001" lon="-118.401"/><node id="4" lat="33.8001" lon="-118.399"/>
          <way id="10"><nd ref="1"/><nd ref="2"/><tag k="highway" v="residential"/><tag k="name" v="First road"/></way>
          <way id="20"><nd ref="3"/><nd ref="4"/><tag k="highway" v="cycleway"/><tag k="name" v="Second road"/></way></osm>'''
        prep = dict(origin_lat_lon_elevation=[33.8, -118.4, 0], points=[
            dict(latitude=33.80004, longitude=-118.4, source_sample=7, break_before=True)])
        result = module.analyze(prep, osm)
        sample = result["matches"][0]
        self.assertEqual(sample["candidates"][0]["way_id"], "10")
        self.assertEqual(sample["candidates"][1]["name"], "Second road")
        self.assertAlmostEqual(sample["candidates"][0]["distance_m"], 4.4478, places=2)
        self.assertAlmostEqual(sample["candidates"][0]["fraction"], 0.5, places=5)
        self.assertEqual(sample["status"], "ambiguous")
        self.assertEqual(prep["points"][0]["latitude"],33.80004)

    def test_missing_nodes_and_error_responses_fail_and_no_road_is_unmatched(self):
        prep = dict(origin_lat_lon_elevation=[33.8,-118.4,0],points=[dict(latitude=33.8,longitude=-118.4,source_sample=0)])
        for xml in (b'<osm><remark>timeout</remark></osm>', b'<!DOCTYPE osm><osm/>',
                    b'<osm><way id="5"><nd ref="missing"/><tag k="highway" v="residential"/></way></osm>'):
            with self.assertRaises(ValueError):
                module.analyze(prep,xml)
        result = module.analyze(prep,b'<osm/>')
        self.assertEqual(result["matches"][0]["status"],"unmatched")

    def test_draping_samples_ground_and_skips_structures_and_missing_coverage(self):
        prep = self.preparation()
        sample = module.terrain_sampler(prep)
        self.assertAlmostEqual(sample(0,0),10)
        self.assertIsNone(sample(10000,10000))
        roads = [dict(way_id="1",tags={},positions=[(-5,0),(5,0)]),
                 dict(way_id="2",tags={"bridge":"yes"},positions=[(-5,0),(5,0)])]
        lines = module.context_lines(prep,roads)
        self.assertEqual(lines["omitted_structure_ways"],["2"])
        self.assertEqual(len(lines["vertices"]),2)
        self.assertAlmostEqual(lines["vertices"][0][1],10.5)
        prep["terrain"]["mesh"]["indices"]=[]
        self.assertIsNone(module.terrain_sampler(prep)(0,0))

    @staticmethod
    def preparation():
        return dict(kind="openride.scenery-preparation",schema_version=1,trainer_ready=False,
            origin_lat_lon_elevation=[33.8,-118.4,0],points=[dict(latitude=33.8,longitude=-118.4,source_sample=0,elevation_m=5)],
            terrain=dict(width=2,height=2,extent=dict(xmin=-118.401,xmax=-118.399,ymin=33.799,ymax=33.801),
                mesh=dict(vertices=[[0,10,0]]*4,indices=[0,1,2,1,3,2])))

    def test_new_bundle_preserves_route_and_rejects_tampered_sources(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            sources = root/"sources"
            sources.mkdir()
            osm = b'<osm><node id="1" lat="33.8" lon="-118.4001"/><node id="2" lat="33.8" lon="-118.3999"/><way id="1"><nd ref="1"/><nd ref="2"/><tag k="highway" v="residential"/></way></osm>'
            manifest = dict(kind="openride.geographic-sources",schema_version=1,attribution={"osm":"Synthetic"},
                files={"map.osm":{"sha256":hashlib.sha256(osm).hexdigest()}})
            (sources/"map.osm").write_bytes(osm)
            (sources/"sources.json").write_text(json.dumps(manifest))
            prep = self.preparation()
            prep["terrain"]["source_snapshot"]=manifest
            prep["files"]=[]
            for name in ("road-preview.obj","bounds.geojson","osm-query.overpass"):
                (root/name).write_bytes(b"fixture")
                prep["files"].append(dict(path=name,sha256=hashlib.sha256(b"fixture").hexdigest()))
            path = root/"preparation.json"
            original = json.dumps(prep).encode()
            path.write_bytes(original)
            output = root/"result"
            report = module.build(path,sources,output)
            self.assertEqual(report["summary"]["matched"],1)
            self.assertEqual(path.read_bytes(),original)
            result = json.loads((output/"preparation.json").read_bytes())
            self.assertEqual(result["points"],prep["points"])
            self.assertFalse(result["trainer_ready"])
            with self.assertRaises(FileExistsError):
                module.build(path,sources,output)
            (sources/"map.osm").write_bytes(b"changed")
            with self.assertRaisesRegex(ValueError,"checksum"):
                module.build(path,sources,root/"bad")
            self.assertFalse((root/"bad").exists())


if __name__ == "__main__":
    unittest.main()
