import importlib.util
import json
import math
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec=importlib.util.spec_from_file_location('assembly',Path(__file__).parents[1]/'assemble-corridor.py')
assembly=importlib.util.module_from_spec(spec);spec.loader.exec_module(assembly)


def source(length=24000):
    points=[dict(source_sample=i,distance_m=float(d),latitude=33+math.degrees(d/assembly.R),longitude=-118.,
        elevation_m=100.+d*.01,break_before=False) for i,d in enumerate(range(0,length+1,100))]
    return dict(points=points,review_revision='synthetic',issues=[],unresolved_breaks=0)


class AssemblyTests(unittest.TestCase):
    def test_reversal_caps_preserve_each_road_segment_and_height(self):
        points=[dict(position=p) for p in [[0.,0.,0.],[0.,1.,10.],[0.,2.,0.],[0.,3.,-10.]]]
        mesh=assembly.road_ribbon(points)
        self.assertEqual(len(mesh['indices']),18)
        self.assertEqual(sorted(set(v[1] for v in mesh['vertices'])),[0.,1.,2.,3.])
        self.assertTrue(all(math.isfinite(v) for p in mesh['vertices'] for v in p))

    def test_projection_roundtrip_and_canonical_long_route(self):
        frame,profile=assembly.route(source(160000),160000)
        for p in profile['points']:
            lat,lon=frame.geographic(p['position'][0],p['position'][2])
            self.assertAlmostEqual(lat,p['latitude'],places=9)
            self.assertAlmostEqual(lon,p['longitude'],places=9)
        self.assertEqual(profile['points'][-1]['distance_m'],160000)
        self.assertAlmostEqual(profile['points'][-1]['elevation_m'],1700)
        self.assertEqual(len(profile['points']),1601) # render tessellation is separate
        with self.assertRaisesRegex(ValueError,'200 km'):assembly.route(source(200100),200100)
        with self.assertRaisesRegex(ValueError,'hemisphere'):frame.project(-frame.origin[0],frame.origin[1]+180)

    def test_stale_inputs_never_publish_or_replace_a_package(self):
        class FakeTerrain:
            provenance=[]
            max_overlap_m=0.
            reconciled_samples=0
            max_reconciliation_shift_m=0.
            def __call__(self,x,z): return 0.
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);project=root/'job';project.mkdir()
            (project/'reviewed-profile.json').write_text(json.dumps(source(200)))
            builder=root/'builder.exe';builder.write_bytes(b'synthetic')
            job=dict(regions=[dict(id='region-0000',end_m=200)],length_m=200,
                     grade_findings=[],review_revision='test',profile_sha256='test')
            destination=root/'new.orpack'
            with patch.object(assembly.corridor,'load',side_effect=[job,ValueError('Saved review changed')]), patch.object(assembly,'Terrain',return_value=FakeTerrain()):
                with self.assertRaisesRegex(ValueError,'Saved review changed'):
                    assembly.assemble(project,destination,builder)
            self.assertFalse(destination.exists())
            self.assertTrue((root/'.openride-assembly').is_dir())
            destination.mkdir();marker=destination/'existing';marker.write_bytes(b'keep')
            with self.assertRaises(FileExistsError):assembly.assemble(project,destination,builder)
            self.assertEqual(marker.read_bytes(),b'keep')

    def test_overlap_feather_is_bounded_by_local_variation_and_rejects_flat_offsets(self):
        terrain=assembly.Terrain.__new__(assembly.Terrain)
        terrain.frame=assembly.Frame([0.,0.,0.]);terrain.max_overlap_m=0.
        terrain.reconciled_samples=0;terrain.max_reconciliation_shift_m=0.
        terrain.reconcile_overlaps=True
        terrain.index={(0,0):[0,1]}
        bounds=dict(south=-1,north=1,west=-1,east=1)
        terrain.tiles=[(None,[0.,0.,0.],bounds)]*2
        terrain.grid=lambda i:(2,2,-8.,-8.,16.,16.,[4.*i,8.+4.*i,4.*i,8.+4.*i])
        self.assertAlmostEqual(terrain(0.,0.),6.)
        self.assertEqual(terrain.reconciled_samples,1)
        terrain.reconcile_overlaps=False
        with self.assertRaisesRegex(ValueError,'disagreement'):terrain(0.,0.)
        terrain.reconcile_overlaps=True
        terrain.grid=lambda i:(2,2,-8.,-8.,16.,16.,[4.*i]*4)
        with self.assertRaisesRegex(ValueError,'disagreement'):terrain(0.,0.)
        terrain.grid=lambda i:(2,2,-8.,-8.,16.,16.,[20.*i,100.+20.*i,20.*i,100.+20.*i])
        with self.assertRaisesRegex(ValueError,'exceeds 10'):terrain(0.,0.)

    def test_prefix_end_preserves_canonical_station_and_height(self):
        frame,profile=assembly.route(source(),16005)
        self.assertEqual(profile['points'][-1]['distance_m'],16005)
        self.assertAlmostEqual(profile['points'][-1]['elevation_m'],260.05)
        self.assertEqual(profile['points'][-1]['source_sample'],160)

    def test_shared_terrain_edges_normals_and_unique_triangle_ownership(self):
        frame,profile=assembly.route(source(1200),1200)
        with tempfile.TemporaryDirectory() as tmp:
            path=Path(tmp)
            entries=assembly.write_geometry(profile,lambda x,z: 2+x*.002+z*.003,path)
            self.assertGreater(len(entries),3)
            shared={};duplicates=0;triangles=set();road_triangles=0
            for entry in entries:
                raw=(path/entry['path']).read_bytes()
                self.assertEqual(assembly.tiled.sha(raw),entry['sha256'])
                meshes=json.loads(raw)
                for kind in ('road','terrain'):
                    mesh=meshes[kind]
                    for p,n in zip(mesh['vertices'],mesh['normals']):
                        key=(kind,p[0],p[2]) if kind=='terrain' else (kind,tuple(p))
                        value=(p[1],n)
                        if key in shared:
                            self.assertEqual(shared[key],value)
                            duplicates+=1
                        shared[key]=value
                    for i in range(0,len(mesh['indices']),3):
                        points=[tuple(mesh['vertices'][k]) for k in mesh['indices'][i:i+3]]
                        key=(kind,tuple(sorted(points)))
                        self.assertNotIn(key,triangles);triangles.add(key)
                        if kind=='road':road_triangles+=1
            self.assertGreater(duplicates,20)
            self.assertEqual(road_triangles,2*(len(assembly.dense_road(profile['points']))-1))

    def test_missing_terrain_fails_without_publishing_meshes_as_package(self):
        frame,profile=assembly.route(source(200),200)
        def missing(x,z): raise ValueError('Missing coverage')
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaisesRegex(ValueError,'Missing coverage'):
                assembly.write_geometry(profile,missing,Path(tmp))
            self.assertFalse((Path(tmp)/'manifest.json').exists())


if __name__=='__main__':unittest.main()
