"""Saved, local route authoring jobs. Acquisition is explicit; riding stays offline."""
import argparse
import copy
import contextlib
import hashlib
import importlib.util
import json
import math
import re
from pathlib import Path
import shutil
import subprocess
import sys
import traceback
import uuid

ROOT=Path(__file__).resolve().parent

def module(name):
    spec=importlib.util.spec_from_file_location(name.replace('-','_'),ROOT/(name+'.py'))
    value=importlib.util.module_from_spec(spec);spec.loader.exec_module(value);return value

def read(path):
    path=Path(path)
    if path.stat().st_size>64*1024*1024:raise ValueError('File exceeds 64 MiB')
    return json.loads(path.read_bytes())

def save(path,value):
    path=Path(path);tmp=path.with_suffix(path.suffix+'.tmp')
    tmp.write_text(json.dumps(value,indent=2,allow_nan=False),encoding='utf-8');tmp.replace(path)

def run_builder(builder,*args):
    subprocess.run([str(builder),*map(str,args)],check=True,stdout=sys.stdout,stderr=sys.stderr)

def sync_project(directory,state):
    """Publish stable editing documents from the committed stage snapshots."""
    def prep(key):return read(directory/state[key]/'preparation.json')
    context=directory/state['context']/'preparation.json' if 'context' in state else None
    corrections=prep('corrected')['corrections'] if 'corrected' in state else dict(kind='openride.scenery-corrections',schema_version=1,preparation_sha256=hashlib.sha256(context.read_bytes()).hexdigest() if context else None,elevation_offset_m=0,road_choices=[])
    corrected=directory/state['corrected']/'preparation.json' if 'corrected' in state else None
    repairs=prep('reviewed')['profile_repairs'] if 'reviewed' in state else dict(kind='openride.profile-repairs',schema_version=1,preparation_sha256=hashlib.sha256(corrected.read_bytes()).hexdigest() if corrected else None,spans=[])
    for name,value in [('corrections.json',corrections),('repairs.json',repairs)]:
        path=directory/name
        if not state.get('managed_documents') and path.exists() and read(path)!=value:
            shutil.copyfile(path,directory/(path.stem+'-before-migration-'+uuid.uuid4().hex[:8]+'.json'))
        save(path,value)
    state['managed_documents']=True
    if 'imported' in state and 'route_source' not in state:
        expected=prep('imported')['source_route_sha256']
        for path in directory.glob('source-*/source.orroute'):
            if hashlib.sha256(path.read_bytes()).hexdigest()==expected:
                state['route_source']=str(path.relative_to(directory));break
    if 'context' in state:
        p=prep('reviewed' if 'reviewed' in state else 'corrected' if 'corrected' in state else 'context')
        points=p.get('reviewed_profile',p.get('corrected_profile',p))['points']
        selected={c['source_sample'] for c in corrections['road_choices']}
        kept=sum(p['source_sample'] not in selected for p in points)
        state['editing_status']=dict(road_choices=len(selected),kept_original_points=kept,repair_spans=len(repairs['spans']))
    save(directory/'project.json',state)

def carry_repairs(previous, corrected, destination):
    """Keep accepted span intent after other road edits; rerun all repair guards."""
    proposal=copy.deepcopy(read(previous)['profile_repairs'])
    proposal['previous_preparation_sha256']=proposal['preparation_sha256']
    proposal['preparation_sha256']=hashlib.sha256(Path(corrected).read_bytes()).hexdigest()
    proposal_path=Path(corrected).parent/'carried-repairs.json'
    save(proposal_path,proposal)
    module('repair-profile').build(corrected,proposal_path,destination)

def refresh_candidates(directory,state):
    """Upgrade cached candidates without downloading or losing accepted edits."""
    if 'context' not in state:return
    context=directory/state['context']/'preparation.json'
    if read(context).get('osm_context',{}).get('candidate_version',1)>=2:return
    if 'terrain' not in state:return
    updated=copy.deepcopy(state)
    output=directory/('context-'+uuid.uuid4().hex[:10])
    module('osm-context').build(directory/state['terrain']/'preparation.json',directory/'sources',output)
    updated['context']=output.name
    if 'corrected' in state:
        document=read(directory/state['corrected']/'preparation.json')['corrections']
        document['preparation_sha256']=hashlib.sha256((output/'preparation.json').read_bytes()).hexdigest()
        incoming=output/'carried-corrections.json';save(incoming,document)
        corrected=directory/('corrected-'+uuid.uuid4().hex[:10])
        module('correct-scenery').build(output/'preparation.json',incoming,corrected)
        updated['corrected']=corrected.name
        if 'reviewed' in state:
            reviewed=directory/('reviewed-'+uuid.uuid4().hex[:10])
            carry_repairs(directory/state['reviewed']/'preparation.json',corrected/'preparation.json',reviewed)
            updated['reviewed']=reviewed.name
    for key in ('profile','package','previewed_package','jitter'):updated.pop(key,None)
    state.clear();state.update(updated)
    print('Refreshed all road alternatives within 30 m; retained saved edits.',flush=True)

def jitter_report(directory,state,builder):
    source=directory/state['corrected']/'preparation.json'
    preparation=read(source);fingerprint=hashlib.sha256(source.read_bytes()).hexdigest()
    report_path=directory/('jitter-'+uuid.uuid4().hex[:10]+'.json')
    paths=[directory/state[k] for k in ('source_copy',) if k in state]
    if state.get('source_path'):paths.append(Path(state['source_path']))
    # Older projects did not retain the original; try the workspace's import folder.
    filename=Path(state.get('source_filename','')).name
    if filename:
        paths.extend(root/'import'/filename for root in (ROOT.parent,ROOT.parent.parent))
    original=next((p for p in paths if p.is_file() and hashlib.sha256(p.read_bytes()).hexdigest()==preparation.get('source_activity_sha256')),None)
    if original is None:
        raise ValueError('Stop detection needs the original GPX/TCX with timestamps. This older project has no accessible matching source; manual removal remains available.')
    evidence=directory/('evidence-'+uuid.uuid4().hex[:10]+'.json')
    run_builder(builder,'--activity-evidence',original,evidence)
    report=module('detect-jitter').detect(preparation,read(evidence),fingerprint)
    spans=read(directory/state['reviewed']/'preparation.json')['profile_repairs']['spans'] if 'reviewed' in state else []
    report['candidates']=[c for c in report['candidates'] if not any(max(s['start_source_sample'],c['span']['start_source_sample'])<min(s['end_source_sample'],c['span']['end_source_sample']) for s in spans)]
    save(report_path,report);state['jitter']=report_path.name
    print(f"Stop detection: {len(report['candidates'])} candidates; {len(report['rejected'])} rejected. No removals applied.",flush=True)

def project_height(p):
    """Explicit DEM heights at unchanged XY anchors. No claim of surveyed road heights."""
    from building_scenery import ground_sampler
    terrain=p['terrain']
    if terrain.get('nodata_pixels',0):raise ValueError('Terrain has missing elevations; projection is unavailable')
    sample=ground_sampler(terrain['mesh'],terrain['width'],terrain['height'])
    base=copy.deepcopy(p.get('reviewed_profile',p['corrected_profile']))
    matches={m['source_sample']:m['candidates'] for m in p.get('osm_context',{}).get('matches',[])}
    for point in base['points']:
        for candidate in matches.get(point.get('source_sample'),[]):
            tags=candidate.get('tags',{})
            if candidate.get('distance_m',999)<12 and (tags.get('bridge','no')!='no' or tags.get('tunnel','no')!='no'):
                raise ValueError('A nearby bridge/tunnel makes terrain heights ambiguous; retain recorded heights')
        x,_,z=point['position'];y=sample(x,z)
        if y is None or not math.isfinite(y):raise ValueError('Route lies outside valid terrain')
        point['position'][1]=y;point['elevation_m']=y+p['origin_lat_lon_elevation'][2]
    # Recompute grades, stationing and mesh through the same correction validator.
    temporary=copy.deepcopy(p);temporary.pop('corrections',None);temporary['points']=base['points']
    c=dict(kind='openride.scenery-corrections',schema_version=1,preparation_sha256='height-pass',elevation_offset_m=0,road_choices=[])
    result=module('correct-scenery').apply(temporary,c,'height-pass')
    result['elevation_method']='Explicit bilinear terrain projection; ground surface, not surveyed road elevation'
    p['reviewed_profile']=result
    return p

def execute(request):
    directory=Path(request['project']).resolve();directory.mkdir(parents=True,exist_ok=True)
    marker=directory/'project.json';action=request['action']
    state=read(marker) if marker.exists() else dict(kind='openride.authoring-project',schema_version=1)
    if state.get('kind')!='openride.authoring-project' or state.get('schema_version')!=1:raise ValueError('Unsupported project')
    builder=Path(request.get('builder',ROOT/'content-builder.exe'))
    def fresh(name):return directory/(name+'-'+uuid.uuid4().hex[:10])
    def preparation(key):return directory/state[key]/'preparation.json'
    def record(key,path):state[key]=str(Path(path).relative_to(directory))
    def invalidate(*keys):
        for key in keys:state.pop(key,None)
    print('Stage: '+action,flush=True)
    if action=='open':
        if not marker.exists() and any(p.name not in ('job.json','job.log','request.json','process.json') for p in directory.iterdir()):raise ValueError('Choose an empty folder for a new project')
        refresh_candidates(directory,state)
    elif action=='import':
        if 'imported' in state:raise ValueError('Choose a new empty project folder for a different source or interval')
        if not marker.exists() and any(p.name not in ('job.json','job.log','request.json','process.json') for p in directory.iterdir()):raise ValueError('Choose an empty project folder')
        save(marker,state)
        source=Path(request['source']).resolve();working=fresh('source');working.mkdir();destination=working/'source.orroute'
        original=working/('original'+source.suffix.lower());shutil.copyfile(source,original)
        if source.suffix.lower()=='.orroute':shutil.copyfile(source,destination)
        else:run_builder(builder,'--import-route',source,destination)
        route=read(destination);length=route['points'][-1]['distance_m']
        start=float(request.get('start_m',0));end=min(float(request.get('end_m',5000)),length)
        output=fresh('import');run_builder(builder,destination,output,start,end)
        record('imported',output);state.update(name=route['name'],source_filename=source.name,interval_m=[start,end])
        record('source_copy',original);record('route_source',destination);state['source_path']=str(source)
    elif action in ('terrain','osm'):
        source=preparation('imported');cache=directory/'sources'
        report=module('acquire-scenery').acquire(source,cache,stage=action)
        if action in report['errors']:raise ValueError(report['errors'][action])
        if action=='osm' and 'terrain.tif' not in report['files']:raise ValueError('Acquire terrain first')
        if 'terrain' in state and read(preparation('terrain'))['terrain']['source_snapshot']==report and (action=='terrain' or 'context' in state):
            print('Verified sources already attached; retaining repairs and preview',flush=True);sync_project(directory,state);return state
        output=fresh('terrain');run_builder(builder,'--terrain',source,cache,output)
        record('terrain',output)
        invalidate('context','corrected','reviewed','profile','package','previewed_package')
        if 'map.osm' in report['files']:
            context=fresh('context');module('osm-context').build(output/'preparation.json',cache,context);record('context',context)
    elif action=='imagery':
        grid=int(request.get('grid',4));cache=directory/f'imagery-{grid}'
        module('acquire-imagery').acquire_tiled(preparation('terrain'),cache,grid=grid)
        record('imagery',cache/'imagery.json');invalidate('package','previewed_package')
    elif action in ('edits','jitter'):
        # Validate both kinds of draft edits before publishing either stage.
        output=fresh('corrected');incoming=fresh('corrections-draft').with_suffix('.json')
        save(incoming,request['document'])
        module('correct-scenery').build(preparation('context'),incoming,output)
        proposal=dict(kind='openride.profile-repairs',schema_version=1,preparation_sha256=hashlib.sha256((output/'preparation.json').read_bytes()).hexdigest(),spans=request['spans'])
        repair_path=output/'draft-repairs.json';save(repair_path,proposal)
        reviewed=fresh('reviewed');module('repair-profile').build(output/'preparation.json',repair_path,reviewed)
        record('corrected',output);record('reviewed',reviewed)
        invalidate('profile','package','previewed_package','jitter')
        if action=='jitter':jitter_report(directory,state,builder)
    elif action=='corrections':
        output=fresh('corrected')
        incoming=request.get('corrections',directory/'corrections.json')
        if 'document' in request:
            incoming=fresh('corrections-draft').with_suffix('.json');save(incoming,request['document'])
        module('correct-scenery').build(preparation('context'),incoming,output)
        reviewed=None
        if 'reviewed' in state:
            reviewed=fresh('reviewed')
            try:carry_repairs(preparation('reviewed'),output/'preparation.json',reviewed)
            except ValueError as error:
                raise ValueError(f'New corrections conflict with saved point removals: {error}. Previous project state retained; review those spans before changing their road choices.') from error
        record('corrected',output);invalidate('reviewed','profile','package','previewed_package')
        if reviewed:
            record('reviewed',reviewed);print('Retained saved point removals and rechecked their geometry.',flush=True)
    elif action=='repairs':
        output=fresh('reviewed')
        incoming=request.get('repairs',directory/'repairs.json')
        if 'document' in request:
            incoming=fresh('repairs-draft').with_suffix('.json');save(incoming,request['document'])
        module('repair-profile').build(preparation('corrected'),incoming,output)
        record('reviewed',output);invalidate('profile','package','previewed_package')
    elif action=='build':
        if 'corrected' not in state:
            sync_project(directory,state)
            output=fresh('corrected')
            module('correct-scenery').build(preparation('context'),directory/'corrections.json',output)
            record('corrected',output)
        state['geometry']=request.get('geometry','road');state['elevation']=request.get('elevation','recorded');state['connect_gaps']=request.get('connect_gaps',False) is True
        save(marker,state)
        source=preparation('reviewed' if 'reviewed' in state else 'corrected')
        p=read(source)
        if request.get('elevation','recorded')=='terrain':
            p=project_height(p);height=fresh('height');height.mkdir();save(height/'preparation.json',p);source=height/'preparation.json'
        mode=request.get('geometry','road')
        if mode not in ('road','gps'):raise ValueError('Unknown geometry mode')
        output=fresh('profile');profile=module('create-route-profile').build(source,directory/'sources/map.osm',output,mode,connect_gaps=state['connect_gaps'])
        for gap in profile.get('road_gap_connectors',[]):
            print(f"Straight connector: points {gap['start_point']}–{gap['end_point']}, {gap['length_m']:.1f} m. Inspect this join in preview.",flush=True)
        for note in profile.get('access_notes',[]):
            print(f"Indoor route uses explicitly selected OSM way {note['way_id']} with outdoor access tags {note['tags']}; informational only.",flush=True)
        # Keep a reviewable profile even if remaining findings prevent packaging.
        record('profile',output);invalidate('package','previewed_package');save(marker,state)
        if profile['findings']:raise ValueError(f"{len(profile['findings'])} unresolved route findings. Open profile review, repair the corrected route, then build again.")
        package=fresh('preview').with_suffix('.orpack');osm=directory/'sources/map.osm'
        imagery=directory/state['imagery'] if request.get('use_imagery',True) and 'imagery' in state else None
        module('chunk-scenery').build(output/'preparation.json',package,True,osm,osm,osm,imagery)
        manifest=read(package/'manifest.json')
        manifest['name']=state['name']+' / '+('aerial '+read(imagery)['request']['size'] if imagery else 'OSM ground')+' / '+package.stem[-10:]
        save(package/'manifest.json',manifest);record('package',package)
        state['geometry']=mode;state['elevation']=request.get('elevation','recorded')
    elif action=='install':
        if request.get('reviewed_package')!=state.get('package'):raise ValueError('Preview the current package and confirm it before installing')
        destination=Path(request['library']).resolve();destination.mkdir(parents=True,exist_ok=True)
        name=re.sub(r'[^A-Za-z0-9 _.-]','-',state['name'])[:40].strip(' .') or 'Route'
        source=directory/state['package'];target=destination/(name+'-'+uuid.uuid4().hex[:8]+'.orpack')
        # Copy into a non-package directory; library sees it only after completion.
        staging=destination/('install-'+uuid.uuid4().hex)
        shutil.copytree(source,staging);staging.rename(target);state['installed']=str(target)
    else:raise ValueError('Unknown authoring stage')
    sync_project(directory,state);print('Stage complete: '+action,flush=True)
    return state

def main():
    parser=argparse.ArgumentParser();parser.add_argument('request');args=parser.parse_args()
    request=read(args.request);directory=Path(request['project']);directory.mkdir(parents=True,exist_ok=True)
    with (directory/'job.log').open('w',encoding='utf-8',buffering=1) as log, contextlib.redirect_stdout(log), contextlib.redirect_stderr(log):
        save(directory/'job.json',dict(status='running',action=request['action']))
        try:
            execute(request);save(directory/'job.json',dict(status='complete',action=request['action']))
        except Exception as error:
            traceback.print_exc();save(directory/'job.json',dict(status='failed',action=request['action'],error=str(error)));return 1
    return 0

if __name__=='__main__':sys.exit(main())
