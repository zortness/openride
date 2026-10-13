import importlib.util
import math
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('tiled',Path(__file__).parents[1]/'tiled-scenery.py')
tiled=importlib.util.module_from_spec(spec);spec.loader.exec_module(tiled)

def fixture():
    points=[]
    for i in range(701):
        distance=i*10
        points.append(dict(source_sample=i,distance_m=distance,latitude=33+math.degrees(distance/6371000),longitude=-118,
                           elevation_m=1000+0.2*min(distance,7000-distance),break_before=False))
    return dict(kind='openride.reviewed-profile',schema_version=1,trainer_ready=False,review_revision='fixture',name='Synthetic summit',geometry_source='synthetic',points=points)

class TiledScenery(unittest.TestCase):
    def test_long_section_keeps_one_profile_and_plans_overlapping_source_tiles(self):
        source=fixture()
        plan=tiled.plan(source,0,7000)
        self.assertGreater(len(plan['tiles']),2)
        points=plan['preparation']['route_profile']['points']
        self.assertAlmostEqual(points[-1]['distance_m'],7000)
        self.assertAlmostEqual(max(p['elevation_m'] for p in points),1700)
        self.assertAlmostEqual(points[1]['grade_percent'],20)
        self.assertAlmostEqual(points[-1]['grade_percent'],-20)
        self.assertTrue(plan['steep_segments'])
        self.assertEqual(source,fixture())

    def test_shared_lattice_has_identical_seam_heights_and_preserves_summit_profile(self):
        plan=tiled.plan(fixture(),0,7000)
        grids={}
        for tile in plan['tiles']:
            x0=tile['x']*1000-40;z0=tile['z']*1000-40
            vertices=[]
            for row in range(5):
                for col in range(5):
                    x=x0+col*270;z=z0+row*270
                    vertices.append([x,0.03*x-0.02*z,z])
            grids[tile['id']]=dict(width=5,height=5,nodata_pixels=0,mesh=dict(vertices=vertices))
        before=tiled.encoded(plan['preparation']['route_profile'])
        mesh,report=tiled.mosaic(plan,grids)
        self.assertGreater(report['shared_boundary_vertices'],0)
        self.assertLess(report['max_overlap_difference_m'],1e-9)
        self.assertEqual(len(mesh['vertices']),len(set((v[0],v[2]) for v in mesh['vertices'])))
        for x,y,z in mesh['vertices']:self.assertAlmostEqual(y,0.03*x-0.02*z)
        self.assertEqual(before,tiled.encoded(plan['preparation']['route_profile']))

    def test_breaks_invalid_distance_and_projection_are_not_bypassed_by_long_section(self):
        source=fixture();source['points'][300]['break_before']=True
        with self.assertRaisesRegex(ValueError,'unresolved breaks'):tiled.plan(source,0,7000)
        with self.assertRaises(ValueError):tiled.plan(fixture(),float('nan'),7000)
        source=fixture();source['points'][300]['distance_m']=-1
        with self.assertRaisesRegex(ValueError,'Unordered'):tiled.plan(source,0,7000)

    def test_missing_dem_and_disagreeing_overlaps_fail_instead_of_hiding_seams(self):
        plan=tiled.plan(fixture(),0,7000)
        with self.assertRaisesRegex(ValueError,'Missing terrain'):tiled.mosaic(plan,{})
        grids={t['id']:dict(width=2,height=2,nodata_pixels=1,mesh=dict(vertices=[])) for t in plan['tiles']}
        with self.assertRaisesRegex(ValueError,'missing elevation'):tiled.mosaic(plan,grids)
        for i,t in enumerate(plan['tiles']):
            x,z=t['x']*1000-40,t['z']*1000-40
            grids[t['id']]=dict(width=2,height=2,nodata_pixels=0,mesh=dict(vertices=[[x+col*1080,i*10,z+row*1080] for row in range(2) for col in range(2)]))
        with self.assertRaisesRegex(ValueError,'overlap differs'):tiled.mosaic(plan,grids)

    def test_hairpin_and_stacked_revisits_keep_route_order_and_distinct_heights(self):
        source=fixture();source['points']=[];distance=0;previous=None
        for i,(x,z,h) in enumerate([(0,0,100),(0,2000,110),(20,2000,120),(20,0,130),(0,0,140),(0,2000,150)]):
            if previous is not None:distance+=math.hypot(x-previous[0],z-previous[1])
            source['points'].append(dict(source_sample=i,distance_m=distance,latitude=33-math.degrees(z/6371000),longitude=-118+math.degrees(x/6371000)/math.cos(math.radians(33)),elevation_m=h,break_before=False))
            previous=(x,z)
        result=tiled.plan(source,0,distance)
        points=result['preparation']['route_profile']['points']
        visits=[p for p in points if math.hypot(p['position'][0],p['position'][2])<1e-6]
        self.assertEqual([p['elevation_m'] for p in visits],[100,140])
        self.assertGreater(visits[1]['distance_m'],4000)
        self.assertEqual(len(result['tiles']),len(set(t['id'] for t in result['tiles'])))


if __name__=='__main__':unittest.main()
