import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import urllib.parse

spec = importlib.util.spec_from_file_location("acquire", Path(__file__).parents[1] / "acquire-scenery.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class AcquisitionTests(unittest.TestCase):
    def test_separate_stages_resume_without_refetching_terrain(self):
        with tempfile.TemporaryDirectory() as tmp:
            source=Path(tmp)/'preparation.json'
            source.write_text(json.dumps(dict(kind='openride.scenery-preparation',schema_version=1,source_route_sha256='test',acquisition_bounds=dict(south=33.77,west=-118.42,north=33.79,east=-118.40))))
            calls=[]
            def terrain(url,fields=None):
                calls.append(url)
                self.assertIsNone(fields)
                if '/exportImage?' in url:return json.dumps(dict(href='https://elevation.nationalmap.gov/test.tif',width=256,height=256,extent=dict(xmin=-118.42,ymin=33.77,xmax=-118.40,ymax=33.79,spatialReference=dict(wkid=4326)))).encode()
                return b'II*\x00data' if url.endswith('.tif') else b'{}'
            output=Path(tmp)/'sources'
            first=module.acquire(source,output,terrain,stage='terrain')
            self.assertFalse(first['complete']);self.assertEqual(len(calls),4)
            def osm(url,fields=None):
                self.assertEqual(url,module.OVERPASS);self.assertIsNotNone(fields)
                return b'<osm><way id="1"/></osm>'
            failed=module.acquire(source,output,lambda *args:b'{"error":"offline"}',stage='osm')
            self.assertIn('osm',failed['errors'])
            resumed=module.acquire(source,output,osm,stage='osm')
            self.assertTrue(resumed['complete']);self.assertEqual(len(calls),4)
            self.assertEqual(first['files']['terrain.tif'],resumed['files']['terrain.tif'])

    def test_bounded_requests_and_complete_snapshot(self):
        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / "preparation.json"
            preparation = dict(kind="openride.scenery-preparation", schema_version=1, source_route_sha256="test",
                               acquisition_bounds=dict(south=33.77, west=-118.42, north=33.79, east=-118.40))
            source.write_text(json.dumps(preparation))

            def fetch(url, fields=None):
                if fields:
                    self.assertIn('nwr["building"]', fields["data"])
                    return b'<osm version="0.6"><way id="1"/></osm>'
                if "/exportImage?" in url:
                    query = urllib.parse.parse_qs(urllib.parse.urlsplit(url).query)
                    self.assertEqual(query["renderingRule"], ['{"rasterFunction":"None"}'])
                    self.assertEqual(query["pixelType"], ["F32"])
                    return json.dumps(dict(href="https://elevation.nationalmap.gov/test.tif", width=256, height=256,
                                           extent=dict(xmin=-118.42, ymin=33.77, xmax=-118.40, ymax=33.79, spatialReference=dict(wkid=4326)))).encode()
                if url.endswith(".tif"):
                    return b"II*\x00fake TIFF; decoder validation is a separate step"
                return b"{}"

            output = Path(tmp) / "sources"
            report = module.acquire(source, output, fetch)
            self.assertTrue(report["complete"])
            self.assertIn("map.osm", report["files"])
            self.assertIn("terrain.tif", report["files"])
            self.assertEqual(json.loads(source.read_text()), preparation)
            with self.assertRaises(FileExistsError):
                module.acquire(source, output, fetch)

    def test_service_failures_are_saved_without_claiming_completion(self):
        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / "preparation.json"
            source.write_text(json.dumps(dict(kind="openride.scenery-preparation", schema_version=1,
                source_route_sha256="test", acquisition_bounds=dict(south=33.77, west=-118.42, north=33.79, east=-118.40))))
            report = module.acquire(source, Path(tmp) / "sources", lambda *args: b'{"error":{"message":"unavailable"}}')
            self.assertFalse(report["complete"])
            self.assertEqual(set(report["errors"]), {"terrain", "osm"})
            self.assertNotIn("terrain.tif", report["files"])

    def test_rejects_global_bounds_before_network_or_output(self):
        with self.assertRaises(ValueError):
            module.plan(dict(kind="openride.scenery-preparation", schema_version=1,
                acquisition_bounds=dict(south=-79, west=-170, north=79, east=170)))


if __name__ == "__main__":
    unittest.main()
