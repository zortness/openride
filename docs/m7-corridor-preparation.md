# M7 resumable corridor preparation

This initial S04 implementation plans an entire reviewed recording and runs
terrain acquisition, OSM acquisition and regional terrain/road baking as separate
resumable stages. It produces independently inspectable regional packages.
It does **not** yet assemble one full-route rider package or reconcile terrain
meshes between regional coordinate frames. S04 remains open, including the Death
Ride terrain/OSM acceptance gate.

## Commands

Configure the [local tool environment](../build/README.md), build the native tool,
then choose a new private project directory:

```powershell
. ./build/Use-Environment.ps1
python tools/corridor-scenery.py plan local/my-review local/my-corridor --builder target/release/content-builder.exe
python tools/corridor-scenery.py status local/my-corridor
python tools/corridor-scenery.py run local/my-corridor --stage terrain --max-regions 1
python tools/corridor-scenery.py run local/my-corridor --stage osm --max-regions 1
python tools/corridor-scenery.py run local/my-corridor --stage bake --builder target/release/content-builder.exe --max-regions 1
```

Repeat a stage command to continue. Completed regions are checksum-verified and
reused. `--max-regions` counts newly completed regions, so repeated commands make
forward progress. Omit it to process all available regions for that stage.
Terrain contacts USGS; OSM contacts Overpass. Baking is offline and currently
uses terrain and the reviewed route only; acquired OSM is retained for subsequent
road/scenery processing. It is not silently presented as integrated OSM scenery.

To reuse terrain from a previous compatible section before acquisition:

```powershell
python tools/corridor-scenery.py seed local/my-corridor local/my-tiled-section
```

Only exact matching geographic requests are reused, after validating source-file
checksums. Original cache directories are read-only. Each copied manifest binds
the verified bytes to its new preparation. Repeated route occurrences keep their
own stations even when a geographic cache entry can be reused.

## Review and recovery

`corridor.json` pins the review revision, original source hashes, profile snapshot
and regional plan hashes. It lists regional tile/point counts, steep-grade findings
and blocked regions. Source selections, accepted repairs, and original samples
remain unchanged. If the review changes, create a new job; the old assets remain
available for inspection and cache seeding.

Grades above 15% require explicit `--accept-steep` for baking after reviewing the
regional `plan.json`. Grades above 100% block that region and appear separately
in the full-corridor findings. No automatic elevation smoothing or flattening is
performed. A profile with unresolved continuity/missing-data findings cannot be
planned. Other valid regions can proceed when a grade or geometry finding blocks
an individual region.

Each region has atomic status and completion receipts. `status` verifies the
actual files, not just a completed flag. Failed acquisition retains valid tile
caches; rerunning the same command retries the failed work. Baking retains complete
regions and retries the current bounded region. Its receipt pins the builder
binary, source dependencies and relevant scripts. If a process stops after
publishing a package but before writing its receipt, retry checks its saved intent,
profile and payload hashes before adopting it. Conflicting artifacts are preserved
and reported instead of overwritten.

Create an empty `STOP` file in the job directory for a cooperative stop between
tiles/regions. Remove it to resume. Ctrl+C also leaves completed work intact.
An exclusive `job.lock` prevents concurrent writers. After forced termination,
confirm the process has exited before removing a stale lock. Incomplete planning
directories and bake scratch directories are retained for diagnosis; they are not
published as complete artifacts. Keep all these directories under ignored `local/`.

## Bounds and remaining work

Default regions are 8 km, configurable from 250 m to 12 km. Planning subdivides
regions that exceed projection, tile or point budgets. Existing per-region limits
remain: 40 source tiles, 10,000 profile points and 100,000 terrain vertices. Jobs
are bounded to 512 regions and 2,000 km. The full source profile and small regional
index remain resident; baking processes one region at a time. This is not a
measured minimum-hardware or memory-budget acceptance result.

Regional boundaries preserve canonical geography, elevation and source distance.
Local terrain lattices and presentation earthwork are still independent across
regions. Do not concatenate these packages or treat regional traversal as evidence
of a continuous full-route world. Remaining work includes cross-region terrain
ownership/seams, unified route/profile streaming and package assembly, OSM scenery
processing, and full-route offline/performance verification.

## Current OC result

On 2026-10-12 the accepted recording planned into 20 regions over 153.126048 km.
Four regions are blocked by seven additional raw-grade segments above 100%; these
are distinct from the four accepted continuity repairs. Their private sample and
height details are in the job's `grade_findings`. The first two regions (0–16 km)
have verified terrain and separately baked packages. The first reused 16 existing
terrain tiles; the second acquired 17 tiles. The original 8 km package remains
unchanged. OSM acquisition has synthetic coverage but has not been run for this
private job. The remaining regions are pending or explicitly blocked. Exported
rider traversal of the 8–16 km region passed (`STREAM_TRAVERSAL_OK`): 84 chunks,
867 route points, peak nine live / three cached chunks. This validates that regional
artifact, not cross-region terrain seams or a whole-route package.

Eight synthetic corridor tests cover full-route station boundaries, blocked
grades, independent stages, interruption/resume, cache reuse, corruption,
concurrent-writer rejection, source revision changes, bake dependency changes and
recovery after interrupted publication. The full Python suite passed 81 tests;
locked/offline Rust workspace tests passed. These results do not close S04.
