# OpenRide

An offline cycling simulator using real geographic routes and controllable indoor trainers.

The Windows prototype supports route authoring, streamed scenery, Suito Bluetooth rides,
activity history and FIT/TCX/GPX exports. Compatibility is limited to documented tested
configurations; broader device pairing and virtual shifting are planned in M6.
Launch `dist/OpenRide.exe` from a local build, keeping its companion files together.
Mobile is deferred; original code is MIT licensed.

- [High-level project plan](docs/project-plan.md): product scope, recommended architecture, content workflow, and open decisions.
- [Milestone roadmap](docs/milestones.md): proposed delivery sequence and acceptance criteria.
- [Development guide](docs/development.md): importer, preview, native build, and verification commands.
- [M1 progress](docs/m1-progress.md): implemented work, evidence, and remaining gates.
- [M2 progress](docs/m2-progress.md): session policy, simulated recording, and remaining hardware work.
- [Activity exports](docs/activity-exports.md): local FIT/CSV exports, pause handling, recovery, and validation limits.
- [M3 progress](docs/m3-progress.md): standalone scenery preparation, RPV source findings, preview tools, and the next terrain/data steps.

See [tool inventory](tools/README.md) for build/test commands and the distinction
between production authoring tools, synthetic tests and local artifacts. See
[tooling direction](docs/tooling-roadmap.md) for the proposed Rust authoring migration
and agent workflows. Personal Garmin samples, downloaded maps, captures and generated
route packages are excluded from the repository; a fresh checkout does not include them.
