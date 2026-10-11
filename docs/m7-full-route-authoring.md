# M7 — Full-route authoring and inspection

Status: agreed 2026-10-11; implementation has not started. This brief follows the
repository's existing milestone documentation layout. M6 remains open.

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

Use a user-selected full ride longer than 5 km. Inspect its source quality, build
its scenery, review tile boundaries and repairs, install its package, and traverse
the full route offline. Reopen the project and package successfully. Demonstrate
resuming interrupted preparation without discarding completed valid work. Record
build time, output size and runtime resource measurements on the test machine.
The user confirms the result is useful for full-ride inspection.

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
distance and repairs; one streamed package; boundary and resource validation.

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

## Acceptance Criteria

- The selected complete ride imports without truncation at 5 km.
- Source timestamps, units, gaps and diagnostics are inspected before preparation.
- Terrain detail follows an explicit resolution budget across the route.
- Boundaries do not create artificial jumps in route distance, elevation or grade.
- Repairs survive reopening and unaffected regions survive resumed preparation.
- One package validates, installs and supports full-length offline traversal.
- Resource measurements and supported limits are documented; short routes regress
  successfully.

## Open Questions

- Which full ride is the first acceptance input, and how large is its extent?
- What terrain/imagery resolution and supported route extent should be targeted?
- What tiling, overlap and coordinate strategy best fits the existing formats?
- What build-time, memory and frame-time budgets apply on the reference machine?

The first planning step is to inspect the chosen full recording and use its
extent and diagnostics to set the initial implementation and validation budgets.
