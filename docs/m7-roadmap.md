# M7 — Full-route authoring implementation roadmap

Direction updated 2026-10-13: [road-first authoring](road-first-authoring.md)
supersedes mandatory recording-geometry/elevation repair for road rides. Recordings
provide evidence; roads and terrain define the generated course. Preserve existing
repair decisions as reference and retain the acceptance gates below.

Extension approved 2026-10-13: [S08–S23 authoring repair tasks](m7-authoring-repair-tasks.md)
add Ride Projects, a Rust authoring CLI, full-corridor course/scenery creation and
ZIP sharing. This is part of M7; the S01–S07 acceptance gates below remain open
until their evidence is recorded. Start the new sequence with S08.

Approved 2026-10-11. S01 review, initial S02 preview and a bounded S03 multi-tile
section build are available; see
[M7 progress](m7-progress.md). This roadmap follows the
repository's existing documentation layout and implements the
[M7 brief](m7-full-route-authoring.md). M6 remains open independently.

## Vision

Inspect and prepare each complete Death Ride, L'Étape du California and OC to
Solana Beach recording as one route backed by independently prepared scenery
tiles. Begin with full-route inspection, then prove mountain elevation continuity,
then scale preparation and rendering. Existing short-route projects remain usable.

## Success Criteria

- All three supplied recordings can be inspected, reviewed, packaged, reopened
  and traversed offline without the current 5 km authoring restriction.
- Source selection, gap decisions and elevation corrections are explicit and
  durable; originals remain unchanged.
- Tiling introduces no artificial distance, elevation or grade discontinuities.
- Builds resume safely and local edits invalidate only their dependent assets.
- Runtime and build resource use is measured and bounded against agreed budgets.
- One continuous 30-minute supervised ride completes with telemetry/control
  evidence and an inspectable export, fulfilling the gate transferred from M2.

## Architecture and Invariants

1. **One canonical route.** Preserve source hashes, sample/segment identities,
   route order and reviewed elevations. Recorded distance, corrected route distance
   and elapsed/timer time remain distinct. Repeated visits to one geographic tile
   retain distinct route occurrences.
2. **Reviewed continuity.** Distinguish stationary recording pauses, missing travel
   and broken tracks. Inspection can show unresolved gaps; a rideable package must
   not silently replace them with straight connectors. TCX/GPX source choice is
   reviewed rather than inferred from the number of retained samples.
3. **Continuous elevation.** Compare recorded heights, terrain and vertical-reference
   metadata; retain source provenance and local corrections. Distance-based filtering
   uses overlap across partition edges and preserves summits, valleys and signed
   grade. Accept reviewed steep roads without flattening them to satisfy the current
   15% finding threshold. Bridges, tunnels and nearby hairpins need explicit handling.
4. **Geographic assets, local rendering.** Use double-precision geographical positions
   and route distance, with bounded local mesh coordinates. Prove projection and
   origin handling against the broad OC route. Reuse geographic tiles without
   collapsing route order; rebase rendering where precision measurements require it.
5. **Corridor acquisition.** Terrain and OSM acquisition follow the route corridor,
   with consistent near-road resolution and shared tile borders. Source tiles and
   render chunks are separate units. Coarser distant terrain provides mountain views.
6. **Incremental publication.** Source and derived-asset hashes, attribution,
   coverage and dependency records drive caching and invalidation. Bake/write
   incrementally. Cancellation or failure preserves verified work and the last
   complete package. Deduplicate OSM objects by identity and keep road solving local.
7. **Versioned, bounded consumers.** Index profile blocks, overview samples and asset
   manifests. Stream imagery tiles separately. Builder, editor, Godot traversal,
   Rust simulation and export lookups share the route contract. Retain compatibility
   with existing packages and account for resident indexes as well as visible meshes.
8. **Route physics is distinct from trainer load.** Signed route grade drives
   simulation and geography exports. M7 does not expand M6's 0–2% trainer-control
   envelope. Document existing physics assumptions rather than silently adding
   altitude-dependent behavior. Multi-hour hardware sessions are not an acceptance
   requirement; current 40-minute session/45-minute outer limits permit the 30-minute
   gate but do not permit riding every supplied recording in real time.

## Key Risks

- Sparse/invalid source points and timestamp gaps can make a full recording
  importable without making it a continuous rideable route.
- Elevation noise, incompatible vertical references and independent tile filtering
  can introduce false climbs, flatten summits or produce seams.
- Hairpins, repeated roads and stacked road geometry cannot be resolved solely by
  nearest XY distance.
- Current limits include local projection extent, DEM dimensions, road-graph size,
  route-point count, package metadata/chunk limits and a single imagery texture.
  Raising only the 5 km cap is insufficient.
- Whole-route metadata or build buffers can grow even when visible mesh counts stay
  bounded. Cache and distant-seek behavior need explicit measurement.

## Slices

- [ ] **S01: Full-ride intake and saved review** `risk:medium` `depends:[]`
  > After this: open all three source pairs, compare variants, and save source/gap decisions with a full-route overview; existing short projects still open.
  Include stable source/sample identity and explicit unresolved-gap states. User
  review is needed for source and geometry decisions; importer execution alone is
  not completion of this slice.
- [ ] **S02: Full-route profile-only preview** `risk:high` `depends:[S01]`
  > After this: inspect the complete OC route in a sparse road/profile preview with indexed data and local coordinates, including distant seeks and visibly unresolved gaps, before acquiring scenery.
  Prove the shared profile contract in authoring, traversal and simulation consumers.
- [ ] **S03: Mountain terrain across tile boundaries** `risk:high` `depends:[S02]`
  > After this: build a reviewed Death Ride climb/summit/descent longer than 5 km across multiple source tiles and inspect one offline package with continuous height/grade and retained steep-section decisions.
  Include synthetic seam, steep-grade, hairpin and stacked-road cases. Visual
  elevation/repair acceptance requires user participation.
- [ ] **S04: Resumable full-corridor preparation** `risk:high` `depends:[S03]`
  > After this: build the full Death Ride terrain/OSM corridor, interrupt and resume acquisition/baking, and retain the previous usable package when a build fails.
  Expose per-region progress, errors and retry; avoid whole-route build buffers.
- [ ] **S05: Local repair and rebuild** `risk:medium` `depends:[S04]`
  > After this: repair an L'Étape region, rebuild affected assets only, and retain decisions elsewhere, including repeated visits to the same road.
  Verify reopening and source-change conflicts, not just immediate rendering.
- [ ] **S06: Streamed imagery and distant terrain** `risk:high` `depends:[S04]`
  > After this: inspect all three routes with consistent nearby imagery detail and coarse distant scenery while texture/terrain residency, distant seeks and cancellation remain bounded.
- [ ] **S07: Integrated validation and performance** `risk:high` `depends:[S05,S06]`
  > After this: reopen, install and traverse all three routes offline, inspect representative seams/elevation features, address measured bottlenecks, and complete one supervised 30-minute ride.
  This is the final cross-process integration and physical acceptance slice.

S05 and S06 can proceed independently after S04. Most implementation and synthetic
verification can proceed without hardware. Detailed slice planning may split a
large slice into smaller demonstrable flows without losing these outcomes.

## Proof Strategy

Initial read-only intake of both TCX/GPX variants passed for all three recordings;
source hashes were unchanged. Detailed measurements and diagnostics remain private
under `local/m7-intake`. This is intake evidence, not scenery or continuity acceptance.

- Compare tiled and untiled synthetic constant slopes and summit crossings within
  declared numeric tolerances; preserve signed grades and route ordering.
- Exercise gaps, missing DEM, valid steep grades versus spikes, repeated tile visits,
  hairpins, stacked roads, corrupted caches, interrupted builds and distant seeks.
- Preserve RPV/Red Rock short-route behavior and prior package compatibility.
- Traverse all three complete supplied recordings offline. Do not assume the
  recording contains an entire official event course or force corrected distance
  to equal a device-reported total. Treat ascent/descent as dependent on documented
  gap handling and filtering, not an unqualified raw-sample sum.
- Run a separate supervised 30-minute physical ride; log equipment, telemetry gaps,
  acknowledgements and exported results. Simulated playback does not satisfy it.

## Verification Classes

| Class | Evidence |
|---|---|
| Contract | Synthetic identity, stationing, elevation, seam and invalidation tests |
| Integration | Authoring through package installation, preview, simulation and export |
| Operational | Offline reopening/traversal, cancellation/resume and measured resource use |
| User/hardware | Reviewed source/gap/elevation decisions and the supervised endurance ride |

## Budgets to Establish

Trial 1–2 km source tiles and 5–10 m near-road terrain sampling are starting
experiments, not final dimensions or claims of source accuracy. Resolve overlap,
coordinate strategy, imagery resolution and data coverage through S02/S03.

Start from the existing proposed 1080p/60 FPS and 16 GB RAM/8 GB VRAM reference
baseline, with proposed 8 GB application/6 GB GPU budgets. These do not establish
minimum hardware. Set explicit frame/stall, precision and quality thresholds after
S02/S03 measurements and before final acceptance. Record build/startup time,
disk/download volume, frame-time p50/p95/p99 and maximum, movement-wait duration,
live/cache counts, CPU/GPU memory peaks and growth over repeated traversals.

## Requirement Coverage

| Requirement | Slices |
|---|---|
| Complete recording intake and preserved originals | S01, S02 |
| Elevation fidelity and continuous stationing | S02, S03, S05 |
| Scalable, resumable preparation | S03, S04 |
| Durable repairs and source provenance | S01, S03, S05 |
| One compatible offline package per recording | S02–S07 |
| Bounded imagery/terrain/profile consumption | S02, S04, S06, S07 |
| M2 endurance and M3 performance follow-ups | S07 |

## Cross-Cutting Checklist

- Preserve zero versus missing, segment breaks and original source identity.
- Keep personal recordings, geographic analysis and downloaded assets out of Git;
  commit synthetic regression fixtures only.
- Preserve attribution and per-tile data provenance.
- Keep acquisition, recording and trainer control independent of rendering.
- Make failures inspectable and prevent incomplete packages replacing valid ones.
- Use isolated Godot preferences and bounded automated processes; hardware tests
  require applicable authorization and readiness.

## Boundary Map

Names below describe proposed contracts, not existing API declarations.

- **S01 → S02:** saved source selection and review decisions keyed by source hash,
  sample/segment identity and route occurrence; unresolved gaps remain explicit.
- **S02 → S03:** indexed canonical profile, global stationing, local-coordinate
  transforms and overview representation shared by authoring/runtime consumers.
- **S03 → S04:** deterministic source-tile plan, seam rules, elevation provenance,
  reviewed steep sections and compatible package metadata.
- **S04 → S05/S06:** resumable build manifest, checksummed asset IDs/dependencies,
  staged publication and tile-to-route-occurrence index.
- **S05/S06 → S07:** durable local repairs, versioned packages, bounded loaders and
  measurement hooks for complete offline integration and hardware acceptance.

## Definition of Done

All seven slice outcomes have recorded evidence; the three full recordings pass
offline integration, short-route regressions remain intact, resource results are
assessed against agreed budgets, and the 30-minute supervised ride is recorded.
Outstanding coverage or quality limitations are explicit. M6 compatibility gates
are neither claimed nor closed by M7 completion.
