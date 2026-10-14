# Tool inventory

Use [build/Use-Environment.ps1](../build/README.md) for local executable overrides.
The example configuration is tracked; `build/environment.local.psd1` is ignored.

These scripts are source tooling, not captured test output. Generated files belong
in ignored local/, export/, dist/ or target/ directories.

| Category | Tools | Keep in Git? |
|---|---|---|
| Build and packaging | build.ps1, export-rider.ps1, sync-scenery-runtime.ps1 | Yes |
| Native Ride Projects | openride-author (Rust crate; bundled under dist/authoring) | Yes; [project lifecycle CLI](../docs/ride-projects.md), no interpreter dependency |
| Legacy authoring pipeline | route-workflow.py; acquire-scenery/imagery; osm-context; correct-scenery; detect-jitter; repair-profile; create-route-profile; package/chunk-scenery; building/coastal/road_scenery; png_tiles | Yes; retained for the Legacy section builder pending S22 migration. New Ride Projects use Rust for import, repair, maps, course/scenery edits, assembly and ZIP sharing |
| Regression coverage | tests/, fixtures/, verify-activity-fit.py, verify-activity-xml.py | Yes; synthetic tests, fixture generators and validators |
| Hardware diagnostics | probe-devices.ps1, probe-ant.ps1 | Yes; outputs and keys stay local |
| Developer previews and benchmarks | preview-*.ps1, benchmark-rider.ps1 | Yes; some defaults refer to unavailable private RPV outputs; supply your own paths |
| Historical reproducible experiments | acquire-osm2world.py, compare-osm2world.py, update-rpv-detail.ps1 | Yes; reference tooling, not prerequisites for an ordinary ride |
| M7 tiled section proof | tiled-scenery.py; fixtures/tiled-terrain.py | Yes; [usage and limits](../docs/m7-tiled-sections.md); private sources/packages stay under local/ |
| M7 resumable corridor jobs | corridor-scenery.py | Yes; [usage and remaining gates](../docs/m7-corridor-preparation.md); regional preparation, not yet a unified full-route package |
| M7 full-route software rehearsal | content-builder --review-audit | Yes; [usage and limits](../docs/route-rideability-audit.md); native physics and steep-grade context, no hardware |
| M7 continuous package assembly | assemble-corridor.py | Yes; [usage and bounds](../docs/continuous-corridor-packages.md); one shared terrain grid and continuous route |
| Bytecode/caches, captures, map downloads, built executables | __pycache__, local/, export/, dist/, target/ | No |

## Local verification

```powershell
cargo test --workspace --locked --offline
python -m unittest discover -s tools/tests
$env:APPDATA = Join-Path (Get-Location) 'local/godot-appdata'
./tools/export-rider.ps1 -Offline
```

Fetch locked dependencies first if the Cargo cache is empty. Install Godot 4.6.2
and its matching Windows export templates. Godot must be on PATH, or pass -Godot
to build/export/preview scripts. Python must be on PATH for legacy authoring and tests, or pass an
executable to scripts with -Python. PowerShell launchers currently target Windows.
Current export copies top-level Python utilities into dist/authoring; tests and
fixtures are not copied. Further packaging minimization is separate work.

`build.ps1` builds the Rust GDExtension and all three native executables:
`device-probe`, `content-builder`, and `openride-author`. Debug or release outputs
are copied into `apps/rider/bin`. `export-rider.ps1` always runs the release build
first, then packages the device worker and both route tools under `dist`; pass
`-Offline` to require the cached locked dependencies. New Ride Projects do not
require Python. Run `dist/authoring/openride-author.exe --help` to inspect the CLI.

The build imports Godot assets after copying native components. Import is bounded
to two minutes, and logged resource/script errors fail the build even if Godot
returns zero. Logs are in `local/build-godot.stdout.log` and
`local/build-godot.stderr.log`. `--import` exits when importing finishes.

The checkbox theme belongs to the main scene, not the project startup settings:
a project theme can load before a fresh checkout has imported its SVG textures.
Do not commit `.godot` caches to fix missing `.ctex` errors. To regress this path,
copy `apps/rider` (including built `bin`) into a new directory under `local`,
excluding `.godot`, then run the following with isolated APPDATA as above:

```powershell
godot --headless --path ABSOLUTE_FRESH_COPY --editor --import
godot --headless --path ABSOLUTE_FRESH_COPY --script res://tests/theme.gd
```

Use hidden, redirected, bounded processes for automated runs. Require zero exit
codes, no `ERROR:`/`SCRIPT ERROR:` log entries and the `THEME_OK` marker. The theme
check verifies all four custom checkbox textures through controls and windows.

Private corpus tests are explicitly ignored; ordinary tests should use synthetic
inputs. Do not publish locally generated diagnostics or activity files as fixtures.
Existing project docs contain historical route names, measurements and capture
identifiers as development evidence; the corresponding private files are excluded.
