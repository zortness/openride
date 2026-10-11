# Initial repository audit — 2026-10-11

Prepared for the owner's requested initial GitHub push. No remote was configured
and the main branch had no commits. This agent session could not stage files:
creating .git/index.lock returned Permission denied because Git metadata is
read-only in this environment. No commit or push was performed.

## Retained source

Rust crates/lockfile, Godot source and resource import configuration, documentation,
MIT license, reusable PowerShell/Python authoring and diagnostic tools, synthetic
tests and fixtures. See the tool inventory for production versus experimental tools.
Godot .gd.uid and SVG .import metadata are kept; .godot caches are excluded.

## Excluded artifacts

Personal imports/exports, local captures and map acquisitions, dist/target/native
binaries, generated rider scenery/editor copies, Python bytecode/environments,
ANT network-key files and local export/environment credentials. No local files
were deleted. Generated editor/scenery source is restored by the build sync step.

The candidate-file scan found no executable/DLL/PCK, FIT/TCX/GPX, PNG, ZIP or
JSONL artifacts. A literal scan against the locally configured ANT key found
zero matches without printing the key. Common credential-pattern matches were
URL credential rejection checks and Windows event-subscription tokens, not secrets.
This is a targeted audit, not a guarantee against every possible secret format.

Historical progress documents retain route names, capture identifiers, sensor
identifiers and summarized physical-test results. They do not contain full private
activity files. Revisit those summaries if publishing requires stricter anonymity.

## Cleanup and verification

- Added cache/generated-editor/credential exclusions.
- Replaced personal executable defaults in six launchers with PATH-based defaults;
  explicit overrides remain available.
- Added project agent guidance, tool inventory and a proposed staged native migration.
- Rust workspace tests passed using locked offline dependencies.
- Python authoring suite: 68 tests passed.
- All PowerShell tool scripts parsed successfully.
- Windows export succeeded with the PATH-based Godot default; the known Windows
  certificate-store diagnostic was emitted.

A final staged diff must still be checked in a terminal with writable Git metadata.
The owner must supply the intended GitHub URL; inspect remote history before
pushing and do not force-push over an existing repository.
