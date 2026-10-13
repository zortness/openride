import importlib.util
import json
import math
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import urllib.parse

spec=importlib.util.spec_from_file_location('corridor',Path(__file__).parents[1]/'corridor-scenery.py')
corridor=importlib.util.module_from_spec(spec)
spec.loader.exec_module(corridor)


def fixture(length=17000):
    points=[dict(source_sample=i,distance_m=float(d),latitude=33+math.degrees(d/6371000),
                 longitude=-118.,elevation_m=100+0.01*d,break_before=False)
            for i,d in enumerate(range(0,length+1,100))]
    return dict(kind='openride.reviewed-profile',schema_version=1,trainer_ready=False,
                review_revision='fixture',geometry_source='synthetic',name='Synthetic corridor',
                points=points,issues=[],unresolved_breaks=0)


class Fetch:
    def __init__(self):self.calls=[]
    def __call__(self,url,fields=None):
        self.calls.append(url)
        if fields is not None:return b'<osm><way id="1"/></osm>'
        if '/exportImage?' in url:
            b=list(map(float,urllib.parse.parse_qs(urllib.parse.urlsplit(url).query)['bbox'][0].split(',')))
            return json.dumps(dict(href='https://elevation.nationalmap.gov/fixture.tif',width=256,height=256,
                extent=dict(xmin=b[0],ymin=b[1],xmax=b[2],ymax=b[3],spatialReference=dict(wkid=4326)))).encode()
        return b'II*\x00synthetic' if url.endswith('.tif') else b'{}'


def fake_package(project,destination):
    destination=Path(destination);destination.mkdir()
    plan=corridor.tiled.load_project(project)
    files={'route.json':corridor.tiled.encoded(dict(points=plan['preparation']['route_profile']['points'])),
           'terrain-provenance.json':corridor.tiled.encoded(dict(review_revision=plan['preparation']['review_revision']))}
    for name,raw in files.items():(destination/name).write_bytes(raw)
    entry=lambda name:dict(path=name,bytes=len(files[name]),sha256=corridor.tiled.sha(files[name]))
    (destination/'manifest.json').write_bytes(corridor.tiled.encoded(dict(kind='openride.stream-preview',schema_version=1,
        route=entry('route.json'),terrain_provenance=entry('terrain-provenance.json'),chunks=[])))


class CorridorTests(unittest.TestCase):
    def test_full_route_plan_keeps_continuous_canonical_boundaries_and_limits(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)/'job'
            job=corridor.create_from_snapshot(fixture(),root)
            self.assertEqual(len(job['regions']),3)
            self.assertEqual(job['regions'][-1]['end_m'],17000)
            self.assertEqual(corridor.load(root),job)
            previous=None
            for region in job['regions']:
                plan=corridor.tiled.load_project(root/region['id'])
                points=plan['preparation']['points']
                identity=lambda p:[p[k] for k in ('latitude','longitude','elevation_m','source_distance_m')]
                if previous is not None:self.assertEqual(identity(points[0]),previous)
                previous=identity(points[-1])
                self.assertLessEqual(len(plan['tiles']),40)
                self.assertLessEqual(len(points),10000)
            with self.assertRaises(FileExistsError):corridor.create_from_snapshot(fixture(),root)

    def test_grade_blockers_remain_explicit_and_valid_regions_survive(self):
        source=fixture()
        source['points'][90]['elevation_m']+=200
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)/'job'
            job=corridor.create_from_snapshot(source,root)
            self.assertIn('blocked',job['regions'][1])
            self.assertEqual(len([g for g in job['grade_findings'] if g['severity']=='blocked']),2)
            self.assertEqual(corridor.status(root)['regions'][1]['bake'],'blocked')
            source['issues']=['Missing tail']
            with self.assertRaisesRegex(ValueError,'unresolved findings'):list(corridor.regions(source,8000))

    def test_acquisition_resumes_without_refetch_and_osm_is_independent(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)/'job'
            corridor.create_from_snapshot(fixture(3000),root,region_m=1000)
            fetch=Fetch()
            self.assertTrue(corridor.run(root,'terrain',max_regions=1,fetch=fetch)['stopped'])
            first=len(fetch.calls)
            corridor.run(root,'terrain',fetch=fetch)
            total=len(fetch.calls)
            self.assertGreater(total,first)
            corridor.run(root,'terrain',fetch=fetch)
            self.assertEqual(len(fetch.calls),total)
            corridor.run(root,'osm',fetch=fetch)
            self.assertTrue(all(r['terrain']=='verified' and r['osm']=='verified' for r in corridor.status(root)['regions']))
            corridor.run(root,'terrain',fetch=fetch)
            self.assertEqual(len([u for u in fetch.calls if u.endswith('.tif')]),total//4)

    def test_failure_retry_preserves_completed_work_and_detects_corruption(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)/'job'
            corridor.create_from_snapshot(fixture(2000),root,region_m=1000)
            fetch=Fetch()
            corridor.run(root,'terrain',max_regions=1,fetch=fetch)
            receipt=(root/'region-0000'/'terrain.done.json').read_bytes()
            def fail(*args):raise OSError('offline')
            with self.assertRaisesRegex(ValueError,'offline'):corridor.run(root,'terrain',fetch=fail)
            self.assertFalse((root/'job.lock').exists())
            self.assertEqual((root/'region-0000'/'terrain.done.json').read_bytes(),receipt)
            corridor.run(root,'terrain',fetch=fetch)
            plan=corridor.tiled.load_project(root/'region-0000')
            bad=root/'region-0000'/plan['tiles'][0]['id']/'sources'/'terrain.tif'
            bad.write_bytes(b'corrupt')
            self.assertTrue(corridor.status(root)['regions'][0]['terrain'].startswith('invalid:'))
            with self.assertRaisesRegex(ValueError,'checksum'):corridor.run(root,'terrain',fetch=fetch)

    def test_clean_stop_and_cache_seeding(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)/'job'
            corridor.create_from_snapshot(fixture(1000),root)
            fetch=Fetch()
            def stop(url,fields=None):
                (root/'STOP').touch()
                return fetch(url,fields)
            self.assertTrue(corridor.run(root,'terrain',fetch=stop)['stopped'])
            (root/'STOP').unlink()
            corridor.run(root,'terrain',fetch=fetch)
            second=Path(tmp)/'second'
            corridor.create_from_snapshot(fixture(1000),second)
            self.assertGreater(corridor.seed_cache(second,root/'region-0000')['seeded_tiles'],0)
            def offline(*args):raise AssertionError('Should reuse verified download')
            corridor.run(second,'terrain',fetch=offline)

    def test_bake_receipts_resume_preserve_previous_package_and_pin_dependencies(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)/'job'
            corridor.create_from_snapshot(fixture(2000),root,region_m=1000)
            corridor.run(root,'terrain',fetch=Fetch())
            builder=Path(tmp)/'builder';builder.write_bytes(b'synthetic tool identity')
            calls=[]
            def bake(project,destination,*args):
                calls.append(Path(project).name)
                if len(calls)==2:raise ValueError('simulated bake failure')
                fake_package(project,destination)
            with patch.object(corridor.tiled,'build',side_effect=bake):
                with self.assertRaisesRegex(ValueError,'simulated'):corridor.run(root,'bake',builder)
                receipt=(root/'region-0000'/'bake.done.json').read_bytes()
                corridor.run(root,'bake',builder)
                self.assertEqual(calls,['region-0000','region-0001','region-0001'])
                self.assertEqual((root/'region-0000'/'bake.done.json').read_bytes(),receipt)
                corridor.run(root,'bake',builder)
                self.assertEqual(len(calls),3)
                builder.write_bytes(b'changed')
                with self.assertRaisesRegex(ValueError,'dependencies changed'):corridor.run(root,'bake',builder)

    def test_interrupted_publication_recovers_without_overwriting_and_review_changes_conflict(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)/'job'
            review=Path(tmp)/'review';review.mkdir()
            (review/'review.json').write_bytes(b'{"synthetic":true}')
            source=fixture(1000);source['review_revision']=corridor.digest_file(review/'review.json');source['sources']=[]
            corridor.create_from_snapshot(source,root,review)
            corridor.run(root,'terrain',fetch=Fetch())
            builder=Path(tmp)/'builder';builder.write_bytes(b'fixture')
            def interrupted(project,destination,*args):
                fake_package(project,destination)
                raise KeyboardInterrupt()
            with patch.object(corridor.tiled,'build',side_effect=interrupted) as build:
                with self.assertRaises(KeyboardInterrupt):corridor.run(root,'bake',builder)
                original=(root/'region-0000'/'preview.orpack'/'manifest.json').read_bytes()
                corridor.run(root,'bake',builder)
                self.assertEqual(build.call_count,1)
                self.assertEqual((root/'region-0000'/'preview.orpack'/'manifest.json').read_bytes(),original)
            (review/'review.json').write_bytes(b'{"synthetic":"changed"}')
            with self.assertRaisesRegex(ValueError,'Saved review changed'):corridor.load(root)

    def test_plan_tampering_and_concurrent_writer_are_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)/'job'
            corridor.create_from_snapshot(fixture(1000),root)
            (root/'job.lock').write_text('other writer')
            with self.assertRaisesRegex(ValueError,'locked'):corridor.run(root,'terrain',fetch=Fetch())
            (root/'job.lock').unlink()
            plan=root/'region-0000'/'plan.json'
            plan.write_bytes(plan.read_bytes()+b' ')
            with self.assertRaisesRegex(ValueError,'plan changed'):corridor.load(root)


if __name__=='__main__':unittest.main()
