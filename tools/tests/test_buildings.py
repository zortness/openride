import importlib.util
import math
from pathlib import Path
import unittest
spec=importlib.util.spec_from_file_location('buildings',Path(__file__).parents[1]/'building_scenery.py')
b=importlib.util.module_from_spec(spec);spec.loader.exec_module(b)

class Buildings(unittest.TestCase):
    def test_height_provenance_and_rejection(self):
        self.assertEqual(b.height_m({'height':'7.5 m','building:levels':'4'}),(7.5,'height'))
        self.assertEqual(b.height_m({'building:levels':'2'}),(6.,'levels-times-3m'))
        self.assertEqual(b.height_m({}),(6.,'assumed-6m'))
        for height in ['nan','inf','100','-1','20 ft']:
            with self.assertRaises(ValueError):b.height_m({'height':height})

    def test_concave_roof_area_both_windings(self):
        poly=[[0,0],[10,0],[10,4],[4,4],[4,10],[0,10]]
        for p in [poly,list(reversed(poly))]:
            triangles=b.roof_triangles(p)
            area=sum(abs((p[j][0]-p[i][0])*(p[k][1]-p[i][1])-(p[j][1]-p[i][1])*(p[k][0]-p[i][0]))/2 for i,j,k in triangles)
            self.assertAlmostEqual(area,64)
            self.assertEqual(len(triangles),4)
        with self.assertRaises(ValueError):b.roof_triangles([[0,0],[10,10],[0,10],[10,0]])

    def test_ground_and_road_clearance(self):
        sample=b.ground_sampler({'vertices':[[0,0,10],[10,10,10],[0,20,0],[10,30,0]]},2,2)
        self.assertEqual(sample(5,5),15)
        self.assertIsNone(sample(-1,5))
        poly=[[0,0],[20,0],[20,20],[0,20]]
        self.assertTrue(b.near_road(poly,[[10,0,10]]))
        self.assertTrue(b.near_road(poly,[[24,0,10]]))
        self.assertFalse(b.near_road(poly,[[30,0,10]]))

    def test_subdivision_retains_area_and_bounds(self):
        mesh={'vertices':[],'indices':[]};b.add_triangle(mesh,[0,0,0],[100,0,0],[0,0,100])
        area=0
        for i in range(0,len(mesh['indices']),3):
            a,c,d=[mesh['vertices'][j] for j in mesh['indices'][i:i+3]]
            self.assertLessEqual(max(math.dist(a,c),math.dist(c,d),math.dist(d,a)),32)
            area+=abs((c[0]-a[0])*(d[2]-a[2])-(c[2]-a[2])*(d[0]-a[0]))/2
        self.assertAlmostEqual(area,5000)

if __name__=='__main__':unittest.main()
