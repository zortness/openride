import copy
import hashlib
import importlib.util
import math
import json
import tempfile
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location("roads",Path(__file__).parents[1]/"create-route-profile.py")
module=importlib.util.module_from_spec(spec); spec.loader.exec_module(module)


def fixture(loop=False, reverse=False):
    refs=[0,1,2,3,0] if loop else [0,1,2]
    xml=('<osm><node id="0" lat="0" lon="0"/><node id="1" lat="0" lon="0.0001"/>'
         '<node id="2" lat="0.0001" lon="0.0001"/><node id="3" lat="0.0001" lon="0"/>'
         '<way id="road">'+''.join(f'<nd ref="{r}"/>' for r in refs)+'<tag k="highway" v="primary"/>'+('<tag k="oneway" v="-1"/>' if reverse else '')+'</way></osm>').encode()
    points=[]
    for i,(lat,lon) in enumerate([(0,0),(.0001,.0001)]):
        x,z=module.osm.local(lat,lon,[0,0,0])
        points.append(dict(source_sample=i,latitude=lat,longitude=lon,position=[x,i,z],elevation_m=i,break_before=i==0))
    p=dict(origin_lat_lon_elevation=[0,0,0],points=points,corrected_profile=dict(points=copy.deepcopy(points)))
    context=module.osm.analyze(p,xml);context['source_sha256']=hashlib.sha256(xml).hexdigest();p['osm_context']=context
    p['corrections']=dict(road_choices=[dict(source_sample=m['source_sample'],way_id=m['candidates'][0]['way_id'],node_ids=m['candidates'][0]['node_ids']) for m in context['matches']])
    return p,xml


class Roads(unittest.TestCase):
    def test_disconnected_runs_require_opt_in_and_bounded_connectors(self):
        p,_=fixture()
        xml=b'<osm><node id="a" lat="0" lon="0"/><node id="b" lat="0" lon="0.0001"/><node id="c" lat="0" lon="0.0003"/><node id="d" lat="0" lon="0.0004"/><way id="one"><nd ref="a"/><nd ref="b"/><tag k="highway" v="residential"/></way><way id="two"><nd ref="c"/><nd ref="d"/><tag k="highway" v="footway"/></way></osm>'
        points=[]
        for i,lon in enumerate([0,.0001,.0003,.0004]):
            x,z=module.osm.local(0,lon,[0,0,0])
            points.append(dict(source_sample=i,latitude=0,longitude=lon,position=[x,0,z],elevation_m=0,break_before=i==0))
        p['points']=points;p['corrected_profile']['points']=copy.deepcopy(points)
        p['osm_context']=module.osm.analyze(p,xml);p['osm_context']['source_sha256']=hashlib.sha256(xml).hexdigest()
        p['corrections']['road_choices']=[dict(source_sample=m['source_sample'],way_id=m['candidates'][0]['way_id'],node_ids=m['candidates'][0]['node_ids']) for m in p['osm_context']['matches']]
        original=copy.deepcopy(p)
        with self.assertRaisesRegex(ValueError,'points 2–3'):module.mixed_profile(p,xml)
        result=module.mixed_profile(p,xml,True)
        self.assertEqual(p,original)
        self.assertEqual([s['mode'] for s in result['sections']],['road','road'])
        self.assertEqual(len(result['road_gap_connectors']),1)
        self.assertAlmostEqual(result['road_gap_connectors'][0]['length_m'],22.238985,places=5)
        self.assertEqual(result['findings'],[])
        self.assertEqual(result['points'][0]['position'],points[0]['position'])
        self.assertEqual(result['points'][-1]['position'],points[-1]['position'])
        for a,b in zip(result['points'],result['points'][1:]):
            self.assertGreater(b['distance_m'],a['distance_m'])
            self.assertLessEqual(b['distance_m']-a['distance_m'],5.000001)
        p['corrected_profile']['points'][2]['break_before']=True
        with self.assertRaisesRegex(ValueError,'disconnected'):module.mixed_profile(p,xml,True)
        p['corrected_profile']['points'][2]['position'][0]+=150
        with self.assertRaisesRegex(ValueError,'100 m'):module.mixed_profile(p,xml,True)

    def test_explicit_restricted_footpath_is_valid_indoor_geometry_with_access_metadata(self):
        for tag,value in [('access','no'),('access','private'),('bicycle','no')]:
            p,xml=fixture()
            xml=xml.replace(b'v="primary"',b'v="footway"').replace(b'</way>',f'<tag k="{tag}" v="{value}"/></way>'.encode())
            p['osm_context']=module.osm.analyze(p,xml)
            p['osm_context']['source_sha256']=hashlib.sha256(xml).hexdigest()
            result=module.mixed_profile(p,xml)
            self.assertEqual(result['mode'],'road')
            self.assertEqual(result['findings'],[])
            self.assertEqual(result['access_notes'][0]['tags'],{tag:value})
            self.assertEqual(result['access_notes'][0]['source_samples'],[0,1])
            # A single snapped point in a GPS section retains the same metadata.
            p['corrections']['road_choices'].pop()
            self.assertEqual(module.mixed_profile(p,xml)['access_notes'][0]['source_samples'],[0])

    def test_default_points_are_valid_and_mixed_route_keeps_gps_endpoint(self):
        p,xml=fixture()
        gps=dict(source_sample=2,latitude=.0001,longitude=.0002,position=[22.238985,2,-11.119493],elevation_m=2,break_before=False)
        p['points'].append(gps);p['corrected_profile']['points'].append(copy.deepcopy(gps))
        original=copy.deepcopy(p)
        result=module.mixed_profile(p,xml)
        self.assertEqual(result['mode'],'mixed')
        self.assertEqual(result['findings'],[])
        self.assertEqual(result['points'][-1]['position'],gps['position'])
        self.assertTrue(any(abs(q['position'][0]-11.11949)<.001 and abs(q['position'][2])<.001 for q in result['points']))
        self.assertEqual(p,original)
        for a,b in zip(result['points'],result['points'][1:]):
            self.assertGreater(b['distance_m'],a['distance_m'])
            self.assertLessEqual(b['distance_m']-a['distance_m'],5.00001)
        p['corrected_profile']['points'][-1]['break_before']=True
        with self.assertRaisesRegex(ValueError,'disconnected'):module.mixed_profile(p,xml)

    def test_all_defaults_keep_original_geometry_and_all_selected_is_unchanged(self):
        p,xml=fixture()
        self.assertEqual(module.mixed_profile(p,xml),module.road_profile(p,xml))
        p['corrections']['road_choices']=[]
        result=module.mixed_profile(p,xml)
        self.assertEqual(result['mode'],'gps')
        self.assertEqual(result['points'][0]['position'],p['points'][0]['position'])
        self.assertEqual(result['points'][-1]['position'],p['points'][-1]['position'])
        with self.assertRaisesRegex(ValueError,'checksum'):module.mixed_profile(p,xml+b' ')

    def test_retains_corner_resamples_and_interpolates_heights(self):
        p,xml=fixture(); original=copy.deepcopy(p)
        result=module.road_profile(p,xml)
        self.assertEqual(p,original)
        self.assertEqual(result['osm_vertices'],3)
        self.assertGreater(result['length_m'],math.hypot(*[p['points'][-1]['position'][i] for i in (0,2)]))
        self.assertTrue(any(abs(v['position'][0]-11.11949)<.001 and abs(v['position'][2])<.001 for v in result['points']))
        self.assertEqual(result['points'][0]['elevation_m'],0)
        self.assertEqual(result['points'][-1]['elevation_m'],1)
        for a,b in zip(result['points'],result['points'][1:]):
            self.assertLessEqual(b['distance_m']-a['distance_m'],5.000001)
            self.assertAlmostEqual(b['grade_percent'],100/result['length_m'])

    def test_rejects_alternate_paths_oneway_and_source_conflict(self):
        p,xml=fixture(loop=True)
        with self.assertRaisesRegex(ValueError,'Multiple connected'):module.road_profile(p,xml)
        p,xml=fixture(reverse=True)
        with self.assertRaisesRegex(ValueError,'one-way'):module.road_profile(p,xml)
        p,xml=fixture()
        with self.assertRaisesRegex(ValueError,'checksum'):module.road_profile(p,xml+b' ')
        p['corrections']['road_choices'].pop()
        with self.assertRaisesRegex(ValueError,'Missing road choice'):module.road_profile(p,xml)

    def test_uturn_is_not_silently_removed(self):
        p,xml=fixture()
        # Visit the far endpoint then return to the start before finishing.
        samples=[copy.deepcopy(p['points'][i]) for i in (0,1,0,1)]
        for i,s in enumerate(samples):s['source_sample']=i
        p['points']=samples;p['corrected_profile']['points']=copy.deepcopy(samples)
        context=module.osm.analyze(p,xml);context['source_sha256']=hashlib.sha256(xml).hexdigest();p['osm_context']=context
        p['corrections']['road_choices']=[dict(source_sample=m['source_sample'],way_id='road',node_ids=m['candidates'][0]['node_ids']) for m in context['matches']]
        with self.assertRaisesRegex(ValueError,'backtracking'):module.road_profile(p,xml)

    def test_gps_mode_and_explicit_fallback_preserve_profile(self):
        p,xml=fixture()
        p.update(kind='openride.scenery-preparation',schema_version=1,trainer_ready=False)
        p['corrected_profile']['findings']=[]
        p['corrections']['road_choices']=[]
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory); source=root/'source.json'; source.write_text(json.dumps(p)); (root/'map.osm').write_bytes(xml)
            result=module.build(source,None,root/'gps','gps')
            self.assertEqual(result['points'],p['corrected_profile']['points'])
            default=module.build(source,root/'map.osm',root/'defaults','road')
            self.assertEqual(default['mode'],'gps')
            self.assertEqual(default['fallback_reasons'],[])
            (root/'map.osm').write_bytes(xml+b' ')
            with self.assertRaises(ValueError):module.build(source,root/'map.osm',root/'blocked','road')
            self.assertFalse((root/'blocked').exists())
            result=module.build(source,root/'map.osm',root/'fallback','road',True)
            self.assertEqual(result['mode'],'gps')
            self.assertTrue(result['fallback_reasons'])

    def test_disconnected_selected_ways_fail(self):
        p,xml=fixture()
        xml=xml.replace(b'</osm>',b'<node id="4" lat="0.00011" lon="0.0001"/><node id="5" lat="0.0002" lon="0.0001"/><way id="other"><nd ref="4"/><nd ref="5"/><tag k="highway" v="primary"/></way></osm>')
        p['osm_context']=module.osm.analyze(p,xml)
        p['osm_context']['source_sha256']=hashlib.sha256(xml).hexdigest()
        other=next(c for c in p['osm_context']['matches'][1]['candidates'] if c['way_id']=='other')
        p['corrections']['road_choices'][1]=dict(source_sample=1,way_id='other',node_ids=other['node_ids'])
        with self.assertRaisesRegex(ValueError,'connected route'):module.road_profile(p,xml)


if __name__=='__main__':unittest.main()
