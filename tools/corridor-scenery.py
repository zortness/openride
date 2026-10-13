"""Resumable full-recording terrain/OSM preparation in bounded regional jobs.

Regional packages are build artifacts, not yet one continuous rider package.
All snapshots, caches, status and generated assets are private authoring data.
"""
import argparse
from contextlib import contextmanager
import copy
import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import subprocess
import sys
import uuid

sys.path.insert(0, str(Path(__file__).parent))
import importlib.util
spec = importlib.util.spec_from_file_location('tiled', Path(__file__).with_name('tiled-scenery.py'))
tiled = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tiled)

MAX_REGIONS = 512
BUDGET_ERRORS = ('coordinate budget', 'stationing tolerance', 'point budget', '40 source tiles')


def digest_file(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def atomic(path, value):
    path = Path(path)
    temporary = path.with_name(path.name + '.' + uuid.uuid4().hex + '.tmp')
    with temporary.open('xb') as stream:
        stream.write(tiled.encoded(value))
        stream.flush()
        os.fsync(stream.fileno())
    temporary.replace(path)


def document(path):
    return json.loads(tiled.read(path))


def safe_file(root, name):
    if not isinstance(name, str) or not name or Path(name).name != name or any(c in name for c in '/\\:') or name in ('.', '..'):
        raise ValueError('Unsafe asset path')
    path = Path(root) / name
    if path.is_symlink() or not path.is_file():
        raise ValueError('Missing or unsafe asset: ' + name)
    return path


def inventory(directory):
    result = {}
    for path in sorted(Path(directory).iterdir()):
        if not path.is_file() or path.is_symlink():
            raise ValueError('Expected flat regular-file artifact directory')
        result[path.name] = dict(bytes=path.stat().st_size, sha256=digest_file(path))
    return result


def verify_inventory(directory, files):
    if not isinstance(files, dict) or not files:
        raise ValueError('Missing artifact checksums')
    for name, entry in files.items():
        path = safe_file(directory, name)
        if path.stat().st_size != entry['bytes'] or digest_file(path) != entry['sha256']:
            raise ValueError('Artifact checksum mismatch: ' + str(path))


@contextmanager
def lock(project):
    path = Path(project) / 'job.lock'
    try:
        with path.open('x', encoding='utf-8') as stream:
            stream.write(f'pid={os.getpid()}\n')
    except FileExistsError:
        raise ValueError('Job locked; after a forced termination confirm the writer exited before removing job.lock') from None
    try:
        yield
    finally:
        path.unlink()


def regions(source, region_m):
    if not math.isfinite(region_m) or not 250 <= region_m <= 12000:
        raise ValueError('Region length must be 250–12,000 m')
    if source.get('issues') or source.get('unresolved_breaks', 0):
        raise ValueError('Full corridor requires a reviewed profile without unresolved findings')
    length = source['points'][-1]['distance_m']
    if not math.isfinite(length) or not 0 < length <= 2_000_000:
        raise ValueError('Corridor distance must be positive and at most 2,000 km')
    start = 0.0
    count = 0
    while start < length:
        end = min(start + region_m, length)
        while True:
            try:
                plan = tiled.plan(source, start, end)
                break
            except ValueError as error:
                if not any(text in str(error) for text in BUDGET_ERRORS) or end-start <= 250:
                    plan = dict(blocked=str(error), requested_distance_m=[start,end])
                    break
                end = start + (end-start)/2
        if count >= MAX_REGIONS:
            raise ValueError('Corridor exceeds 512 bounded regions')
        yield plan
        start = end
        count += 1


def create_from_snapshot(source, destination, review=None, region_m=8000):
    """Exclusive staged publication; a failed plan never looks like a resumable job."""
    destination = Path(destination)
    if destination.exists():
        raise FileExistsError('Choose a new corridor project')
    staging = destination.with_name(destination.name + '.planning-' + uuid.uuid4().hex)
    staging.mkdir()
    raw = tiled.encoded(source)
    tiled.write_new(staging / 'reviewed-profile.json', raw)
    records = []
    previous = None
    for index, plan in enumerate(regions(source, region_m)):
        name = f'region-{index:04d}'
        path = staging / name
        path.mkdir()
        tiled.write_new(path / 'reviewed-profile.json', raw)
        if 'blocked' in plan:
            start,end=plan['requested_distance_m']
            records.append(dict(id=name,start_m=start,end_m=end,blocked=plan['blocked']))
            previous=None
            continue
        tiled.write_plan(path, plan)
        points = plan['preparation']['points']
        start, end = plan['preparation']['requested_distance_m']
        endpoint = lambda p: [p[k] for k in ('source_distance_m', 'latitude', 'longitude', 'elevation_m')]
        if previous is not None and previous != endpoint(points[0]):
            raise ValueError('Region boundary changes canonical geography/height/station')
        previous = endpoint(points[-1])
        records.append(dict(id=name, start_m=start, end_m=end,
                            plan_sha256=digest_file(path / 'plan.json'),
                            tiles=len(plan['tiles']), points=len(points),
                            steep_segments=len(plan['steep_segments']),
                            max_abs_grade_percent=max(abs(p['grade_percent'] or 0) for p in points)))
    job = dict(kind='openride.corridor-preparation', schema_version=1,
               review_directory=str(Path(review).resolve()) if review else None,
               review_revision=source['review_revision'], profile_sha256=tiled.sha(raw),
               length_m=source['points'][-1]['distance_m'], regions=records,
               output='Independent regional preview packages; whole-route rider assembly remains open',
               terrain_seams='Regional grids have local origins; cross-region terrain reconciliation remains open')
    job['grade_findings']=[]
    for a,b in zip(source['points'],source['points'][1:]):
        distance=b['distance_m']-a['distance_m']
        if distance>0:
            grade=100*(b['elevation_m']-a['elevation_m'])/distance
            if abs(grade)>15:
                job['grade_findings'].append(dict(before_sample=a['source_sample'],after_sample=b['source_sample'],
                    start_m=a['distance_m'],end_m=b['distance_m'],horizontal_m=distance,
                    before_height_m=a['elevation_m'],after_height_m=b['elevation_m'],grade_percent=grade,
                    severity='blocked' if abs(grade)>100 else 'review'))
    tiled.write_new(staging / 'corridor.json', tiled.encoded(job))
    (staging / 'cache').mkdir()
    staging.rename(destination)
    return job


def create(review, destination, builder, region_m=8000):
    destination = Path(destination)
    if destination.exists():
        raise FileExistsError('Choose a new corridor project')
    snapshot = destination.with_name(destination.name + '.profile-' + uuid.uuid4().hex + '.json')
    subprocess.run([str(builder), '--review-profile', str(review), str(snapshot)], check=True, timeout=60)
    return create_from_snapshot(document(snapshot), destination, review, region_m)


def load(project):
    project = Path(project)
    job = document(project / 'corridor.json')
    if job.get('kind') != 'openride.corridor-preparation' or job.get('schema_version') != 1:
        raise ValueError('Unsupported corridor job')
    if digest_file(project / 'reviewed-profile.json') != job['profile_sha256']:
        raise ValueError('Corridor profile changed')
    source = document(project / 'reviewed-profile.json')
    if source['review_revision'] != job['review_revision'] or source['points'][-1]['distance_m'] != job['length_m']:
        raise ValueError('Corridor profile identity mismatch')
    if job.get('review_directory'):
        review = Path(job['review_directory'])
        if digest_file(review / 'review.json') != job['review_revision']:
            raise ValueError('Saved review changed; create a new corridor plan for the new revision')
        for entry in source['sources']:
            if digest_file(safe_file(review, entry['file'])) != entry['sha256']:
                raise ValueError('Source snapshot changed')
    if not 1 <= len(job['regions']) <= MAX_REGIONS:
        raise ValueError('Invalid region count')
    end = 0.0
    for index, region in enumerate(job['regions']):
        if region['id'] != f'region-{index:04d}' or region['start_m'] != end or not end < region['end_m'] <= job['length_m']:
            raise ValueError('Invalid region order or coverage')
        path = project / region['id']
        if path.is_symlink():
            raise ValueError('Unsafe regional directory')
        if region.get('blocked'):
            if digest_file(path / 'reviewed-profile.json') != job['profile_sha256']:
                raise ValueError('Regional source profile changed')
            try:
                tiled.plan(source,region['start_m'],region['end_m'])
            except ValueError as error:
                if str(error)!=region['blocked']:
                    raise ValueError('Blocked region finding changed') from error
            else:
                raise ValueError('Region is incorrectly marked blocked')
            end=region['end_m']
            continue
        if digest_file(path / 'plan.json') != region['plan_sha256']:
            raise ValueError('Regional plan changed')
        if digest_file(path / 'reviewed-profile.json') != job['profile_sha256']:
            raise ValueError('Regional source profile changed')
        plan = tiled.load_project(path)
        if plan['preparation']['requested_distance_m'] != [region['start_m'], region['end_m']]:
            raise ValueError('Region station range changed')
        end = region['end_m']
    if end != job['length_m']:
        raise ValueError('Incomplete corridor coverage')
    return job


def cache_request(preparation):
    acquisition = tiled.module('acquire-scenery')
    raster, query = acquisition.plan(preparation)
    return dict(terrain_service=acquisition.USGS, terrain_export=raster,
                osm_endpoint=acquisition.OVERPASS, osm_query=query)


def verified_sources(directory, preparation, stage):
    manifest = document(Path(directory) / 'sources.json')
    raw = Path(preparation).read_bytes()
    if manifest.get('kind') != 'openride.geographic-sources' or manifest.get('schema_version') != 1 or manifest['preparation_sha256'] != tiled.sha(raw):
        raise ValueError('Source cache identity mismatch')
    p = json.loads(raw)
    if manifest['source_route_sha256'] != p['source_route_sha256']:
        raise ValueError('Source route identity mismatch')
    request = cache_request(p)
    if manifest['requests']['terrain_export'] != request['terrain_export']:
        raise ValueError('Source request mismatch')
    verify_inventory(directory, manifest['files'])
    required = ['terrain.tif', 'usgs-export.json'] if stage == 'terrain' else ['map.osm']
    if any(name not in manifest['files'] for name in required) or manifest.get('errors', {}).get(stage):
        raise ValueError('Incomplete ' + stage + ' acquisition')
    return manifest


def acquire_tile(project, directory, stage, fetch=None):
    """Cache by exact geographic request, then bind verified bytes to each preparation."""
    preparation = directory / 'preparation.json'
    p = document(preparation)
    request = cache_request(p)
    key = tiled.sha(tiled.encoded(request))
    cache = Path(project) / 'cache' / key
    cache.mkdir(exist_ok=True)
    seed = dict(kind='openride.scenery-preparation', schema_version=1, trainer_ready=False,
                source_route_sha256=key, acquisition_bounds=p['acquisition_bounds'])
    seed_raw = tiled.encoded(seed)
    seed_path = cache / 'preparation.json'
    if not seed_path.exists():
        tiled.write_new(seed_path, seed_raw)
    if seed_path.read_bytes() != seed_raw:
        raise ValueError('Geographic cache preparation changed')
    acquisition = tiled.module('acquire-scenery')
    kwargs = dict(stage=stage)
    if fetch is not None:
        kwargs['fetch'] = fetch
    result = acquisition.acquire(seed_path, cache / 'sources', **kwargs)
    if result.get('errors', {}).get(stage):
        raise ValueError(str(result['errors'][stage]))
    manifest = verified_sources(cache / 'sources', seed_path, stage)
    sources = directory / 'sources'
    sources.mkdir(exist_ok=True)
    for name in manifest['files']:
        original = safe_file(cache / 'sources', name)
        temporary = sources / (name + '.' + uuid.uuid4().hex + '.tmp')
        shutil.copyfile(original, temporary)
        temporary.replace(sources / name)
    rebound = copy.deepcopy(manifest)
    rebound.update(preparation_sha256=digest_file(preparation), source_route_sha256=p['source_route_sha256'], geographic_cache_key=key)
    atomic(sources / 'sources.json', rebound)
    return verified_sources(sources, preparation, stage)


def seed_cache(project, section):
    """Reuse compatible terrain downloads from an existing bounded section, read-only."""
    project,section=Path(project),Path(section)
    with lock(project):
        load(project)
        plan=tiled.load_project(section)
        count=0
        for tile in plan['tiles']:
            directory=section/tile['id']
            manifest=verified_sources(directory/'sources',directory/'preparation.json','terrain')
            preparation=document(directory/'preparation.json')
            key=tiled.sha(tiled.encoded(cache_request(preparation)))
            destination=project/'cache'/key
            if destination.exists():
                verified_sources(destination/'sources',destination/'preparation.json','terrain')
                continue
            staging=project/'cache'/('seed-'+uuid.uuid4().hex)
            staging.mkdir()
            seed=dict(kind='openride.scenery-preparation',schema_version=1,trainer_ready=False,
                      source_route_sha256=key,acquisition_bounds=preparation['acquisition_bounds'])
            tiled.write_new(staging/'preparation.json',tiled.encoded(seed))
            (staging/'sources').mkdir()
            for name in manifest['files']:
                shutil.copyfile(safe_file(directory/'sources',name),staging/'sources'/name)
            manifest=copy.deepcopy(manifest)
            manifest.update(preparation_sha256=digest_file(staging/'preparation.json'),source_route_sha256=key)
            tiled.write_new(staging/'sources'/'sources.json',tiled.encoded(manifest))
            verified_sources(staging/'sources',staging/'preparation.json','terrain')
            staging.rename(destination)
            count+=1
        return dict(seeded_tiles=count)


def dependencies(path, stage):
    plan = tiled.load_project(path)
    entries = []
    for tile in plan['tiles']:
        directory = Path(path) / tile['id']
        manifest = verified_sources(directory / 'sources', directory / 'preparation.json', stage)
        names = ('terrain.tif', 'usgs-export.json') if stage == 'terrain' else ('map.osm',)
        entries.append(dict(tile=tile['id'], files={n: manifest['files'][n] for n in names},
                            attribution=manifest.get('attribution'), requests=manifest['requests']))
    return tiled.sha(tiled.encoded(entries))


def build_identity(path, builder):
    return dict(terrain=dependencies(path, 'terrain'), builder=digest_file(builder),
                tiled=digest_file(Path(__file__).with_name('tiled-scenery.py')),
                chunks=digest_file(Path(__file__).with_name('chunk-scenery.py')))


def check_receipt(path, region, stage, builder=None):
    receipt = document(Path(path) / (stage + '.done.json'))
    if receipt['plan_sha256'] != region['plan_sha256']:
        raise ValueError('Stage receipt belongs to another plan')
    if stage == 'bake':
        if builder is not None and receipt['inputs'] != build_identity(path, builder):
            raise ValueError('Bake dependencies changed; create a new job to retain the previous artifact')
        if receipt['inputs']['terrain'] != dependencies(path, 'terrain'):
            raise ValueError('Baked terrain dependencies changed')
        verify_inventory(Path(path) / 'preview.orpack', receipt['files'])
    elif receipt['sources_sha256'] != dependencies(path, stage):
        raise ValueError('Acquisition receipt no longer matches source data')
    return receipt


def validate_package(package, plan):
    manifest=document(package/'manifest.json')
    if manifest.get('kind')!='openride.stream-preview' or manifest.get('schema_version')!=1:
        raise ValueError('Incomplete regional package')
    entries=[manifest['route'],manifest['terrain_provenance']]+manifest['chunks']
    verify_inventory(package,{e['path']:dict(bytes=e['bytes'],sha256=e['sha256']) for e in entries})
    route=document(package/manifest['route']['path'])
    provenance=document(package/manifest['terrain_provenance']['path'])
    if route['points']!=plan['preparation']['route_profile']['points'] or provenance['review_revision']!=plan['preparation']['review_revision']:
        raise ValueError('Published regional package does not match the planned profile')
    return inventory(package)


def run(project, stage, builder=None, accept_steep=False, max_regions=None, fetch=None):
    project = Path(project)
    if stage not in ('terrain', 'osm', 'bake'):
        raise ValueError('Invalid corridor stage')
    if stage == 'bake' and not builder:
        raise ValueError('Baking requires a native builder')
    if max_regions is not None and max_regions < 1:
        raise ValueError('max-regions must be positive')
    with lock(project):
        job = load(project)
        completed = 0
        for region in job['regions']:
            path = project / region['id']
            if region.get('blocked'):
                print(f"Blocked {region['id']}: {region['blocked']}",flush=True)
                continue
            try:
                if (path / (stage + '.done.json')).exists():
                    check_receipt(path, region, stage, builder)
                    print(f"Verified {region['id']} {stage}; reuse", flush=True)
                    continue
                if (project / 'STOP').exists() or (max_regions is not None and completed >= max_regions):
                    return dict(stopped=True, completed_this_run=completed)
                plan = tiled.load_project(path)
                atomic(path / 'status.json', dict(stage=stage, state='running'))
                if stage == 'bake':
                    if plan['steep_segments'] and not accept_steep:
                        raise ValueError('Review steep grades in plan.json; --accept-steep required for this region')
                    inputs = build_identity(path, builder)
                    package = path / 'preview.orpack'
                    intent=dict(plan_sha256=region['plan_sha256'],inputs=inputs,
                                steep_accepted=bool(plan['steep_segments'] and accept_steep))
                    intent_path=path/'bake.intent.json'
                    # Recover a publication/receipt interruption only after checking its inputs
                    # and every published payload; never replace the previous complete package.
                    if package.exists():
                        if not intent_path.exists() or document(intent_path)!=intent:
                            raise ValueError('Unreceipted package has conflicting inputs; previous package retained')
                    else:
                        atomic(intent_path,intent)
                        tiled.build(path, package, builder, accept_steep)
                    atomic(path / 'bake.done.json', dict(intent,files=validate_package(package,plan)))
                else:
                    for index, tile in enumerate(plan['tiles']):
                        if (project / 'STOP').exists():
                            atomic(path / 'status.json', dict(stage=stage, state='stopped', tile=tile['id']))
                            return dict(stopped=True, completed_this_run=completed)
                        print(f"{region['id']} {stage} tile {index+1}/{len(plan['tiles'])}", flush=True)
                        atomic(path / 'status.json', dict(stage=stage, state='running', tile=tile['id']))
                        acquire_tile(project, path / tile['id'], stage, fetch)
                    atomic(path / (stage + '.done.json'), dict(plan_sha256=region['plan_sha256'], sources_sha256=dependencies(path, stage)))
                atomic(path / 'status.json', dict(stage=stage, state='complete'))
                completed += 1
            except (Exception, KeyboardInterrupt) as error:
                atomic(path / 'status.json', dict(stage=stage, state='failed', error=str(error) or 'Interrupted'))
                raise
        return dict(stopped=False, completed_this_run=completed, blocked_regions=sum(bool(r.get('blocked')) for r in job['regions']))


def status(project):
    job = load(project)
    rows = []
    for region in job['regions']:
        path = Path(project) / region['id']
        row = dict(region)
        row['last_operation'] = document(path / 'status.json') if (path / 'status.json').exists() else None
        for stage in ('terrain', 'osm', 'bake'):
            if region.get('blocked'):
                row[stage]='blocked'
            elif not (path / (stage + '.done.json')).exists():
                row[stage] = 'pending'
            else:
                try:
                    check_receipt(path, region, stage)
                    row[stage] = 'verified'
                except (ValueError, KeyError, OSError) as error:
                    row[stage] = 'invalid: ' + str(error)
        rows.append(row)
    return dict(length_m=job['length_m'], review_revision=job['review_revision'], regions=rows, output=job['output'])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='action', required=True)
    create_parser = sub.add_parser('plan')
    create_parser.add_argument('review')
    create_parser.add_argument('destination')
    create_parser.add_argument('--builder', required=True)
    create_parser.add_argument('--region-m', type=float, default=8000)
    run_parser = sub.add_parser('run')
    run_parser.add_argument('project')
    run_parser.add_argument('--stage', choices=('terrain', 'osm', 'bake'), required=True)
    run_parser.add_argument('--builder')
    run_parser.add_argument('--accept-steep', action='store_true')
    run_parser.add_argument('--max-regions', type=int)
    status_parser = sub.add_parser('status')
    status_parser.add_argument('project')
    seed_parser=sub.add_parser('seed')
    seed_parser.add_argument('project')
    seed_parser.add_argument('section')
    args = parser.parse_args()
    if args.action == 'plan':
        result = create(args.review, args.destination, args.builder, args.region_m)
    elif args.action == 'run':
        result = run(args.project, args.stage, args.builder, args.accept_steep, args.max_regions)
    elif args.action == 'seed':
        result=seed_cache(args.project,args.section)
    else:
        result = status(args.project)
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    try:
        main()
    except (Exception, KeyboardInterrupt) as error:
        print(str(error) or 'Interrupted; completed work retained', file=sys.stderr)
        sys.exit(1)
