import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec=importlib.util.spec_from_file_location("package",Path(__file__).parents[1]/"package-scenery.py")
module=importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class Packages(unittest.TestCase):
    def test_self_contained_deterministic_and_preserves_source(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            source=root/"source.json"
            p=dict(kind="openride.scenery-preparation",schema_version=1,trainer_ready=False,name="Test",source_route_sha256="route",
                points=[1,2],corrected_profile=dict(points=[1,2]),reviewed_profile=dict(points=[1,2]),
                files=[dict(path="authoring-only.obj")],acquisition_bounds={},origin_lat_lon_elevation=[0,0,0],
                terrain=dict(source_snapshot=dict(attribution=dict(terrain="Synthetic test data"))))
            raw=json.dumps(p).encode(); source.write_bytes(raw)
            first=module.build(source,root/"a","synthetic","0.1")
            second=module.build(source,root/"b","synthetic","0.1")
            self.assertEqual(first,second)
            self.assertEqual(first["profile"],"reviewed_profile")
            self.assertFalse(first["trainer_ready"])
            self.assertEqual(first["dependencies"],[])
            for entry in first["files"]:
                payload=(root/"a"/entry["path"]).read_bytes()
                self.assertEqual(len(payload),entry["bytes"])
                self.assertEqual(hashlib.sha256(payload).hexdigest(),entry["sha256"])
            self.assertEqual(json.loads((root/"a/preparation.json").read_bytes())["files"],[])
            self.assertEqual(source.read_bytes(),raw)
            with self.assertRaises(FileExistsError):module.build(source,root/"a","synthetic","0.1")
            with self.assertRaises(ValueError):module.build(source,root/"bad","../escape","0.1")
            self.assertFalse((root/"bad").exists())


if __name__=="__main__":unittest.main()
