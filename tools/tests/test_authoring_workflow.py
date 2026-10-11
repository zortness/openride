import copy
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0,str(Path(__file__).parents[1]))
import png_tiles
import importlib.util
spec=importlib.util.spec_from_file_location('workflow',Path(__file__).parents[1]/'route-workflow.py')
workflow=importlib.util.module_from_spec(spec);spec.loader.exec_module(workflow)

class TiledImagery(unittest.TestCase):
    def test_assembly_keeps_north_west_order_and_checks_corruption(self):
        tiles=[png_tiles.encode(2,2,bytes(color)*4) for color in [(255,0,0),(0,255,0),(0,0,255),(255,255,0)]]
        w,h,pixels=png_tiles.decode(png_tiles.assemble(tiles,2,2))
        self.assertEqual((w,h),(4,4))
        for x,y,color in [(0,0,[255,0,0]),(3,0,[0,255,0]),(0,3,[0,0,255]),(3,3,[255,255,0])]:
            self.assertEqual(list(pixels[(y*w+x)*3:(y*w+x)*3+3]),color)
        with self.assertRaises(ValueError):png_tiles.decode(tiles[0][:-4])

    def test_resume_requests_only_missing_tiles_and_binds_cache(self):
        imagery=workflow.module('acquire-imagery');calls=[]
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);source=root/'source.json'
            p=dict(terrain={'extent':dict(xmin=-118.01,xmax=-118.,ymin=33.,ymax=33.01)},origin_lat_lon_elevation=[33.,-118.,0.])
            workflow.save(source,p)
            def acquire(preparation,destination,*args):
                bounds=workflow.read(preparation)['terrain']['extent'];calls.append(bounds)
                if len(calls)==2:raise TimeoutError('tile interrupted')
                destination.mkdir();raw=png_tiles.encode(512,512,bytes([len(calls),0,0])*512*512)
                (destination/'imagery.png').write_bytes(raw)
                workflow.save(destination/'imagery.json',dict(bounds=bounds,sha256=hashlib.sha256(raw).hexdigest()))
            with patch.object(imagery,'acquire',side_effect=acquire):
                with self.assertRaises(TimeoutError):imagery.acquire_tiled(source,root/'cache',grid=2)
                self.assertFalse((root/'cache/imagery.json').exists())
                imagery.acquire_tiled(source,root/'cache',grid=2)
                self.assertEqual(len(calls),5) # one completed + failed + three missing
                imagery.acquire_tiled(source,root/'cache',grid=2)
                self.assertEqual(len(calls),5)
                self.assertEqual(png_tiles.decode((root/'cache/imagery.png').read_bytes())[:2],(1024,1024))
                p['origin_lat_lon_elevation'][2]=1;workflow.save(source,p)
                with self.assertRaisesRegex(ValueError,'different bounds'):imagery.acquire_tiled(source,root/'cache',grid=2)

class Authoring(unittest.TestCase):
    def test_combined_edit_validation_is_atomic(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);(root/'context').mkdir()
            points=[dict(source_sample=i,position=[i*10,0,0],elevation_m=0,break_before=i==0) for i in range(4)]
            p=dict(kind='openride.scenery-preparation',schema_version=1,trainer_ready=False,points=points,files=[],origin_lat_lon_elevation=[0,0,0])
            workflow.save(root/'context/preparation.json',p)
            state=dict(kind='openride.authoring-project',schema_version=1,context='context')
            workflow.save(root/'project.json',state)
            document=dict(kind='openride.scenery-corrections',schema_version=1,preparation_sha256=hashlib.sha256((root/'context/preparation.json').read_bytes()).hexdigest(),elevation_offset_m=2,road_choices=[])
            spans=[dict(method='manual_remove',start_source_sample=0,end_source_sample=2,reason='Reviewed')]
            result=workflow.execute(dict(project=tmp,action='edits',document=document,spans=spans))
            saved=workflow.read(root/result['reviewed']/'preparation.json')
            self.assertEqual([p['source_sample'] for p in saved['reviewed_profile']['points']],[0,2,3])
            self.assertEqual(saved['reviewed_profile']['points'][0]['position'][1],2)
            prior=(root/'project.json').read_bytes();repairs=(root/'repairs.json').read_bytes()
            spans[0]['end_source_sample']=999
            with self.assertRaises(ValueError):workflow.execute(dict(project=tmp,action='edits',document=document,spans=spans))
            self.assertEqual((root/'project.json').read_bytes(),prior)
            self.assertEqual((root/'repairs.json').read_bytes(),repairs)

    def test_import_retains_original_source_even_if_external_file_disappears(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);source=root/'activity.gpx';source.write_bytes(b'<gpx>retained source bytes</gpx>')
            project=root/'project'
            def builder(_builder,*args):
                if args[0]=='--import-route':workflow.save(args[2],dict(name='Test',points=[dict(distance_m=0),dict(distance_m=100)]))
                else:
                    output=Path(args[1]);output.mkdir()
                    workflow.save(output/'preparation.json',dict(source_route_sha256=hashlib.sha256(Path(args[0]).read_bytes()).hexdigest()))
            with patch.object(workflow,'run_builder',side_effect=builder):
                state=workflow.execute(dict(project=str(project),action='import',source=str(source),end_m=100))
            source.unlink()
            reopened=workflow.execute(dict(project=str(project),action='open'))
            self.assertEqual(reopened['source_filename'],'activity.gpx')
            self.assertEqual((project/reopened['source_copy']).read_bytes(),b'<gpx>retained source bytes</gpx>')
            self.assertEqual(reopened['interval_m'],[0,100])
            self.assertEqual(reopened['route_source'],state['route_source'])

    def test_new_project_creates_stable_empty_documents_and_reopens(self):
        with tempfile.TemporaryDirectory() as tmp:
            state=workflow.execute(dict(project=tmp,action='open'))
            root=Path(tmp)
            self.assertTrue(state['managed_documents'])
            self.assertEqual(workflow.read(root/'corrections.json')['road_choices'],[])
            self.assertEqual(workflow.read(root/'repairs.json')['spans'],[])
            self.assertEqual(workflow.execute(dict(project=tmp,action='open')),state)

    def test_migration_recovers_accepted_documents_and_preserves_external_draft(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)
            for name in ['context','corrected','reviewed']:(root/name).mkdir()
            correction=dict(road_choices=[dict(source_sample=0)])
            repair=dict(spans=[dict(start_source_sample=0,end_source_sample=2)])
            workflow.save(root/'context/preparation.json',{})
            workflow.save(root/'corrected/preparation.json',dict(corrections=correction))
            workflow.save(root/'reviewed/preparation.json',dict(profile_repairs=repair,reviewed_profile=dict(points=[dict(source_sample=0),dict(source_sample=2)])))
            workflow.save(root/'corrections.json',dict(external_draft=True))
            state=dict(kind='openride.authoring-project',schema_version=1,context='context',corrected='corrected',reviewed='reviewed')
            workflow.save(root/'project.json',state)
            result=workflow.execute(dict(project=tmp,action='open'))
            self.assertEqual(workflow.read(root/'corrections.json'),correction)
            self.assertEqual(workflow.read(root/'repairs.json'),repair)
            self.assertEqual(result['editing_status']['kept_original_points'],1)
            self.assertEqual(len(list(root.glob('corrections-before-migration-*.json'))),1)

    def test_carry_repairs_keeps_explicit_spans_and_revalidates_changes(self):
        from test_profile_repairs import Repairs
        fixture=Repairs();fixture.setUp()
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);old=root/'previous.json';new=root/'corrected.json'
            workflow.save(old,dict(profile_repairs=fixture.proposal))
            workflow.save(new,fixture.prep)
            workflow.carry_repairs(old,new,root/'reviewed')
            result=workflow.read(root/'reviewed/preparation.json')
            self.assertEqual([p['source_sample'] for p in result['reviewed_profile']['points']],[0,1,3,4])
            self.assertEqual(result['profile_repairs']['previous_preparation_sha256'],'hash')
            fixture.prep['corrections']['road_choices'][2]['way_id']='other'
            workflow.save(new,fixture.prep)
            with self.assertRaisesRegex(ValueError,'one OSM way'):workflow.carry_repairs(old,new,root/'bad')

    def test_terrain_height_changes_only_height_and_recomputes_grade(self):
        p=dict(kind='openride.scenery-preparation',schema_version=1,trainer_ready=False,
               origin_lat_lon_elevation=[0,0,100],terrain=dict(width=2,height=2,nodata_pixels=0,mesh=dict(vertices=[[0,0,0],[20,2,0],[0,0,20],[20,2,20]],indices=[])),
               corrected_profile=dict(points=[dict(source_sample=i,position=[x,None,5],elevation_m=None,break_before=i==0) for i,x in enumerate([2,12])]))
        original=copy.deepcopy(p);result=workflow.project_height(p)['reviewed_profile']
        self.assertEqual(result['points'][0]['position'],[2,.2,5])
        self.assertAlmostEqual(result['points'][1]['grade_percent'],10)
        self.assertEqual(result['findings'],[])
        self.assertEqual(p['corrected_profile'],original['corrected_profile'])
        p['terrain']['nodata_pixels']=1
        with self.assertRaisesRegex(ValueError,'missing elevations'):workflow.project_height(p)

    def test_failed_stage_does_not_advance_project(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);state=dict(kind='openride.authoring-project',schema_version=1,imported='imported')
            workflow.save(root/'project.json',state)
            with patch.object(workflow,'module') as module:
                module.return_value.acquire.return_value={'errors':{'terrain':'offline'}}
                with self.assertRaisesRegex(ValueError,'offline'):workflow.execute(dict(project=tmp,action='terrain'))
            self.assertEqual(workflow.read(root/'project.json'),state)

    def test_install_requires_current_preview_and_never_overwrites(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp);package=root/'preview.orpack';package.mkdir();(package/'test').write_text('payload')
            state=dict(kind='openride.authoring-project',schema_version=1,name='Example',package='preview.orpack')
            workflow.save(root/'project.json',state)
            request=dict(project=tmp,action='install',library=str(root/'library'),reviewed_package='old.orpack')
            with self.assertRaisesRegex(ValueError,'Preview'):workflow.execute(request)
            request['reviewed_package']='preview.orpack'
            first=workflow.execute(request)['installed'];second=workflow.execute(request)['installed']
            self.assertNotEqual(first,second);self.assertEqual((Path(first)/'test').read_text(),'payload')

if __name__=='__main__':unittest.main()
