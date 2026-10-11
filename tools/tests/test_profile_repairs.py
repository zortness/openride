import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("repairs",Path(__file__).parents[1]/"repair-profile.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class Repairs(unittest.TestCase):
    def test_manual_removal_can_cross_split_way_ids_but_retains_geometry_limits(self):
        self.proposal['spans'][0]['method']='manual_remove'
        self.prep['corrections']['road_choices'][3]['way_id']='next_street_segment'
        self.assertEqual(len(module.repair(self.prep,self.proposal,'hash')['points']),4)
        self.prep['corrected_profile']['points'][2]['position'][2]=26
        with self.assertRaisesRegex(ValueError,'deviation'):module.repair(self.prep,self.proposal,'hash')

    def test_manual_deletion_uses_surviving_road_choices_and_trims_endpoints(self):
        p=dict(kind='openride.scenery-preparation',trainer_ready=False,corrected_profile=dict(points=[dict(source_sample=i,position=[i*10,0,12 if i==2 else 0],elevation_m=0,break_before=i==0) for i in range(6)]),corrections=dict(road_choices=[dict(source_sample=i,way_id='main') for i in [0,1,3,4,5]]))
        proposal=dict(kind='openride.profile-repairs',schema_version=1,preparation_sha256='hash',spans=[dict(method=m,start_source_sample=a,end_source_sample=b,reason='Manual edit') for m,a,b in [('trim_start',0,1),('manual_remove',1,3),('trim_end',4,5)]])
        result=module.repair(p,proposal,'hash')
        self.assertEqual([v['source_sample'] for v in result['points']],[1,3,4])
        self.assertTrue(result['points'][0]['break_before'])
        self.assertEqual(result['points'][0]['distance_m'],0)
        proposal['spans'][1]['method']='replace_with_chord'
        with self.assertRaises(ValueError):module.repair(p,proposal,'hash')

    def setUp(self):
        self.prep = dict(kind="openride.scenery-preparation",trainer_ready=False,files=[],
            corrected_profile=dict(points=[dict(source_sample=i,position=[x,y,0],elevation_m=y+16,
                break_before=i in (0,2)) for i,(x,y) in enumerate([(0,0),(10,1),(9,1.2),(20,2),(30,3)])]),
            corrections=dict(road_choices=[dict(source_sample=i,way_id="main") for i in range(5)]))
        self.proposal = dict(kind="openride.profile-repairs",schema_version=1,preparation_sha256="hash",
            spans=[dict(start_source_sample=1,end_source_sample=3,method="replace_with_chord",reason="Review stationary drift")])

    def test_explicit_repair_preserves_endpoints_originals_and_shared_grade(self):
        original = copy.deepcopy(self.prep)
        result = module.repair(self.prep,self.proposal,"hash")
        self.assertEqual(self.prep,original)
        self.assertEqual([p["source_sample"] for p in result["points"]],[0,1,3,4])
        self.assertEqual(result["points"][2]["grade_percent"],10)
        self.assertEqual(result["points"][2]["position"],[20,2,0])
        self.assertEqual(result["mesh"]["vertices"][6][1],2)
        self.assertEqual(result["repairs"][0]["replaced_breaks"],[2])
        self.assertEqual(result["findings"],[])

    def test_rejects_conflicts_large_deviations_and_different_roads(self):
        with self.assertRaisesRegex(ValueError,"Source conflict"):
            module.repair(self.prep,self.proposal,"changed")
        self.prep["corrected_profile"]["points"][2]["position"][2] = 11
        with self.assertRaisesRegex(ValueError,"deviation"):
            module.repair(self.prep,self.proposal,"hash")
        self.prep["corrected_profile"]["points"][2]["position"][2] = 0
        self.prep["corrections"]["road_choices"][2]["way_id"] = "side"
        with self.assertRaisesRegex(ValueError,"one OSM way"):
            module.repair(self.prep,self.proposal,"hash")

    def test_unrelated_breaks_remain_and_overlaps_fail(self):
        self.prep["corrected_profile"]["points"][4]["break_before"] = True
        result = module.repair(self.prep,self.proposal,"hash")
        self.assertEqual(result["findings"][0]["source_sample"],4)
        self.assertIsNone(result["points"][-1]["grade_percent"])
        self.proposal["spans"] *= 2
        with self.assertRaisesRegex(ValueError,"Overlapping"):
            module.repair(self.prep,self.proposal,"hash")

    def test_empty_undo_and_two_connectors_sharing_retained_endpoint(self):
        self.proposal['spans']=[]
        restored=module.repair(self.prep,self.proposal,'hash')
        self.assertEqual(len(restored['points']),5)
        self.assertEqual(restored['repairs'],[])
        for i,point in enumerate(self.prep['corrected_profile']['points']):
            point['position']=[i*10,0,0];point['elevation_m']=16;point['break_before']=i==0
        self.proposal['spans']=[dict(start_source_sample=a,end_source_sample=b,method='replace_with_chord',reason='Remove selected point') for a,b in [(0,2),(2,4)]]
        result=module.repair(self.prep,self.proposal,'hash')
        self.assertEqual([p['source_sample'] for p in result['points']],[0,2,4])
        self.assertEqual(result['findings'],[])

    def test_bundle_hash_and_exclusive_output(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root/"preparation.json"
            raw = json.dumps(self.prep).encode()
            source.write_bytes(raw)
            self.proposal["preparation_sha256"] = hashlib.sha256(raw).hexdigest()
            proposal = root/"proposal.json"
            proposal.write_text(json.dumps(self.proposal))
            module.build(source,proposal,root/"result")
            self.assertEqual(source.read_bytes(),raw)
            output = json.loads((root/"result/preparation.json").read_bytes())
            self.assertFalse(output["trainer_ready"])
            self.assertEqual(output["corrected_profile"],self.prep["corrected_profile"])
            with self.assertRaises(FileExistsError):
                module.build(source,proposal,root/"result")


if __name__ == "__main__":
    unittest.main()
