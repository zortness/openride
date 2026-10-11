import copy
import importlib.util
import unittest
import tempfile
import hashlib
import json
from pathlib import Path

spec = importlib.util.spec_from_file_location("corrections", Path(__file__).parents[1]/"correct-scenery.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class Corrections(unittest.TestCase):
    def setUp(self):
        self.prep = dict(kind="openride.scenery-preparation", schema_version=1, trainer_ready=False,
            origin_lat_lon_elevation=[34,-118,20],
            points=[dict(source_sample=i, position=[i*10, i*1.0, 0], elevation_m=20+i,
                         break_before=i==0, raw_grade_percent=10) for i in range(4)],
            osm_context=dict(matches=[dict(source_sample=1, candidates=[dict(way_id="1", node_ids=["2","3"], projected_xz=[12,0])])]))
        self.edits = dict(kind="openride.scenery-corrections", schema_version=1, preparation_sha256="source",
                          elevation_offset_m=16, road_choices=[])

    def test_offset_preserves_original_and_grade(self):
        original = copy.deepcopy(self.prep)
        result = module.apply(self.prep, self.edits, "source")
        self.assertEqual(self.prep, original)
        self.assertEqual(result["points"][1]["grade_percent"], 10)
        self.assertEqual(result["points"][1]["elevation_m"], 37)
        self.assertEqual(result["mesh"]["vertices"][0][1], 16)

    def test_explicit_snap_changes_shared_geometry_grade(self):
        self.edits["road_choices"] = [dict(source_sample=1, way_id="1", node_ids=["2","3"])]
        result = module.apply(self.prep, self.edits, "source")
        self.assertAlmostEqual(result["points"][1]["grade_percent"], 100/12)
        self.assertEqual(result["mesh"]["vertices"][2][0], 12)
        self.assertEqual(result["points"][1]["position"][0], 12)

    def test_breaks_missing_and_large_gaps_stay_disconnected(self):
        for change in (dict(break_before=True), dict(position=[1000,1,0]), dict(position=[10,None,0], elevation_m=None)):
            with self.subTest(change=change):
                prep = copy.deepcopy(self.prep)
                prep["points"][1].update(change)
                result = module.apply(prep, self.edits, "source")
                self.assertIsNone(result["points"][1]["grade_percent"])
                self.assertLess(len(result["mesh"]["indices"]), 18)

    def test_source_and_candidate_conflicts(self):
        with self.assertRaisesRegex(ValueError, "Source conflict"):
            module.apply(self.prep, self.edits, "changed")
        self.edits["road_choices"] = [dict(source_sample=1, way_id="unknown", node_ids=["2","3"])]
        with self.assertRaisesRegex(ValueError, "Candidate conflict"):
            module.apply(self.prep, self.edits, "source")
        self.edits["road_choices"] = []
        self.edits["elevation_offset_m"] = float("nan")
        with self.assertRaises(ValueError):
            module.apply(self.prep, self.edits, "source")

    def test_bundle_roundtrip_exclusive_output_and_conflict_before_write(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.prep["files"] = []
            source = root/"preparation.json"
            raw = json.dumps(self.prep).encode()
            source.write_bytes(raw)
            self.edits["preparation_sha256"] = hashlib.sha256(raw).hexdigest()
            edits = root/"edits.json"
            edits.write_text(json.dumps(self.edits))
            self.assertEqual(module.build(source, edits, root/"output"), 4)
            result = json.loads((root/"output/preparation.json").read_bytes())
            self.assertEqual(result["points"], self.prep["points"])
            self.assertFalse(result["trainer_ready"])
            self.assertEqual(source.read_bytes(), raw)
            with self.assertRaises(FileExistsError):
                module.build(source, edits, root/"output")
            source.write_bytes(raw+b" ")
            with self.assertRaisesRegex(ValueError, "Source conflict"):
                module.build(source, edits, root/"conflicted")
            self.assertFalse((root/"conflicted").exists())


if __name__ == "__main__":
    unittest.main()
