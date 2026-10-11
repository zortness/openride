# Tool inventory

These scripts are source tooling, not captured test output. Generated files belong
in ignored local/, export/, dist/ or target/ directories.

| Category | Tools | Keep in Git? |
|---|---|---|
| Build and packaging | build.ps1, export-rider.ps1, sync-scenery-runtime.ps1 | Yes |
| Active authoring pipeline | route-workflow.py; acquire-scenery/imagery; osm-context; correct-scenery; detect-jitter; repair-profile; create-route-profile; package/chunk-scenery; building/coastal/road_scenery; png_tiles | Yes; currently used by route creation |
| Regression coverage | tests/, fixtures/, verify-activity-fit.py, verify-activity-xml.py | Yes; synthetic tests, fixture generators and validators |
| Hardware diagnostics | probe-devices.ps1, probe-ant.ps1 | Yes; outputs and keys stay local |
| Developer previews and benchmarks | preview-*.ps1, benchmark-rider.ps1 | Yes; some defaults refer to unavailable private RPV outputs; supply your own paths |
| Historical reproducible experiments | acquire-osm2world.py, compare-osm2world.py, update-rpv-detail.ps1 | Yes; reference tooling, not prerequisites for an ordinary ride |
| Bytecode/caches, captures, map downloads, built executables | __pycache__, local/, export/, dist/, target/ | No |

## Local verification

```powershell
cargo test --workspace --locked --offline
python -m unittest discover -s tools/tests
$env:APPDATA = Join-Path (Get-Location) 'local/godot-appdata'
./tools/build.ps1 -Release -Offline
./tools/export-rider.ps1
```

Fetch locked dependencies first if the Cargo cache is empty. Install Godot 4.6.2
and its matching Windows export templates. Godot must be on PATH, or pass -Godot
to build/export/preview scripts. Python must be on PATH for authoring, or pass an
executable to scripts with -Python. PowerShell launchers currently target Windows.
Current export copies top-level Python utilities into dist/authoring; tests and
fixtures are not copied. Further packaging minimization is separate work.

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
