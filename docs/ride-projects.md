# Ride Projects and the native CLI

The default editor now follows [road-first authoring](road-first-authoring.md):
**Recording → Roads → Preview → Scenery & share**. Use the recording to infer
connected roads, review named alternatives, then generate elevations from terrain.
Original recordings and legacy repairs remain intact. The detailed trace-based
operations below describe the compatibility path; recorded-height requirements
apply to that path, not the new road-first course. New decisions use `roads.json`.

S08 adds **Create → New Ride Project / Open Ride Project**. Enter a ride name and
choose a new or empty folder to create it. Open selects the folder, not a manifest
file. The overview shows the saved name, location and empty import state. **Save
name** persists a rename. The project can be moved and reopened without rewriting
paths. The app remembers the last successfully opened Ride Project.

Choose **Import complete ride** and select one or more GPX/TCX variants of the
same recording. The native importer preserves complete sources, compares sample
counts, recorded/geometry distance, missing values, gaps and diagnostics, and
provides map/elevation plots with sample inspection. Choose geometry and elevation
independently, then **Save source choices**. **Show import log** is off by default.
Source choices and snapshots survive moving/reopening the project.

Import does not close gaps, remove loops, align different source profiles or
blend elevations. Repairs are reviewed in the next panel. Different recordings and duplicate inputs
are rejected. Import accepts 1–8 variants, 2–100,000 samples per source, 64 MiB per
file and 128 MiB combined. A project currently accepts one intake batch; choose
all variants together. Later source replacement/addition is not implemented.

**Preview and repair** offers bounded gap closures and tiny-loop removals with
before/after previews. Accept, keep the original or undo a decision; `repairs.json`
is saved automatically. Heights are never flattened by this stage. Larger gaps
and uncertain cases remain unresolved for course editing.

**Maps for the whole ride** plans 2 km corridor tiles with about 8 m terrain
samples. Acquire/resume topography, streets and optional imagery independently.
Receipts bind the completed files to their checksums; retries repair missing or
corrupt tiles. Cancel preserves completed tiles. The current bounds are 200 km,
99 km from the projection origin, 1024 source tiles and latitudes within ±80°.
Topography uses USGS 3DEP, streets use OpenStreetMap, and optional NAIP imagery
uses 1024-pixel tiles at roughly 2 m ground resolution. Metadata/attribution stay
with each tile. Provider coverage is not guaranteed. HTTP rate limits use bounded
waits and remain retryable. Select a coverage region to inspect one cached image;
the map does not load the complete corridor's textures at once.

**Build course** attempts connected road/path matching and retains ambiguous or
unmatched occurrences in its issue list. Select an occurrence to focus its map.
Choose a nearby road, keep the trace, click/edit a point, explicitly join a gap, or
undo. `corrections.json` saves edits against source occurrences and map revisions.
Changed inputs invalidate derived results and report edit conflicts; **Archive
stale edits** preserves the old book under `jobs/` before clearing active edits.
Point edits are limited to 5 km from their source occurrence. Recorded heights
remain unchanged, and cross-source elevation alignment is not implemented here.
Renaming a project preserves its course and edits. Geometry/source or map changes
can require a rebuild; changing the display name does not invalidate road choices.

**Scenery** acquires buildings, vegetation and water independently from
OpenStreetMap. Filter and select a feature, or draw a new polygon on the map.
Select a vertex and click to move it; select the outer boundary or a hole to
edit that ring. Buildings expose height; vegetation exposes density and trees;
water supports an explicit surface elevation. Exclusion polygons clear vegetation.
Save, hide, undo and reset persist in the project's `scenery.json`. Reset removes
an override; hide suppresses a source feature. Source changes produce conflicts
that require an explicit save or reset before rebuilding.

Building height defaults to 6 m when tags provide no height or level count, with
provenance. Vegetation placement is deterministic and keeps clear of the course,
buildings, water and exclusions. Water retains polygon holes and uses one flat
surface per feature; absent an explicit elevation it uses the lowest available
boundary terrain sample. Review this approximation for long rivers and reservoirs.
Simple directed coastline crossings and islands are supported; complex crossings,
incomplete shorelines and invalid topology produce warnings instead of guessed
water. Nominal sea level is 0 m and is not a validated vertical-datum conversion.

**Build 3D package** compiles the reviewed course and acquired terrain into one
continuous native package. Resolve every course issue first. The current builder
requires recorded heights from the same source as geometry; missing heights and
cross-source alignment remain explicit errors. Grades above 15% require the
explicit steep-grade preview option after inspection. Builds are unvalidated
previews, not a declaration that the route is ready for trainer use.

The builder uses a shared 8 m terrain lattice and 256 m runtime chunks, bounded
DEM caches, shared terrain/road normals and content-hashed chunk reuse. Terrain
presentation follows ordinary mapped roads locally while leaving recorded course
heights unchanged. Reviewed manual/trace segments lower terrain for clearance
without raising embankments. Bridges, tunnels and stacked crossings remain
excluded from terrain conformance. Immutable versions live under
`build/course/<build-key>/`; `build/course/current.json` selects the last complete
version. Interrupted or failed builds retain that version and completed staging
assets for retry. Geometry is currently recomputed before unchanged chunks are
reused; selective local geometry regeneration is still pending.

**Open built preview** opens that package directly. **Export ride ZIP** writes the
versioned archive under `exports/` without asking for internal filenames. Optional
imagery, buildings, vegetation and water are compiled into the same package.
The build panel reports imagery coverage, warnings and whether the saved build
matches current inputs. An edited point outside the acquired corridor can still
require additional coverage before building.

Imagery uses a shared 2 m pixel lattice with a one-pixel gutter in each 130 x 130
chunk texture. Missing imagery uses terrain shading. The baker retains at most
eight decoded source images; the runtime loads nearby textures with chunks and
releases them on unload. A coarse 128 m terrain mesh supplies distant coverage.
Scenery is clipped to chunk boundaries; vegetation is capped at 1,024 candidates
per chunk and individual meshes at 30,000 vertices. Source polygons have bounded
vertex counts; rejected or unsupported features appear as warnings. Water lowers
the terrain underneath it without altering recorded course elevations.

The route library accepts existing unpacked package folders with `manifest.json`
and ordinary `.zip` files. **Export selected ZIP** packages only referenced runtime
assets; source recordings and logs are excluded. **Open ZIP** validates checksums
and installs into a managed offline cache. ZIPs may contain the files directly
or one enclosing package folder. Unsafe paths, links, duplicate entries, extra
unreferenced files and oversized expansion are rejected. Existing `.orpack`
folders remain supported; their suffix is not required.

The **Legacy section builder** retains the previous Python-based workflow and its
saved projects, including optional section exports. New Ride Project import always
retains the whole recording; it does not require a crop or 5 km selection. The
legacy builder does not interpret new Ride Projects; migration is S22.

The release includes `authoring/openride-author.exe`. Contributor builds also put
it in `target/release/` and `apps/rider/bin/`. It requires no Python, PowerShell,
Godot or trainer connection to perform project operations:

```powershell
./dist/authoring/openride-author.exe project create "local/My Ride" --name "My Ride"
./dist/authoring/openride-author.exe project open "local/My Ride"
./dist/authoring/openride-author.exe project status "local/My Ride"
./dist/authoring/openride-author.exe project rename "local/My Ride" --name "Coast Ride" --expected-revision SHA256_FROM_OPEN
./dist/authoring/openride-author.exe project import "local/My Ride" --expected-revision REVISION_FROM_OPEN --source "import/ride.gpx" --source "import/ride.tcx"
./dist/authoring/openride-author.exe project select-sources "local/My Ride" --expected-revision REVISION_FROM_IMPORT --geometry GEOMETRY_SOURCE_SHA256 --elevation ELEVATION_SOURCE_SHA256
./dist/authoring/openride-author.exe project repair "local/My Ride" --expected-revision REVISION --repair PROPOSAL_ID --status accepted
./dist/authoring/openride-author.exe project plan-maps "local/My Ride" --expected-revision REVISION
./dist/authoring/openride-author.exe project acquire "local/My Ride" --expected-revision REVISION --layer terrain
./dist/authoring/openride-author.exe project acquire "local/My Ride" --expected-revision REVISION --layer streets
./dist/authoring/openride-author.exe project acquire "local/My Ride" --expected-revision REVISION --layer imagery --max-tiles 1
./dist/authoring/openride-author.exe project acquire "local/My Ride" --expected-revision REVISION --layer buildings
./dist/authoring/openride-author.exe project acquire "local/My Ride" --expected-revision REVISION --layer vegetation
./dist/authoring/openride-author.exe project acquire "local/My Ride" --expected-revision REVISION --layer water
./dist/authoring/openride-author.exe project edit-scenery "local/My Ride" --expected-revision REVISION --id FEATURE_ID --mode hide
./dist/authoring/openride-author.exe project edit-scenery "local/My Ride" --expected-revision REVISION --id FEATURE_ID --mode undo
./dist/authoring/openride-author.exe project build-course "local/My Ride" --expected-revision REVISION
./dist/authoring/openride-author.exe project edit-course "local/My Ride" --expected-revision REVISION --sample 25 --mode trace
./dist/authoring/openride-author.exe project edit-course "local/My Ride" --expected-revision REVISION --sample 25 --mode undo
./dist/authoring/openride-author.exe project build-package "local/My Ride" --expected-revision REVISION
./dist/authoring/openride-author.exe project export-package "local/My Ride" --expected-revision REVISION
./dist/authoring/openride-author.exe package pack "export/My Unpacked Ride" "export/My Ride.zip"
./dist/authoring/openride-author.exe package validate "export/My Ride.zip"
./dist/authoring/openride-author.exe package install "export/My Ride.zip" "local/package-cache"
```

Read the new revision from each successful result before the next mutation.
Scenery saves accept `--mode save --feature JSON`. The desktop editor writes its
draft automatically to `jobs/scenery-edit.json` and passes `--feature-request
SHA256`; the CLI verifies that exact draft before saving. No internal-file picker
is needed, and concurrent draft replacement cannot silently change the request.
`package` commands return a `package` object instead of a project `snapshot`.
ZIP expansion is limited to 2 GiB overall and 16 MiB per asset; ordinary route/chunk
payloads retain the runtime's 2 MiB limit. ZIP export refuses an existing filename.

`project.json` is `kind: openride.ride-project`, `schema_version: 1`. It contains
stable project identity, name, stage (`empty` or `imported`) and the fixed relative paths
listed in the [approved task breakdown](m7-authoring-repair-tasks.md). Stage files
and directories are created lazily by their owning stages. Source data is never
embedded in the project manifest. Unknown schemas and malformed conventions are
rejected with actionable errors, not silently upgraded.

## CLI and UI contract

Commands emit one JSON object per stdout line with `protocol_version: 1`.
`event: progress` includes the action and phase; the terminal `event: result`
contains `status: complete` with `snapshot: {project, revision}`, or `status:
failed/cancelled` with `error: {code, message}`. Revisions hash the exact saved
manifest bytes for empty projects; imported project revisions bind both the
manifest and saved review. Imported snapshots also return `review` containing
source metadata, activities, gaps and source choices. Exit codes are 0 for success,
2 for failure (including usage), and
3 for cancellation. `--help` is human-readable and exits 0.

Project commands accept `--cancel-file PATH`. Existence requests cancellation; writes
check before publication. Cancellation after publication cannot roll back a saved
project and may return success. Godot runs the same CLI in a worker thread, keeps
navigation responsive, shows a cancel action and checks both exit status and the
terminal result. No shell command string is constructed from user paths.

Writes acquire `project.lock`, validate the expected revision when updating,
stage and sync `project.json.pending`, then atomically replace `project.json`.
Creation refuses nonempty folders. Failed or cancelled writes leave the previous
manifest intact. A process killed during a write may leave a lock/pending file:
opening still reads the last complete manifest. Before manually removing a stale
lock or pending file, confirm no authoring writer is active and retain the pending
file for inspection. Automatic crash-lock recovery is not provided in S08.

Source snapshots use `sources/<sha256>.gpx` or `.tcx`; `sources/index.json` records
their identities. `review.json` owns source choices and `logs/import.log` stores
diagnostics. The imported project stage is published last, after all source hashes
validate. Failed/cancelled intake can leave private staging and verified snapshots
without exposing a partial imported state; retry preserves original snapshots.
An interrupted atomic file write can require inspection of its `.pending` file
before retrying. Source-choice saves also acquire `review.lock` to coordinate
with native review tooling and never invoke automatic elevation repairs.

The lock coordinates cooperating writers; unrelated external file editors do not
participate. Atomic replacement does not establish power-loss durability for all
filesystems. Project folders contain private ride data as later stages populate
them and should remain outside tracked source.

## Verification

`cargo test -p openride-author --locked --offline` exercises moved projects,
revision conflicts, interrupted writes, cancellation, unsupported versions,
unsafe references, preservation of nonempty folders and the real CLI protocol.
Import coverage includes paired recordings beyond 5 km, missing elevations,
retained gaps, duplicate/unrelated inputs, source corruption, moved projects,
source-choice revisions and no implicit elevation repair.
The exported app accepts `--verify-ride-project --project-test-root=NEW_ABSOLUTE_DIR`
after `--`; it uses explicit failure exits to check native creation, moving,
reopening in a fresh screen, rename persistence and stale-write rejection.
Use isolated APPDATA and a hidden, bounded process as required by AGENTS.md.
Repeat `--import-source=ABSOLUTE_GPX_OR_TCX` to extend this check to full import,
source choice persistence and the collapsed log toggle. The supplied synthetic
pair is `tools/fixtures/ride-project.gpx` and `.tcx`.

The native assembly regression checks sloped terrain and imagery seams, scenery
meshes, ZIP contents, cache reuse after a local edit, cancellation and retention
of the previous package. Scenery tests cover polygon holes, deterministic placement,
course clearance, coastline direction, persistent overrides and source conflicts.
The exported project check supports `--verify-project-scenery` alongside
`--build-project=ABSOLUTE_PROJECT` to exercise save/hide/undo/reset through the CLI.
Run the larger
synthetic 150 km counterpart explicitly with:

```powershell
cargo test -p openride-author --release --locked --offline --test assembly builds_long_native_corridor -- --ignored
```

This test downloads nothing and does not exercise trainer hardware. To retain its
disposable Ride Project for UI inspection, set `OPENRIDE_SYNTHETIC_PROJECT` to a
new directory under `local/` before running it. Headless `--verify-stream` checks
visit every 25 m without display frame pacing; use rendered benchmarks separately
for frame-time/GPU acceptance.
