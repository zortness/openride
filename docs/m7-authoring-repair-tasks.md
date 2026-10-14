# M7 authoring repair tasks

2026-10-13 direction change: the owner approved [road-first authoring](road-first-authoring.md)
and a simpler four-stage editor. Recordings now provide road-sequence evidence;
generated road geometry and terrain heights define the default course. The
recording-repair/elevation-alignment prerequisites below remain compatibility
work, not prerequisites for road rides. S22 migration should preserve prior
decisions as reference rather than silently applying elevation blends to new roads.

Requested 2026-10-12 after the full OC package became previewable. These tasks
address the missing end-to-end creation experience. They extend M7; they do not
claim acceptance of the existing S01–S07 slices. IDs continue at S08 to preserve
existing references. Approved 2026-10-13 as an extension of the M7 roadmap.

S08-S21 implementation baselines are recorded in [M7 progress](m7-progress.md),
including native imagery streaming and editable buildings, vegetation and water.
OC terrain/street coverage is complete. Real-course review, representative scenery
acceptance, complex coastline handling, migration and complete acceptance remain
open. Checkboxes below retain
acceptance gates rather than claiming that implementation alone closes them.

## Intended experience

**Create or open Ride Project → Import sources → Preview and repair → Acquire
maps → Build course → Review and edit → Add scenery → Build and share.**

The complete imported ride is the default. Selecting a subsection is optional.
A project owns its inputs, decisions, downloads, jobs and outputs. Users choose
the project folder and source activities, not internal JSON files. Each stage
shows useful status and the next action. Import logs are collapsed by default
behind a toggle; actionable errors remain visible without opening the log.

One Rust executable (working name `openride-author`) provides all production
import and creation operations through a CLI. Godot calls that same executable.
It can share Rust libraries with the existing backend, but does not require the
trainer backend or Godot to run CLI jobs. Moving Python behind a Rust launcher
does not satisfy this requirement. PowerShell used for developer build/export
automation and Python used for developer tests may remain; neither is required
to create a ride in a distributed release.

Automatic **Repair** closes gaps and removes tiny accidental loops only. It does
not silently smooth elevations or snap to roads. Road/path matching happens in
**Build course**, with reviewable uncertainty and subsequent manual edits.
Previously accepted elevation blends remain recorded and readable; changing
the default repair policy must not discard the accepted OC work.

## Project conventions

Version-one file conventions established in S08 (stage files are created when used):

```text
project.json                 project identity, schema, stage state, references
sources/index.json           original names, hashes, selected source roles
sources/<source-id>.<ext>     immutable original activity snapshots
review.json                  source choices, findings and review decisions
repairs.json                 gap and tiny-loop decisions, undo history
course.json                  derived course and source-occurrence mapping
corrections.json              road/path choices and manual course edits
scenery.json                 buildings, vegetation and water additions/edits
maps/index.json              coverage, providers, hashes and attribution
maps/                        acquired topography, streets and imagery
jobs/                        resumable work, receipts and staging
logs/import.log              import diagnostics, optional in the UI
logs/authoring.log            build and acquisition diagnostics
build/course/                validated unpacked runtime package
exports/<ride-name>.zip      shareable compiled runtime package
```

All paths are project-relative. Moving a folder must not break the project.
Cache files and versioned source IDs can be generated internally; their paths
never become correction-file selection chores. Project folders contain personal
geography and remain outside tracked source, normally under ignored `local/`.
Sharing exports compiled content and required attribution, not original activities,
biometrics, project logs or machine-specific settings.

## Tasks

Each task includes native CLI behavior, the corresponding Godot flow, persistence
and focused validation. Dependencies refer to this breakdown, not completed M7
acceptance. Existing implementations are reusable starting points.

- [ ] **S08: Create and reopen a Ride Project** `risk:medium` `depends:[]`
  > After this: create a project folder, close the app, move the folder and reopen it from either the CLI or Create Ride.
  Introduce the Rust executable with project create/open/status commands and a
  versioned `project.json`. Replace the section-first landing screen with New/Open
  and a project overview. Establish relative paths, atomic saves and stale-writer
  rejection. Define versioned job/progress/result events and exit codes for the
  UI, including cancellation and actionable failures. Verify paths with spaces,
  moved projects, unsupported versions and interrupted writes.

- [ ] **S09: Import one or more complete source files** `risk:medium` `depends:[S08]`
  > After this: import a TCX/GPX pair, inspect the whole recording and reopen the saved source choices without choosing a 5 km interval.
  Integrate existing native source inspection/review with the project and CLI.
  Preserve immutable snapshots, source hashes, units, timestamps and sample identity.
  Compare complementary variants and select geometry/elevation sources; do not
  concatenate different recordings implicitly. Default to full length; offer
  optional cropping separately. Add a collapsed log toggle and concise import
  summary. Verify multi-file selection, duplicate inputs, missing samples and
  reopen behavior through the exported UI and CLI.

- [ ] **S10: Close gaps through the Repair workflow** `risk:medium` `depends:[S09]`
  > After this: inspect a gap, preview a proposed connection, accept or undo it, and retain the decision after reopening.
  Apply bounded, attributable gap closure; distinguish recording pauses from
  missing travel and keep ambiguous connections visible for review. Save all
  decisions automatically in project-owned files. Disable automatic elevation
  blending in the new import/Repair path while preserving legacy accepted blends.
  Verify unresolved gaps, undo/reject and unchanged original source bytes.

- [ ] **S11: Remove tiny accidental loops** `risk:high` `depends:[S10]`
  > After this: remove a small GPS loop in preview while retaining a genuine hairpin, out-and-back and repeated road visit.
  Define bounded loop criteria using route order and spatial extent; uncertain
  candidates remain reviewable. Show the proposed geometry and distance changes,
  preserve source-occurrence mapping, and support undo/reject. Recalculate derived
  stationing without deleting original samples. Verify topology cases and that
  repair does not introduce elevation smoothing or road snapping.

- [ ] **S12: Acquire topography for the whole route** `risk:high` `depends:[S10]`
  > After this: request topography for the complete project, cancel partway through and resume only missing or invalid tiles.
  Port corridor planning, DEM acquisition and cache validation into Rust. Show
  planned coverage, resolution, download estimate and per-region progress on the
  project map. Preserve attribution, overlap rules, bounded requests, retries and
  checksummed receipts. Verify long corridors, missing coverage, interruption and
  cache corruption without falling back to one large low-detail rectangle.

- [ ] **S13: Acquire street and path maps for the whole route** `risk:high` `depends:[S10]`
  > After this: acquire and inspect a continuous road/path map along the ride, including across acquisition boundaries.
  Port OSM acquisition/context preparation to Rust and persist it under `maps/`.
  Preserve graph connectivity, source IDs, path/access tags, bridge/tunnel/layer
  information and feature provenance across tiles. Expose missing coverage and
  retry in the UI. Verify boundary deduplication, connectivity and offline reuse.

- [ ] **S14: Build a course with road/path matching** `risk:high` `depends:[S11,S13]`
  > After this: Build course attempts route-wide road/path alignment and highlights ambiguous or unmatched sections for review.
  Port and extend matching in Rust, using ordered route occurrences and connected
  map paths rather than independent nearest-point snapping. Retain the source
  trace overlay; show distance changes and matching confidence. Do not invent a
  road where coverage is missing. Keep elevation provenance and recorded heights
  distinct from horizontal matching and surface construction. Verify parallel
  paths, hairpins, bridges, crossings, repeated roads and gaps in map coverage.

- [ ] **S15: Edit course issues directly in the project** `risk:high` `depends:[S14]`
  > After this: step through course issues, choose an alternative path or edit an unmatched section, then reopen and rebuild with those edits intact.
  Add an issue list linked to map/3D locations, before/after overlays, manual course
  edits, accept/undo and explicit unresolved states. Persist `corrections.json`
  automatically; remove correction-file pickers from this flow. Bind edits to
  source/map revisions and route occurrences; report conflicts on changed inputs.
  Verify edits at repeated visits and rebuilding without losing unrelated work.

- [ ] **S16: Acquire and stream full-route imagery** `risk:high` `depends:[S12]`
  > After this: acquire optional imagery for the entire corridor and inspect consistent detail at distant points without loading all textures.
  Port imagery planning, downloads and tile preparation into Rust. Reuse project
  coverage, expose quality/download estimates, cancellation/resume and visible
  coverage gaps. Stream nearby imagery with bounded caches and coarse distant
  terrain. Verify seams, attribution, distant seeks and offline reopening.

- [ ] **S17: Build the full terrain-and-course package from the UI** `risk:high` `depends:[S12,S15]`
  > After this: Build in Create Ride produces and opens one continuous full-route preview with no Python or PowerShell authoring process.
  Port tiled preparation, terrain reconciliation, road geometry and continuous
  assembly to Rust; integrate S16 imagery when available. Preserve shared-edge
  heights/normals, canonical course semantics and bounded runtime loading. Resume
  interrupted work, invalidate only dependent assets after edits and publish
  atomically while retaining the last usable package. Verify seams, cancellation,
  missing data, local rebuilds and a complete long route through CLI and UI.

- [ ] **S18: Add editable buildings** `risk:medium` `depends:[S17]`
  > After this: generate buildings from acquired footprints, edit or remove one and retain that change after a local rebuild.
  Port building generation to Rust, deduplicate boundary features and save user
  overrides in `scenery.json`. Include bounded rendering and missing-height
  defaults with provenance. Verify persistent add/edit/remove and route clearance.

- [ ] **S19: Add editable shrubbery and vegetation** `risk:medium` `depends:[S17]`
  > After this: generate vegetation from available land-cover information and add or clear a patch without it reappearing after rebuilding.
  Implement deterministic, bounded Rust placement with project-owned overrides,
  exclusion zones and stable identities. Keep the rideable course clear. Verify
  tile boundaries, repeatable rebuilds, density budgets and manual exclusions.

- [ ] **S20: Add editable water bodies and coastline** `risk:high` `depends:[S17]`
  > After this: lakes, rivers and coastline span tile boundaries, and a water-boundary edit survives reopening and rebuilding.
  Port water/coastal geometry into Rust, retaining holes and multipolygon topology.
  Coordinate water surfaces with terrain and preserve bridges/stacked crossings
  without flattening the road onto water. Persist manual additions and corrections.
  Verify shore seams, islands, missing data and representative bridge cases.

- [ ] **S21: Share ZIP packages and load either representation** `risk:medium` `depends:[S08]`
  > After this: export a compiled package as an ordinary `.zip` and load either that ZIP or its unpacked folder into the ride library.
  Implement native pack/validate/install CLI operations and UI Export/Open using
  an existing valid package first; integrate new S17 outputs afterward. Recognize
  `manifest.json` and validate its required referenced files and hashes rather
  than requiring a `.orpack` directory suffix. Preserve old unpacked packages.
  Choose bounded extraction into a managed cache or an equivalent bounded reader;
  reject unsafe paths, duplicate entries, oversized expansion and incomplete
  packages. Verify ZIP/unpacked equivalence, offline traversal and export contents.

- [ ] **S22: Migrate existing projects and remove the old creation path** `risk:medium` `depends:[S09,S11,S15,S16,S17,S18,S19,S20,S21]`
  > After this: open an existing short-route project or full-route saved review in the new workflow, preserving originals and accepted edits.
  Offer an explicit, reversible migration to the Ride Project conventions; reuse
  verified assets where compatible and identify what must be rebuilt. Preserve
  the accepted OC gap/elevation decisions without reapplying old automatic repair
  policy. Remove runtime Python settings, production script launchers and internal
  file pickers after their native replacements pass parity checks. Update setup,
  CLI and user documentation; developer build scripts may remain.

- [ ] **S23: Verify the complete native creation experience** `risk:high` `depends:[S22]`
  > After this: on a clean Windows install, create, repair, acquire, edit, build, share and reopen a full ride using only the shipped UI and native CLI.
  Exercise OC, Death Ride and L'Étape plus short-route regressions. Verify relocated
  projects, process interruption, source-change conflicts, offline reopening and
  ZIP/unpacked traversal. Measure build time, download/disk volume, frame pacing,
  memory and GPU residency against agreed budgets. Confirm the log stays hidden
  by default and no internal filenames or interpreter setup are required. Obtain
  user acceptance of the workflow and representative course/scenery output.

## Implementation order and existing gates

Start with S08–S10. Then pursue the road-matching path S11 → S13 → S14 → S15 early,
using verified local street data where available to expose ambiguity risks.
Topography (S12), imagery (S16) and ZIP loading (S21) have independent paths.
S17 combines the course and terrain. S18–S20 add scenery independently, followed
by migration/cutover and complete integration. Dependencies are prerequisites,
not an instruction to launch parallel agents.

The Rust port is delivered within each stage, with existing Python outputs kept
as comparison evidence until parity is established. Compare semantic geometry,
stationing, provenance and error behavior with declared tolerances; byte equality
is not required for floating-point output. Do not ship a stage as native while
silently retaining a Python/PowerShell production dependency.

The original S01–S07 acceptance gates still apply. S23 supplies authoring and
software integration evidence for S07; it does not replace the separate supervised
30-minute hardware ride or close M6. Synthetic checks cannot establish physical
trainer behavior. Supported route/resolution budgets remain explicit.

## Scope reconciliation

The M7 brief and tooling roadmap now include native authoring migration and the
project/course/scenery flow. This breakdown extends the approved roadmap while
preserving historical evidence and incomplete acceptance gates. Treat older automatic
elevation-repair documentation as existing behavior being superseded for new
projects, not as authorization to delete accepted decisions.

## Code entry points

- `apps/rider/route_workflow.gd`: current section-first UI and Python job runner.
- `apps/rider/full_ride_review.gd` and `full_route_preview.gd`: native intake
  and full-route review consumers to integrate into the project workflow.
- `crates/content-builder` and `crates/ride-core`: existing native import,
  review, profile, terrain and validation capabilities to reuse behind the CLI.
- `tools/route-workflow.py`, `corridor-scenery.py`, `assemble-corridor.py` and
  the acquisition/scenery utilities: production behavior to migrate by stage.
- `apps/scenery-preview`: source editor and streamed rendering integration;
  generated copies in `apps/rider/scenery` and `apps/rider/editor` are not edited.
- `apps/rider/content_library.gd` and `apps/scenery-preview/package_library.gd`:
  package discovery/validation to extend alongside streaming reads through a
  shared packed/unpacked access contract.
