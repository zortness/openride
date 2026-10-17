# M7 progress

## Home and shell UX — 2026-10-15

Home now contains only the app name, source/build version and UTC build date over a
looping fictional coastal ride. The camera follows a curving road past low-rise
buildings and trees, with a beach and sea to one side. The scene uses generated
geometry, no personal route or network data, and stops rendering while Home is
hidden. Release builds generate and pack the version/date metadata. The executable
window title is `OpenRide`.

Back, Home, Ride, Create, Activities, Settings and a compact window-mode icon share
the top row. The active section has a persistent selected outline after one tab
press, including nested screens; the redundant per-screen caption is removed. On
Home, two Back presses open the exact Yes/No exit confirmation. A fresh exported
Windows build passed the UX regression covering replay motion, title, icon size,
single-press tab selection, two-press exit dialog, Yes-button exit and rendering suspension. A
rendered Home capture is at `local/home-preview.png` for review.
The owner noticed lane-line flicker during motion. All markings had been coplanar
with the road mesh; they now sit 6 cm above it, with wider lines and 4× MSAA in
the Home viewport. The rebuilt exported app passed the UX check and a fresh
rendered Home capture with no script errors. Motion appearance still benefits from
owner review on the display used for riding.

## Automatic scenery follow-up — 2026-10-15

The owner expected buildings and vegetation to arrive automatically and found the
polygon editor misleading. The current native pipeline downloads OSM building
footprints and mapped woodland, scrub, heath and forest areas, then plants
deterministic trees and shrubs in those areas. Legacy vegetation also used mapped
land cover; satellite imagery currently supplies ground color and does not classify
plants. Manual polygons are corrections for missing or inaccurate map features.

The real OC project had all 112 building source tiles cached but no scenery catalog:
catalog assembly hit the 20,000-feature limit because it included off-route
buildings. Catalog generation now retains features near the saved ride corridor and
preserves edited source identities. It recovered 19,550 building footprints in a
10.1 MiB catalog; the bounded feature limit was raised to 30,000 while the 16 MiB
read limit stays in place. The saved course rebuilt with building meshes in 855 of
1,413 chunks (450,258 building triangles); a rendered snapshot near 1 km shows
them. The 247 road choices and accepted connection are unchanged. The current
project build is `3af35d0f8c03e4e927723277c7761e52224e761a3a3058375babea828e1bf294`.
The ZIP already in `dist/content/` is the earlier corrected terrain package; it
does not include these buildings.

Scenery & share now offers one primary resumable action for mapped buildings and
vegetation, reports layer tile coverage and feature counts, and places polygon
editing and individual-layer downloads under Advanced. The exported Windows app
and its synthetic scenery workflow pass. One live vegetation tile was downloaded
for the real project, but it contained no route-near vegetation: vegetation
coverage is 1/112 and the current build has zero plant instances. Full-route
vegetation acquisition, rebuild/export and visual review remain open. Satellite
vegetation classification is not implemented.

## Road-first refactor — 2026-10-13

Long-route payload follow-up: canonical route JSON now permits 8 MiB across native
assembly, package/ZIP validation, library and scenery loading, simulation/replay,
the device worker and saved activity export. Scenery payloads retain their 2 MiB
limits, and profiles retain the 10,000-point limit. Route serialization and size
validation run before imagery preparation and chunk generation. The earlier
2 MiB synthetic-route validation below describes the previous budget.

Validation: workspace Rust and all 90 Python tests passed, including the 8 MiB
route boundary, unchanged scenery/point caps, ZIP round-trip and saved activity
export above 2 MiB. The real OC preview built at 153.490 km with 9,285 points in
2,639,856 route bytes and 1,413 chunks; saved road choices were unchanged. The
exported app completed offline traversal with at most nine live and three cached
chunks, and its software power demonstration passed. Grades above 15% were retained
explicitly for this unvalidated preview; this is not hardware or GPU acceptance.

2026-10-15 owner check: the real road-first ride exported as a ZIP, was placed in
`dist/content/`, and opened for preview through the main Ride menu. This confirms
the user-facing export, local content discovery and preview entry path for this
package. Visual terrain, road and grade review is still pending.

Long-route inspection follow-up: the owner saw no issue in a quick preview, so a
direct-distance screenshot option (`--snapshot-distance-m` with `--snapshot`) was
added for targeted, bounded rendered checks. Twelve stations spanning the route
found buried camera/road geometry near 107.6 km, at the only two profile grades
above 15%. Terrain conformance had treated heights anywhere within the 30 m road
corridor as stacked surfaces; the steep road's own nearby stations triggered that
exclusion. Stacked-surface detection now compares only similarly close surfaces.
The saved choices, route geometry and terrain-derived grade profile were unchanged.
The rebuilt package regenerated 26 chunks; a compiled-mesh scan found zero camera
intrusions across 9,285 route points, versus one at 107,612.8 m in the prior
package. Rendered images at 107.60–107.64 km show the road and camera clear, and
the new package completed a 153.490 km offline traversal with nine live and three
cached chunks at peak. The corrected ZIP is installed in `dist/content/`; the
previous ZIP is retained under `local/content-backups/`. The steep grades remain
unvalidated for trainer use. Sparse snapshots and mesh clearance do not establish
complete visual, bridge, GPU or hardware acceptance.

Preview readiness follow-up: Preview now explains road/scenery blockers and shows
remaining topography coverage. With road review complete, its build action resumes
terrain acquisition and queues the build using the existing cancellation/error
handling. Recorded-connection controls are collapsed under Advanced on Roads.

2026-10-14: short disconnected road transitions now offer an explicit recorded
connection with map preview and undo in the road-first editor. `roads.json` stores
the occurrence endpoints separately from road choices; inference revalidates the
continuous recording and 250 m limit. Changed endpoints fail without publishing
a replacement. The accepted geometry uses terrain heights and manual terrain
clearance rather than inheriting bridge/tunnel tags from a neighboring way.
Synthetic regression coverage includes preservation of choices, save/reopen,
reinference, terrain package build, undo, stale writers, and rejecting long or
missing-recording links.

Road review usability follow-up: saved, resolved road choices remain in the review
list with an Accepted checkmark. Automatic matches remain distinct and need no
action; Next to review skips accepted choices. Numbered road labels match dropdown
options (including unnamed ways), with amber highlighting for the selected option
and wider/closer context controls. These indicators use saved course state, so they
survive reopening without introducing another project file.

Road-choice propagation: the explicit stretch action applies a choice to nearby
ambiguous visits along an unbranched connected road chain, bounded to 5 km each
way. Gaps, turnarounds, other saved choices and departures stop expansion. The
native CLI uses `choose-road --scope nearby`; grouped undo metadata lives in
`roads.json`, preserving later individual edits. Regression coverage includes
same-name disconnected roads, named branches, breaks, repeated visits, stale
writers, persistence and grouped undo.

The owner approved recreating road rides from an inferred road sequence rather
than treating recorded geometry/elevation as the course. The native default and
four-stage editor now implement [that approach](road-first-authoring.md).

- New `infer-roads`, `choose-road` and `reset-roads` CLI operations save a separate
  `roads.json`, preserve legacy artifacts and enforce source/map revision checks.
  Forward/backward scoring uses neighboring observations, connected OSM topology,
  road eligibility and one-way direction. Intermediate way identities and repeated
  occurrences remain explicit. Uncertain stretches are grouped for review.
- The editor now has Recording, Roads, Preview and Scenery & share stages, with
  persistent navigation and independently scrolling pages. Named road alternatives
  appear beside the map; recording diagnostics and legacy tools are collapsed.
  Download maps & infer roads runs planning, resumable downloads and inference in
  sequence; cancellation stops subsequent stages. Restart archives old choices.
- Road profiles sample terrain at <=30 m spacing with a local smoothing window;
  recorded elevation is no longer required or authoritative. Bridge/layer profiles
  interpolate between terrain-supported endpoints. Tunnel builds explicitly fail
  until interior geometry exists. Existing trace-based builds retain their prior
  semantics. New geometry still uses the shared terrain/chunk/imagery/scenery system.

Validation: locked/offline workspace passed; the final native authoring suite has
34 passing normal tests, including sequence disambiguation, grouped alternatives,
road eligibility, missing recorded elevations, terrain-derived profiles, archived
choices, stale writers and preservation of legacy files. All 90 Python compatibility
tests passed. The exported editor passed four-stage navigation, cached map
preparation/inference, named choice save/reopen/undo, scenery editing and package
build/export/open. Rendered editor and preview screenshots were inspected.

Cached real OC inference took about 3.3 s: 153.758 km, 3,224 retained observations,
147 unresolved stretches (141 road ambiguities, five recording gaps, one disconnected
transition). These are still proposals, not accepted choices. This grouped count
is not directly comparable to the old per-observation issue count. The synthetic
150.113 km road-first course has zero unresolved stretches and builds 1,176 chunks;
latest repeat build took 13.6 s. A 20 m profile initially exceeded the 2 MiB runtime
route budget; the tested 30 m spacing fits without changing fine road-mesh sampling.
The final exported offline traversal passed all 150.113 km with 5,401 profile
points, six live chunks and two cached chunks at peak. Imagery loaded/unloaded
correctly over its synthetic start coverage. Headless traversal is not a GPU
performance acceptance benchmark.

Remaining: real OC road-choice review and missing connection, representative bridge
heights, tunnel interior geometry, turn restrictions, sparse recording transitions
beyond 500 m and existing scenery/performance/hardware acceptance. Legacy migration
must preserve old elevation decisions as reference, not apply them silently to a
new road-derived profile. This refactor does not close all M7 acceptance gates.

Visual validation follow-up: the owner found buried road/camera geometry near
1.22 km remaining in the short synthetic fixture. Manual/trace edits had been
classified with elevated structures, suppressing terrain clearance. They now
allow lowering terrain without raising an embankment; tagged structures and
stacked crossings retain their exclusions. A compiled-terrain regression covers
the edited segment and return. All 30 native authoring tests passed, and a fresh
rendered traversal/screenshot at 1.22 km remaining shows the road and camera clear
of terrain. The bundled authoring binary and short fixture were rebuilt.
The earlier description of the midpoint screenshot as a water crossing was
incorrect: the synthetic lake is beside the road. Water currently uses a simple
colored material, without animated waves or a dedicated water shader.

## 2026-10-13: S16-S20 native scenery continuation

This checkpoint supersedes the implementation status in the earlier continuation
below. Native packages now include tiled imagery, coarse distant terrain,
buildings, vegetation and water. The project editor acquires each scenery layer,
edits polygon boundaries/holes and properties, and saves/hides/undoes/resets
project-owned overrides through the bundled Rust CLI. Changed source fingerprints
produce conflicts; no correction-file selection or interpreter is required.

Imagery uses a shared 2 m lattice with matching gutters, an eight-source-image
baker cache and nearby runtime textures. Transparent/missing pixels retain terrain
shading. Immutable builds bind imagery receipts and scenery revisions; the UI
reports stale builds, coverage and warnings. ZIP validation includes the new
assets. Coarse terrain uses a 128 m lattice outside nearby detailed cells.

Buildings retain footprint holes, use tagged height/levels or a documented 6 m
default, and keep clear of the course. Deterministic vegetation respects density,
water/building/exclusion masks and a per-chunk candidate budget. Water preserves
holes and shared surface levels while lowering underlying terrain without changing
course elevations. Directed coastline assembly handles a simple crossing with
contained islands. Incomplete, branched or multiple-crossing coastlines are
reported rather than filled speculatively. Inferred water levels and nominal 0 m
sea level remain preview approximations requiring review.

Fresh validation after implementation:

- Locked/offline Rust workspace passed, including six scenery regressions for
  topology, chunk edges, placement/clearance, persistence, acquisition and conflicts.
- Explicit synthetic 150 km release assembly regression passed in 49.51 s,
  including terrain/imagery seams, native ZIP assets and rebuild/failure behavior.
- Release build/export passed. Exported rendered editor emitted
  `PROJECT_SCENERY_OK polygon_save hide undo reset native_cli` and
  `PROJECT_BUILD_PREVIEW_OK native_geometry runtime_loaded`.
- Exported headless traversal covered 150.113 km and 1,176 chunks with six live
  chunks/textures and two cached chunks at peak. Only the start has synthetic
  imagery: 26 image loads exercise unloads and the distant missing-image fallback.
  This is a bounded-loading check, not a GPU performance benchmark.
- Rendered 2.224 km traversal passed; the initial screenshot review missed buried
  terrain at the manual edit (see follow-up above). All 90 Python regressions passed after rerunning
  outside the sandbox's Windows temporary-directory restriction.
- Live acquisition of one OC tile per layer produced 1,101 buildings, nine
  vegetation areas and ten water features with no catalog warnings. This is a
  real-source ingestion smoke test, not complete real-route scenery acceptance.

Remaining gates: representative real buildings/coasts/bridges and water levels,
full-corridor real imagery/scenery appearance and GPU budgets, complex coastline
topology, selective geometry regeneration (unchanged assets are reused but geometry
is recomputed), S22 legacy/elevation-blend migration and S23 acceptance. Raw OC
course issues still require review; accepted legacy elevation blends are untouched.
Hardware/endurance gates remain deferred. Task checkboxes retain these acceptance
requirements rather than treating implementation as milestone closure.

## Earlier native authoring checkpoint

2026-10-13 native authoring continuation: S10/S11 project-owned repair proposals,
S12 corridor terrain, S13-S15 street graph/matching/editing, S16 imagery acquisition
and map inspection, S17 native terrain/road assembly, and S21 ZIP integration now
have implementations. This is partial milestone delivery; acceptance gates remain
open where listed below. Accepted legacy OC elevation blends are untouched.

Gap closure preserves heights and offers only bounded recording-pause joins;
tiny-loop proposals preserve larger loops, out-and-backs and later visits.
Decisions are persistent and undoable. Connected road/path matching keeps
ambiguities, disconnected crossings and missing coverage explicit. The editor
supports road choices, trace acceptance, edited endpoints, gap joins and undo,
with source-occurrence identities and revision conflicts. Rename preserves edits.

All 112 OC terrain tiles and all 112 street tiles downloaded and validated;
the merged graph has 45,013 street ways. Bounded street-only queries avoid the
timeouts observed with broad scenery requests; paced requests and Retry-After
handling resume rate-limited jobs. Full raw-route plans cover OC (153.097 km,
112 tiles), L'Etape (163.176 km, 62 tiles), and Death Ride (170.933 km, 64 tiles).
One real L'Etape NAIP tile passed native PNG validation and bounded map inspection.
Full-route 3D imagery streaming is still pending.

Native assembly builds continuous terrain/road packages with a shared 8 m lattice,
256 m chunks, DEM reconciliation, content-hashed chunk reuse and immutable build
versions. The UI builds, opens and exports to project-owned filenames. It refuses
unresolved course issues or missing/alignment-ambiguous heights; steep grades
require explicit preview acceptance. Builds remain unvalidated for trainer use.
ZIP pack/validate/install accepts ordinary unpacked folders and rejects unsafe or
unreferenced contents. Existing OC assets round-tripped through a 36.3 MB ZIP.

Validation: locked/offline Rust workspace passed, including 24 native authoring
tests; the additional explicit 150 km synthetic release test passed in 32.66 s
including acquisition fixtures, matching, three builds and validation. Its 1,176
chunks passed 42,283 shared-edge height/normal comparisons; repeat assembly reused
all chunks. A local edit reused distant assets; failure/cancellation retained the
previous build. Exported UI checks
passed repair/reopen/undo, maps, course edit/undo, native build/open and ZIP flows.
The 2.2 km native rendered traversal passed with six live chunks at peak. Full
Python regression passed (90 tests). These are software checks, not hardware proof.

Final exported headless traversals both passed: the native 150.113 km synthetic
package (1,176 chunks, peak six live/two cached) and ZIP-installed accepted
153.126 km OC package (1,431 chunks, peak nine live/three cached). Checks visited
every 25 m, validated camera poses, exercised distant seeks and proved load/unload
bounds. Headless verification omits display pacing and is not a rendered frame-rate
benchmark. Exported UX regression also passed. All six original activity hashes
and the accepted OC review hash were rechecked unchanged.

The raw OC course has 830 unresolved occurrences after conservative matching;
they have not been silently accepted. Its earlier accepted 153.126 km preview is
preserved. Remaining work includes complete real-course review/parity, streamed
3D imagery, S18-S20 editable scenery, S22 legacy migration/cutover and S23 complete
integration/performance acceptance. Rebuilds reuse unchanged assets but currently
recompute geometry; selective local regeneration remains open. Manual edits beyond
the acquired corridor need coverage handling. Hardware/endurance gates remain open.

The user authorized continuing through the approved slices without routine prompts
and using disposable projects from multiple long recordings in `import/`.

2026-10-13 S09: Ride Projects now import 1–8 complete GPX/TCX variants through
`openride-author project import`. The Create screen compares recordings, displays
full map/elevation plots and sample details, and saves independent geometry and
elevation choices through `select-sources`. The import log is togglable and hidden
by default. Sources use immutable hash-named files under `sources/`; review and
log filenames are automatic. Revisions bind project and review state. Duplicate
or unrelated recordings fail without publishing an imported stage. No automatic
elevation repair, gap closure or road matching runs in this workflow.

Validation: seven native project tests and the locked/offline Rust workspace
passed; all 90 Python tests passed. Release build/export, rendered exported
multi-file import, log toggle, moved-project reopen, persisted source choices and
the existing UX regression passed. CLI intake of both supplied variants of OC,
Death Ride and L'Étape passed with original source hashes unchanged. These private
test projects retain raw source gaps; they do not replace the accepted OC review.

S10 project gap repair is next. Select all source variants in one import batch;
adding/replacing sources after intake is not yet supported. Optional section
exports remain in the legacy workflow. M7 scenery, performance and supervised
hardware acceptance gates remain open.

2026-10-13: the [S08–S23 authoring breakdown](m7-authoring-repair-tasks.md) is approved.
S08 is implemented: Create now opens a Ride Project overview with New/Open folder
actions and persistent name/status. `openride-author` provides native project
create/open/status/rename commands and a versioned JSON-lines contract. Project
paths are relative; staged atomic saves, an exclusive writer lock and revision
checks protect saved state. The release bundles the CLI in `authoring/`.
See [Ride Project usage and recovery](ride-projects.md).

Validation: locked/offline Rust workspace tests (including four new native project
integration tests), all 90 Python tests, release build/export, exported native
project lifecycle checks and existing UX regression passed. A rendered screenshot
was inspected. The project check covers creation, moving/reopening through a fresh
screen, rename persistence, stale-writer rejection and refusal to replace a
nonempty folder. No trainer connection was made.

S09 full-source import is next. The new project currently records an empty stage;
the old section workflow remains accessible as Legacy section builder. This is
not a claim that the whole authoring pipeline is native or that M7 is accepted.

2026-10-12 continuous-package follow-up: the [corridor assembler](continuous-corridor-packages.md)
now builds all 20 OC regions into one 153.126048 km package: 3,716 unchanged
canonical route points, 1,431 chunks and 95,115,919 payload bytes. All terrain
receipts, package hashes and original source snapshots verified. Across 44,652
shared-edge comparisons, heights and normals matched exactly. Explicit bounded
terrain overlap reconciliation handled nine sample calls (maximum source spread
4.468 m, maximum derived height shift 1.891 m); route elevations remain unchanged.
Retained steep segments are explicitly accepted for this unvalidated preview.

Exported full traversal passed: peak nine live chunks, three cached chunks,
798,038 live payload bytes, maximum main update 0.497 ms and mesh commit 3.548 ms.
These headless measurements are observations, not reference-machine FPS/RAM/VRAM
acceptance. The rendered continuous 24 km prefix also passed and its regional
boundary was visually inspected. Projection corruption checks, 90 Python tests,
locked/offline Rust workspace tests, release export and library/ride-entry checks
passed; the library contains two valid packages and no invalid entries.

M7 remains open: other validation recordings, OSM scenery, bridge/stacked-road
terrain handling, incremental assembly recovery, resource measurements and the
30-minute supervised hardware ride are not complete. Terrain acquisition resumes
from verified cache; interrupted assembly retains staging but currently restarts.
Earlier entries below describe the implementation history.

2026-10-12 rideability follow-up: the new [read-only native route audit](route-rideability-audit.md)
completed the full repaired OC route at 150, 200 and 250 W in the default physics
model. The 30 retained >15% raw segments are short (0.36–2.22 m); their 20 m context
grades are all below 12% in magnitude. No further elevation edits or steep-grade
acceptance were made. This is a software profile result, not scenery or hardware
acceptance. Whole-route assembly and regional seam reconciliation remain open.

The repaired revision now has verified terrain and independently baked packages
for 0–24 km (three regions). Thirty-three terrain tiles were reused; 14 more were
acquired. An HTTP 502 recovered by retrying from verified cache. Exported traversal
of the new 16–24 km region passed: 79 chunks, 921 route points, peak nine live and
three cached chunks. Rust workspace tests and 81 Python tests passed. This does
not verify cross-region transitions or close S04; the remaining regions and unified
package/runtime assembly are still pending.

2026-10-12 follow-up: [automatic import repairs](automatic-route-repairs.md) now
apply conservative elevation-noise blends on source/decision saves, with native
before/after comparisons and durable undo/reject. The four approved OC blends
are saved; the seven >100% grade blockers are gone. A new revision-pinned corridor
plans all 20 regions without blockers, preserving 153.126048 km, 3,716 points,
source bytes and geography. Thirty segments still exceed 15% and require review.
Original corridor artifacts remain available; unified package assembly is pending.

Validation: locked/offline Rust workspace tests, 81 Python tests, Windows release
build/export and exported automatic-import/repair accept/reject/undo/save/reopen
checks passed. Synthetic and disposable OC reviews were exercised before applying
the approved changes. This does not establish whole-route scenery or hardware gates.

2026-10-12: initial [resumable corridor jobs](m7-corridor-preparation.md) now plan
the complete accepted OC profile into 20 regions and independently resume terrain,
OSM acquisition and regional baking. The first two regions have verified terrain
and separate packages. Seven additional >100% raw-grade segments block four
regions; the accepted continuity repairs remain unchanged. Whole-route package
assembly, regional terrain seam reconciliation, OSM scenery and S04 acceptance
remain open. Eight new synthetic tests passed (81 Python tests total), along with
locked/offline Rust workspace tests. See the linked document for precise limits.

2026-10-11: S01 review, initial S02 full-route sparse preview and a bounded S03
multi-tile section builder are available. M7 remains open. The [roadmap](m7-roadmap.md)
defines the remaining slices. The newly supplied review under `local/oc_to_solana`
is confirmed to contain the GPX for both source roles and all 13 pause decisions.
The user has now accepted all four local repair proposals, and native profile
export confirms zero remaining breaks or profile issues. Other recording reviews and continuity/elevation
acceptance are still pending, so S01 is not marked accepted.

## Full-ride intake and saved review

Open **Create → Full ride review / compare TCX and GPX**. Import one or more
variants of the same ride, inspect the full map and elevation profile, and choose
geometry and elevation sources independently. The comparison distinguishes
recorded distance from geometry distance excluding source gaps. Sample inspection
shows segment identity, UTC time, elevation and importer diagnostics. Selecting a
gap shows elapsed time, endpoint separation and its diagnostic reasons.

Decisions can be unresolved, keep break, recording pause or missing travel, with
an optional note. Saving them does not modify snapshots. Source choices default
to unselected. The S02 preview can derive short pause connectors as described below.

Each private review directory stores original-byte TCX/GPX snapshots and a
versioned `review.json`. Source hashes and sample indices bind gap decisions;
reopening validates snapshots before applying them. Save checks the opened
revision, holds an exclusive lock and replaces the manifest through a temporary
file. Conflicts and invalid decisions preserve the accepted manifest. Unsaved
edits block opening another review and trigger an exit warning.

Keep these folders private, preferably under ignored `local/`. They contain
personal geography and all original recording fields. Do not commit them. A
failed initial write may leave an incomplete new folder. An interrupted save may
leave `review.lock` or `review.json.tmp`; after confirming all writers have exited,
back up the folder before removing those remnants and reopening the accepted
`review.json`. The UI does not automatically recover interrupted saves.

## Verification

- Three synthetic Rust persistence tests cover independent source/gap identity,
  save/reopen after original removal, stale revisions, invalid decisions, lock
  conflicts, snapshot tampering, duplicate inputs and path traversal rejection.
- Godot integration verifies full-source plots beyond 5 km, explicit source
  choices, saved gap classification, navigation, reopening through a fresh native
  worker, and unchanged originals. Failures exit nonzero in release exports.
- All three private TCX/GPX pairs (Death Ride, L'Étape du California, OC to Solana
  Beach) import and reopen with complete recordings over 100 km; original hashes
  remain unchanged. No real source or gap decisions were selected automatically.
- Locked offline Rust workspace tests and all 68 Python tooling tests passed.
  Windows export, exported synthetic and three-pair review checks, and the existing
  short-project/editor/simulated-ride regression passed (`SOURCE_REVIEW_OK`,
  `SOURCE_REVIEW_PRIVATE_OK`, `UX_OK`, `UX_RIDE_OK`). The first UX run used an
  outdated fake trainer and failed its shift check; rebuilding the fixture from
  current source resolved it. Private logs and screenshots remain under `local/`.

To reproduce the review integration check, run the exported executable with
`--headless -- --verify-source-review --review-root=ABS_NEW_DIRECTORY` using isolated
`APPDATA` under `local/godot-appdata`, hidden execution, redirected logs and a
60-second timeout. The test creates synthetic recordings and requires a new
directory. Optional `--review-inputs=ABS_JSON` takes an array of TCX/GPX path pairs
for private validation; keep that JSON and outputs in `local/`. For a rendered
synthetic screenshot, omit `--headless` and add `--review-snapshot=ABS_PNG`.

The current intake bounds are 1–8 source variants, 64 MiB per source / 128 MiB
combined and 2–100,000 samples per source. Import/save runs in a native worker;
this first overview still retains whole activity arrays in the UI. These are
intake guards, not validated runtime memory/performance budgets. The map is a
schematic local track view, not the canonical long-route projection contract.

## S02: full-route sparse preview

From a saved review, choose **Open full-route sparse preview (no 5 km limit)**.
Geometry/elevation selections must currently use the same source. The native
profile keeps geographic coordinates and cumulative horizontal distance in f64,
uses binary distance lookup, and returns at most 1,024 points around a selected
station (up to ±500 m). Coordinates are rebased before conversion to Godot vectors;
remote discontinuities cannot create global-sized mesh coordinates. Dense sources
may narrow the displayed interval to respect the point bound. A decimated overview
contains at most 513 points and preserves breaks across omitted samples; it is for
navigation, not detailed elevation validation. The full native point index remains
resident; disk-backed profile blocks and large-source performance acceptance remain
follow-ups.

The preview applies only saved recording-pause decisions, with a 5 m endpoint
separation limit, a 1 m height-difference limit and a maximum absolute raw connector
grade of 100%. Co-located samples add no distance and require heights matching
within 1 cm; vertical changes without horizontal travel remain findings. No height
smoothing or flattening occurs. These initial connector limits are conservative
inspection policy, not claims about road quality or trainer safety. All other
breaks remain explicit and add no artificial connector distance.

The slider, elevation overview and ±10 km actions support distant seeks; 30 km/h
playback stops before unresolved breaks and stops when leaving the screen. The
native profile can feed the existing Rust simulation when continuous and within
its existing 10,000-point/grade limits. Missing data prevents that conversion.
This screen does not connect hardware, record activities, install packages or
replace the existing scenery pipeline. It regenerates from the saved review and
rejects a revision conflict rather than previewing stale decisions.

Four Rust contract tests cover >20 km pause bridging, preserved source breaks,
unresolved-gap stops, clipped/bounded coordinates, unsafe pause heights and missing
tail data. The Godot check verifies saving, full-route seeking, playback, navigation,
reopening and unchanged inputs. The 22.13 km synthetic fixture bridges one pause.
A disposable review built from the user's OC GPX reaches 153.119 km with 3,718
retained points, 11 bridged pauses and four retained discontinuities. Test-only
pause decisions were applied to that copy, not the user's saved review. Findings
include a 1 m height change across 0.82 m, a 5.93 m separation, and stationary
height changes. Geographic details and captures remain private under `local/`.

Final verification: locked/offline Rust workspace tests, all 68 Python tests,
Windows export, exported synthetic/OC `FULL_PREVIEW_OK`, `SOURCE_REVIEW_OK`,
`UX_OK` and `UX_RIDE_OK` passed. Rendered synthetic and OC screenshots were
inspected. No hardware test or performance acceptance is implied.

Run `--verify-full-preview --preview-root=ABS_NEW_DIRECTORY` after `--` in the
source or exported app, with the same isolated/hidden/bounded test conventions as
S01. Optional `--preview-source=ABS_GPX` creates a disposable review and explicitly
classifies its breaks for testing; it does not edit the source or original review.
Optional `--preview-snapshot=ABS_PNG` requires rendered execution, not `--headless`.

## Explicit local repair review

The sparse preview now offers source-bound proposals for short pause connectors,
local elevation blends and stationary elevation consolidation. Select a proposal
to inspect its location, per-sample elevation changes, before/after stations and
grades, route length and retained-break count. **Accept repair**, **Reject repair**
and **Undo decision** save through the existing revision/lock/atomic replacement
path. Undo returns the proposal to an inactive state and restores the original
derived profile; snapshots remain unchanged. Other accepted repairs remain applied
in each comparison. A missing grade at a break is shown as unavailable.

The default 5 m pause cutoff is unchanged. Only an accepted source-bound connector
may extend it to 10 m; the existing height/grade checks still apply. Blends use
nearby endpoint heights and horizontal distance over at most 100 m / 32 samples,
changing each height by at most 2 m. Stationary clusters retain their first recorded
height. These are review suggestions, not verified road elevations; steep grades
remain visible. Missing travel, missing samples and unclassified source breaks
are not repaired automatically. At most 128 non-overlapping proposals are offered.

Decisions include exact proposed values in the optional `repairs` map in schema-1
reviews. Existing reviews without that map remain readable. Older clients reject
reviews containing repairs rather than silently ignoring them. Accepted repairs
block conflicting source/gap changes until undone. Inactive decisions can remain
as records after changing source choices. Preview and builder consume the same
corrected native profile. Existing packages are not modified; replan/rebuild from
the saved revision to include accepted repairs.

`content-builder --review-repairs REVIEW NEW_JSON` exports a private comparison
without changing the review. Add `--verify-route-repairs` to the existing
`--verify-full-preview` test to exercise accept/reject/undo in a new disposable
review; optional `--preview-source` still creates a copy, never editing a saved
review. The authoritative OC review now contains all four accepted proposals.
Full-corridor preparation, incremental rebuilds, mountain acceptance, scenery and
performance/endurance gates remain open; this does not close S05 or M7.

Validation on 2026-10-11: locked/offline Rust workspace tests and 73 Python tests
passed. The release Windows build/export and exported synthetic/OC repair lifecycle
checks passed (`ROUTE_REPAIRS_OK`), as did ordinary source review and sparse preview
regressions. In a disposable OC review, accepting all four proposals yielded
153.126048 km, 3,716 retained points, 13 bridged pauses and zero retained breaks;
the native builder exported that same repaired profile. Hashes confirmed the
authoritative review and its GPX snapshot were unchanged. The rendered synthetic
repair screen was inspected; controls fit the default window and the content
scrolls at smaller sizes. This verifies software behavior, not elevation acceptance
or whole-route scenery/trainer performance.

## Next

The user accepted all four OC repairs on 2026-10-11. Native reopen and profile
export verified 153.126048 km, 3,716 points, 13 bridged pauses and zero retained
breaks or profile issues in the authoritative review. Full-corridor preparation
is next; this does not establish a completed full-route scenery package.

Before repair, the saved OC review had four retained findings. Its first 8 km is
continuous and needs no steep-grade override. A real terrain/road package for that
section is now under `local/oc_to_solana/content`; see the [tiled section builder](m7-tiled-sections.md).
It uses 16 USGS source tiles, a shared 10 m terrain lattice and 85 streamed chunks.
The exported app traversed the whole section with at most nine live / three cached
chunks. A synthetic 7 km mountain package preserved ±20% grades across 18 source
tiles and passed exported traversal. Source acquisition recovered from an HTTP
502 by reusing the verified cache. Provenance corruption is rejected before viewing.

Locked/offline Rust tests, 73 Python tests, Windows export, real/synthetic streamed
traversal, library discovery, software-only power-demo and existing UX regressions
passed. Those earlier checks left the saved review unchanged; the user's subsequent
repair acceptance is recorded above. These bounded proofs do not close S03:
Death Ride mountain/user acceptance is still pending. Implement S04 full-corridor preparation
next. Imagery/distant terrain remain S06, and the 30-minute supervised ride and
measured performance acceptance remain S07. No hardware was exercised.
