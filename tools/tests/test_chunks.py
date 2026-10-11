import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import math

spec=importlib.util.spec_from_file_location('chunks',Path(__file__).parents[1]/'chunk-scenery.py')
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)


class Chunks(unittest.TestCase):
    def test_coarse_terrain_subdivision_preserves_surface_and_colours(self):
        mesh=dict(vertices=[[0,0,0],[40,4,0],[0,0,20]],indices=[0,1,2],colors=[[0,0,0],[1,0,0],[0,0,0]])
        original=json.dumps(mesh);result=module.bounded_terrain(mesh)
        self.assertEqual(json.dumps(mesh),original)
        self.assertGreater(len(result['indices']),3)
        area=0
        for i in range(0,len(result['indices']),3):
            a,b,c=[result['vertices'][j] for j in result['indices'][i:i+3]]
            self.assertLessEqual(max(math.dist(a,b),math.dist(b,c),math.dist(c,a)),32)
            area+=abs((b[0]-a[0])*(c[2]-a[2])-(c[0]-a[0])*(b[2]-a[2]))/2
        self.assertAlmostEqual(area,400)
        for v,color in zip(result['vertices'],result['colors']):
            self.assertAlmostEqual(v[1],v[0]/10)
            self.assertAlmostEqual(color[0],v[0]/40)

    def test_riding_road_shares_corner_vertices_at_profile_height(self):
        points=[dict(position=p) for p in [[0,0,0],[10,1,0],[10,2,10]]]
        mesh=module.riding_road(points)
        self.assertEqual(len(mesh['vertices']),6)
        self.assertEqual(len(mesh['indices']),12)
        self.assertEqual(set(mesh['indices'][:6]) & set(mesh['indices'][6:]),{2,3})
        for i,p in enumerate(points):
            for axis in range(3):
                self.assertAlmostEqual((mesh['vertices'][2*i][axis]+mesh['vertices'][2*i+1][axis])/2,p['position'][axis])

    def test_riding_corridor_preserves_profile_and_outer_terrain(self):
        points=[dict(position=[0,10,0]),dict(position=[100,12,0])]
        mesh=dict(vertices=[[50,40,0],[50,40,18],[50,40,24],[50,40,30],[50,40,80]],indices=[])
        original=json.dumps((mesh,points))
        adjusted,count=module.conform_terrain(mesh,points)
        self.assertEqual(count,3)
        self.assertAlmostEqual(adjusted['vertices'][0][1],10.75)
        self.assertAlmostEqual(adjusted['vertices'][1][1],10.75)
        self.assertAlmostEqual(adjusted['vertices'][2][1],25.375)
        self.assertEqual(adjusted['vertices'][3:],mesh['vertices'][3:])
        self.assertEqual(json.dumps((mesh,points)),original)

    def test_conserves_geometry_at_boundaries_and_hashes_every_payload(self):
        mesh=dict(vertices=[[250,0,0],[260,0,0],[250,0,10],[260,0,10]],indices=[0,1,2,1,3,2])
        p=dict(kind='openride.scenery-preparation',trainer_ready=False,name='Synthetic',origin_lat_lon_elevation=[0,0,0],
            route_profile=dict(mode='road',findings=[],points=[],mesh=mesh),terrain=dict(mesh=mesh,source_snapshot=dict(attribution={'test':'Synthetic'})))
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);source=root/'source.json';raw=json.dumps(p).encode();source.write_bytes(raw)
            report=module.build(source,root/'out')
            self.assertEqual(report['chunks'],2)
            manifest=json.loads((root/'out/manifest.json').read_bytes())
            counts={'terrain':0,'road':0}
            for entry in manifest['chunks']:
                content=(root/'out'/entry['path']).read_bytes()
                self.assertEqual(hashlib.sha256(content).hexdigest(),entry['sha256'])
                self.assertEqual(len(content),entry['bytes'])
                chunk=json.loads(content)
                for kind in counts:counts[kind]+=len(chunk[kind]['indices'])//3
            self.assertEqual(counts,{'terrain':2,'road':2})
            inland=b'<osm/>';osm=root/'map.osm';osm.write_bytes(inland)
            p['osm_context']=dict(source_sha256=hashlib.sha256(inland).hexdigest())
            inland_source=root/'inland.json';inland_source.write_text(json.dumps(p))
            module.build(inland_source,root/'inland',coastline_osm=osm)
            self.assertNotIn('coast',json.loads((root/'inland/manifest.json').read_bytes()))
            self.assertFalse((root/'inland/coast.json').exists())
            self.assertEqual(source.read_bytes(),raw)
            with self.assertRaises(FileExistsError):module.build(source,root/'out')
            p['terrain']['mesh']['vertices'][1][0]=1000;source.write_text(json.dumps(p))
            with self.assertRaisesRegex(ValueError,'overhang'):module.build(source,root/'invalid')
            self.assertFalse((root/'invalid').exists())


if __name__=='__main__':unittest.main()
