import copy
import importlib.util
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location("jitter",Path(__file__).parents[1]/"detect-jitter.py")
module=importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class Jitter(unittest.TestCase):
    def setUp(self):
        xs=[-20,0,2,0,2,0,2,0,2,20,40]
        points=[dict(source_sample=i,latitude=34+i*1e-5,longitude=-118,position=[x,0,0],elevation_m=16,break_before=i==0) for i,x in enumerate(xs)]
        self.prep=dict(kind="openride.scenery-preparation",trainer_ready=False,source_activity_sha256="activity",points=points,
            corrected_profile=dict(points=copy.deepcopy(points)),corrections=dict(road_choices=[dict(source_sample=i,way_id="road") for i in range(len(xs))]))
        self.evidence=dict(kind="openride.activity-evidence",schema_version=1,source_activity_sha256="activity",samples=[
            dict(source_sample=i,latitude=p["latitude"],longitude=p["longitude"],time_ms=i*10000,speed_mps=None) for i,p in enumerate(points)])

    def test_detects_stop_and_preserves_all_inputs(self):
        original=copy.deepcopy(self.prep)
        report=module.detect(self.prep,self.evidence,"prep")
        self.assertEqual(len(report["candidates"]),1)
        candidate=report["candidates"][0]
        self.assertTrue(candidate["requires_review"])
        self.assertIsNone(candidate["recorded_speed_median_mps"])
        self.assertEqual(self.prep,original)
        self.assertEqual(len(report["evidence"]),len(original["points"]))

    def test_motion_conflict_is_rejected_and_gaps_are_explicit(self):
        for s in self.evidence["samples"]: s["speed_mps"]=3
        report=module.detect(self.prep,self.evidence,"prep")
        self.assertFalse(report["candidates"])
        self.assertEqual(report["rejected"][0]["confidence"],"ambiguous_motion")
        for i,s in enumerate(self.evidence["samples"]):
            s["speed_mps"]=None
            if i>=5:s["time_ms"]+=120000
        report=module.detect(self.prep,self.evidence,"prep")
        self.assertEqual(report["candidates"][0]["recording_gaps_over_60s"],1)

    def test_progress_and_fast_loops_are_not_stops(self):
        for i,p in enumerate(self.prep["points"]): p["position"][0]=i*3
        self.assertFalse(module.detect(self.prep,self.evidence,"prep")["candidates"])
        self.setUp()
        for i,s in enumerate(self.evidence["samples"]): s["time_ms"]=i*1000
        self.assertFalse(module.detect(self.prep,self.evidence,"prep")["candidates"])

    def test_missing_time_and_source_mismatch(self):
        for s in self.evidence["samples"]:s["time_ms"]=None
        self.assertFalse(module.detect(self.prep,self.evidence,"prep")["candidates"])
        self.evidence["source_activity_sha256"]="other"
        with self.assertRaisesRegex(ValueError,"conflict"):
            module.detect(self.prep,self.evidence,"prep")


if __name__=="__main__":unittest.main()
