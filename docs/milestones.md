# Proposed milestone roadmap

Status: M1 is complete; see [M1 progress](m1-progress.md) for verification and the engine decision. [M2 implementation](m2-progress.md) remains in progress with deferred hardware acceptance. [M3 scenery preparation](m3-progress.md) has started. [M4 offline ride](m4-progress.md) is accepted for the tested Windows/Suito BLE/RPV setup, including single-file FIT/TCX imports in GoldenCheetah 3.8. [M5 hardware/content hardening](m5-progress.md) has started with the ANT receive diagnostic. Later milestones remain proposed.

Audience: the project owner and contributors selecting the next bounded piece of work. Read the [project plan](project-plan.md) for scope and architecture. Milestones are defined by observable outcomes; estimates should follow the first feasibility findings.

## M1 — Import and inspect a real route

Outcome: turn the supplied RPV export into an inspectable route without losing the original activity, and prove the basic Rust/Godot integration.

Scope:

- Use the confirmed Windows desktop scope with mobile deferred. Record the test machine and licensing direction; use the 16 GB RAM / 8 GB VRAM baseline from the project plan.
- Establish a small Godot application with a thin Rust extension and independent activity/route models.
- Import TCX and GPX, including the supplied sensor extensions and lap boundaries.
- Distinguish paired exports of an activity without relying only on filenames.
- Display route shape, elevation versus distance, source measurements, gaps, and data-quality findings.
- Support choosing a route subsection and exporting its route data.
- Define a draft manifest sufficient to reopen the inspection result; keep the full world schema provisional.

Acceptance:

- Every supplied XML sample imports or produces a precise, justified diagnostic; valid missing data is supported.
- RPV shows the source 12.54 km lap total separately from any recalculated distance.
- Paired TCX/GPX imports do not create unrecognized duplicate activities; complementary information remains attributable.
- Missing positions, missing sensors, lap transitions, timestamp anomalies, and time gaps are visible rather than silently fabricated away.
- The exported desktop app runs the Rust extension and opens local files offline.
- Original files remain unchanged. A written decision records whether to retain Godot.

Boundary: this is a route inspection application, not a finished 3D world editor. Use a simple track view or local schematic; a hosted basemap must not become an offline requirement.

## M2 — Reliable trainer session without scenery

Outcome: run a supervised Suito session with observable telemetry, controlled simulation requests, and recoverable recording.

Scope:

- Inventory actual device models, firmware, adapters, and exposed capabilities.
- Connect to the Suito over BLE FTMS; acquire control and receive telemetry.
- Exercise a short synthetic grade profile with bounded transitions and explicit pause/resume.
- Add heart-rate broadcast and cadence-source selection when equipment permits.
- Record a durable session and export a minimal valid FIT activity plus inspectable CSV.
- Create replayable telemetry fixtures so ordinary development does not require pedaling.

Acceptance:

- Complete a continuous 30-minute supervised session with recorded control responses and telemetry freshness.
- Observe deliberate trainer disconnect, sensor dropout, pause, and reconnect; document actual trainer behavior and prevent silent control resumption.
- A UI stall does not block controller work; a simulated crash leaves a recoverable activity.
- Missing power is distinguishable from zero power. The same trainer cannot have two internal control owners.
- FIT output opens in GoldenCheetah and another independent decoder with consistent samples, durations, and units.

Scheduling update (2026-10-09): the user deferred the 30-minute ride and deliberate dropout/disconnect tests. FIT import and power display are user-confirmed; independent decoding and detailed pause/summary validation remain pending. The user authorized moving on to M3 while these acceptance items remain open.

Dependency: M1 establishes the application/native integration. Hardware scheduling can influence order, but device work must not hold up source-data investigation. Heart-rate availability is not a prerequisite for testing trainer control; any unavailable sensor remains an explicit unverified item.

## M3 — Build, correct, and share one 3D route package

Outcome: produce a small RPV scenery package with the standalone builder, inspect it visually, and install it in the exported client independently.

Scope:

- Inspect local elevation and OSM coverage; compare available road elevations with the activity.
- Evaluate OSM2World and a minimal custom path for the same representative section, then choose the initial pipeline.
- Prepare route-adjacent terrain, roads, basic buildings/vegetation, and distant scenery.
- Add correction records for road alignment and elevation; preview them in Godot.
- Stream chunks, manage coordinates, and validate package dependencies and attribution.
- Discover dropped-in packages and show incompatible/missing content errors.

Acceptance:

- Install the package into a fresh local content library without rebuilding the app or installing GIS tools on the rider side.
- Traverse the route with simulated telemetry and internet disconnected.
- Show a visual correction surviving regeneration; changed source references produce a conflict report.
- Road appearance and simulated grade derive from the same corrected profile, including any identified bridge/tunnel cases.
- Record frame-time distribution, peak memory, load stalls, package size, and build time on a named Windows reference machine with 16 GB RAM and 8 GB VRAM. Start from the proposed 1080p/60 FPS target and 8 GB application-memory / 6 GB GPU-memory budgets in the project plan; settle detailed acceptance thresholds before evaluating this gate.
- Demonstrate that nearby geometry remains bounded as traversal continues; identify any entire-route metadata retained in memory.

Scheduling update (2026-10-10): the user accepted the coastal/building preview and deferred vegetation and further surface detail. Prioritize exported-client package integration and performance measurement before M4. The exported client now discovers streamed packages and opens a simulated riding preview; see [desktop integration](m3-desktop-integration.md). Performance acceptance remains open.

Boundary: believable route scenery is sufficient. High-detail landmarks and comprehensive editing tools are later additions.

Transition decision (2026-10-10): the user considers current performance adequate to proceed to M4. Retain reference-machine/GPU-residency checks and brief chunk-related movement waits as follow-up work; they no longer block starting the integrated ride. This decision does not establish tested minimum hardware specifications.

## M4 — First complete offline ride

Outcome: an athlete can install content, pair devices, ride a real route, finish, and inspect the exported result.

Implementation update (2026-10-10): capture replay is user-confirmed. The live riding view now connects the Suito controller to route physics, bounded grade control and durable recording; software/fixture checks passed. A short supervised ride succeeded, and the user verified FIT, TCX and GPX exports in GoldenCheetah. Full-route completion with ordinary pause/resume is now journal-verified. The user confirmed compatible TCX power/map import and FIT with all data in GoldenCheetah 3.8. M4 is accepted for this tested configuration; see [M4 progress](m4-progress.md).

Scope:

- Integrate route simulation, streamed scenery, real power input, and trainer grade control.
- Add minimal ride UI: pairing, route selection, pause/resume, progress, live metrics, and finish summary.
- Persist chosen sources and session state without hiding connection failures.
- Complete TCX and GPX exports alongside FIT/CSV and distinguish virtual positions.

Acceptance:

- Complete an RPV ride offline, including a climb, descent, pause/resume, and activity export.
- Verify the visible road, reported course grade, controller target, and simulated progress agree under documented smoothing/limits.
- Reopen the resulting activity in GoldenCheetah; inspect timing, power, HR/cadence where present, laps, and virtual distance.
- Retain the precise route/world versions used in the ride record.
- No account, content server, or development tools are needed for the prepared experience.

This is the first integrated product proof, not a promise of broad hardware compatibility.

## M5 — Hardware and content hardening

Outcome: a repeatable desktop release candidate with a broader validated content/device set.

Implementation: [M5 progress](m5-progress.md). ANT USB-m communication and short Fenix HR reception are verified. Optional selected ANT HR now integrates with live rides and exports; simultaneous physical ride validation remains pending. The separate chainstay cadence sensor is deferred; Bluetooth retains Suito power/cadence/control.

- Add USB ANT adapter integration, cadence/HR profiles, and Suito FE-C with single-controller enforcement.
- Exercise Torrance Beach, Death Ride, and difficult historical imports; expand geographic corrections based on observed problems.
- Complete activity import/export coverage including FIT samples obtained or generated for testing.
- Improve package installation, dependency handling, data recovery, authoring documentation, and public synthetic fixtures.
- Publish a tested compatibility matrix and licensing/attribution inventory.

Do not claim untested USB trainer models or infer ANT support from BLE success.

## M6 — Multi-device pairing and friend testing

Agreed 2026-10-11; discovery/diagnostics implementation has started. See
[M6 progress](m6-progress.md) and the [M6 brief](m6-device-compatibility.md)
for outcomes, device evidence, acceptance gates and unresolved equipment details.

Target setups: Echelon EX-5S, KICKR CORE with Zwift Cog/Click, and Saris M2.
Windows remains first; the Echelon bike can be investigated independently of its
screen. Virtual shifting is in scope because the CORE setup uses a single cog.

Recommended sequence:

1. Package discovery and diagnostics for the friends' equipment.
2. Pair by role; broaden standard trainer and Bluetooth sensor support.
3. Develop virtual shifting on the Suito with a fixed physical gear and gamepad.
4. Validate CORE/Cog/Click, with direct input or an optional bridge.
5. Validate M2; bring FE-C forward if needed, using Suito for local development.
6. Investigate Echelon capabilities; choose a bounded adapter/bridge or manual mode.

Separate ready-for-friends acceptance from physical compatibility validation.
M5 work and deferred endurance/dropout checks remain open. FE-C work overlaps M5
and M6 and should be tracked once, with results linked from both milestones.

## M7 — Full-route authoring and inspection

Agreed 2026-10-11 as a separate milestone; implementation has not started.
See the [M7 brief](m7-full-route-authoring.md). M6 remains open independently.

Outcome: import a complete personal ride beyond the current 5 km authoring cap,
inspect and repair it as one 3D project, and build one offline streamed package.
Full-source inspection is already available; scalable scenery preparation is the
new work. Begin with intake of a user-selected ride, then plan bounded acquisition
and preparation that preserve continuous route distance and review decisions.

Acceptance includes full-length offline traversal, inspection across scenery
boundaries, resumable preparation, preserved original data and measured resource
use. The first ride and supported size/resolution budgets remain to be selected.

## Later milestones

See the [product backlog](backlog.md) for additional deferred requirements, including multiple rider/bike profiles and saved combinations. These items have no assigned milestone yet.

Prioritize separately after the offline ride and compatibility foundations:

1. Structured ERG workouts, with explicit transitions between workout and terrain control.
2. Additional desktop platforms and native phone feasibility/product scope.
3. Content catalog and community authoring/distribution workflow.
4. Standalone device controller and optional local network companion.
5. Ghost rides, then multiplayer coordination and drafting.
6. Optional route branching/steering and additional device families.

## First discussion decisions

Recommend starting with M1: it uses the available files, exposes import assumptions, and establishes the visual workflow without depending on trainer availability. Keep M2 close behind so rendering work does not get far ahead of reliable hardware integration.

Windows desktop first, deferral of mobile, MIT for original code, and RPV as the test ride are confirmed. The initial capacity baseline is 16 GB system RAM and 8 GB dedicated VRAM. The M1 progress document records the development machine and verification status. A named CPU/GPU reference and detailed rendering budgets must be settled before M3 performance evaluation.
