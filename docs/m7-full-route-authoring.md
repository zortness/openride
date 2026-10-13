# M7 — Full-route authoring and inspection

Status: implementation started 2026-10-11. S01 full-ride intake/saved review and
the initial S02 full-route sparse preview are available. A bounded S03 multi-tile
builder has produced an OC 8 km terrain/road package; see [M7 progress](m7-progress.md).
See the [M7 roadmap](m7-roadmap.md) for the approved seven-slice sequence and
verification strategy. M6 remains open.

## Project Description

Allow a complete personal ride to become one inspectable 3D route project and
offline package, extending authoring beyond the current 5 km section limit.

## Why This Milestone

The user wants to import a full ride and inspect it. The source-activity inspector
already accepts full recordings; the limit is in scenery preparation. Current
authoring uses one geographic rectangle, a fixed elevation raster and bounded
map/geometry data. Raising the distance constant alone would not preserve detail
or establish that larger projects can be built and inspected reliably.

## User-Visible Outcome

- Import a complete recording and inspect its extent, elevation, samples and gaps.
- Prepare scenery for the full route with visible progress and resumable work.
- Inspect and repair the route across scenery boundaries in one project.
- Install one offline package and traverse its full length in the preview.

## Completion Class

Integration and operational: synthetic tests support the contracts, but acceptance
requires a real full-ride project and inspection in the Windows application.

## Final Integrated Acceptance

Use all three user-selected validation recordings: Death Ride, L'Étape du
California and OC to Solana Beach. Inspect both TCX and GPX source variants before
choosing geometry/elevation inputs. Build scenery for each complete supplied
recording, review tile boundaries and repairs, install its package, and traverse
the full route offline. Reopen each project and package successfully. Demonstrate
resuming interrupted preparation without discarding completed valid work. Record
build time, output size and runtime resource measurements on the test machine.
The user confirms the result is useful for full-ride inspection.

Transferred from M2/M3 on 2026-10-11: complete a continuous 30-minute supervised
ride on sufficiently long content, recording telemetry freshness, control
acknowledgements and the resulting activity. Measure frame-time distribution,
loading stalls, peak memory and GPU residency on a named reference machine;
address performance issues against budgets agreed before evaluation. These gates
remain unverified despite M2/M3 closure. No separate M8 is currently assigned.

## Architectural Decisions

### Bound work geographically while preserving one route

Extend acquisition and preparation around tiles or bounded sections, preserving
continuous distance along the route and stable source references. The exact tile
layout remains a planning decision. Simply increasing the 5 km cap would stretch
the fixed raster and enlarge whole-area map requests without solving scaling.

### Reuse source inspection and runtime streaming

Keep the existing source inspector and streamed runtime as foundations. Expand
the authoring stages that currently operate on a single bounded area; runtime
streaming alone does not remove those constraints. Preserve review decisions and
data provenance when rebuilding affected scenery.

## Error Handling Strategy

Identify the failed acquisition or preparation region and retain completed work.
Surface source gaps, missing terrain, unavailable map data and invalid edits;
do not silently bridge gaps or fabricate heights. Keep the last usable project
and package intact when a rebuild fails. Validate cached inputs before reuse.

## Risks and Unknowns

- Long rides can span large areas even when the route corridor is narrow.
- Terrain, imagery and map boundaries can introduce seams or duplicate geometry.
- Whole-route metadata and editor overlays may grow despite runtime streaming.
- Source quality, service coverage and download cost depend on the selected ride.
- The first read-only intake found timestamp gaps in all three source pairs and
  missing positions in the TCX variants. Geometry lengths differ between variants;
  source choice and gap repair must be explicit, not automatic concatenation.
- Existing steep-grade findings can block packaging. Review must distinguish real
  steep roads from bad height samples without flattening valid climbs or descents.

## Existing Codebase / Prior Art

The activity importer and inspector provide full-source diagnostics. The content
builder and authoring workflow currently prepare bounded sections. Terrain and
OSM acquisition, tiled imagery, road correction and repair, package generation
and runtime chunk streaming provide the existing integration points.

## Relevant Requirements

Offline Windows use, preserved original recordings, explicit missing data,
reproducible scenery, durable review decisions and bounded resource use. No formal
requirement IDs have been assigned.

## Scope

### In Scope

Full-ride intake and inspection; scalable terrain/map acquisition; imagery coverage
consistent with the larger project; resumable preparation; continuous route
distance and repairs; one streamed package; boundary and resource validation;
30-minute ride acceptance and performance improvements transferred from M2/M3.

### Out of Scope

M6 device compatibility and virtual shifting, Android, multiplayer, junction
navigation, global map coverage and a wholesale authoring-language migration.

### Non-Goals

Unlimited route sizes or unmeasured performance guarantees. A longer route must
retain explicit supported coverage and resource budgets.

## Technical Constraints

Windows desktop, existing Godot/Rust application and authoring tooling. Original
activities remain unchanged and private; generated content and downloaded sources
stay outside version control. External data attribution and license requirements
remain attached to the package. Ordinary package inspection works offline.

## Integration Points

Source import, authoring project state, terrain/OSM/imagery services and caches,
repair tools, package validation, offline route library and streamed preview.

## Testing Requirements

Synthetic cases cover tile boundaries, repeated visits to the same region,
source gaps, missing coverage, interrupted preparation and cache invalidation.
Regress short-route authoring and saved repairs. Exercise the chosen full ride
end to end, inspecting terrain/road continuity and measuring build and runtime
resources. Personal recordings and geographic exports remain private test inputs.

The endurance test requires actual supervised trainer riding, not simulated
playback. Record equipment, duration, telemetry gaps, control responses and export
results. Performance checks include movement waits at chunk boundaries and
whole-route metadata growth as well as frame times and CPU/GPU memory.

Validation corpus selected 2026-10-11: Death Ride, L'Étape du California and OC to
Solana Beach. All six existing TCX/GPX files passed the read-only importer in the
initial intake; source hashes were unchanged. Detailed measurements, geographic
data and diagnostics remain in ignored local analysis artifacts. This establishes
importability, not continuous-route correctness or scenery readiness. Keep short
RPV/Red Rock projects as regressions alongside the three long recordings.

## Acceptance Criteria

- All three selected complete recordings import without truncation at 5 km.
- Source timestamps, units, gaps and diagnostics are inspected before preparation.
- Terrain detail follows an explicit resolution budget across the route.
- Boundaries do not create artificial jumps in route distance, elevation or grade.
- Repairs survive reopening and unaffected regions survive resumed preparation.
- One package validates, installs and supports full-length offline traversal.
- Resource measurements and supported limits are documented; short routes regress
  successfully.
- A continuous 30-minute supervised ride completes with recorded control responses,
  telemetry freshness and an inspectable activity export.
- Frame pacing, chunk-loading stalls and memory/GPU residency are measured on a
  named machine and assessed against agreed budgets; remaining issues are explicit.

## Open Questions

- Which TCX/GPX geometry and elevation sources, and which gap resolutions, should
  be accepted for each of the three selected recordings after detailed review?
- What terrain/imagery resolution and supported route extent should be targeted?
- What tiling, overlap and coordinate strategy best fits the existing formats?
- What build-time, memory and frame-time budgets apply on the reference machine?

Initial intake is complete and the implementation sequence is approved. Start
with S01: saved full-ride source/gap review. Resolution/resource budgets remain
explicit planning decisions for S02/S03. No physical trainer tests ran during intake.
