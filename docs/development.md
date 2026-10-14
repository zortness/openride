# Development and route inspection

For machine-specific Python/Godot/PowerShell/Bash paths, use the tracked example
and ignored local configuration in [build/](../build/README.md). Dot-source
`./build/Use-Environment.ps1` before the commands below, or use `build/run.ps1`
for a single command. This configures only the current process and its children.

Audience: contributors building and validating the Windows app. Start with the
[README setup](../README.md#first-time-setup-windows-x64), [M7 progress](m7-progress.md)
and [road-first authoring](road-first-authoring.md). The importer commands below
also document the older section-based workflow.

For M2 hardware diagnostics, run `./tools/probe-devices.ps1` from normal PowerShell. See [M2 progress](m2-progress.md) for the read-only BLE probe's scope, capture files, and current hardware verification limits.

Use `-AcquireControl` for the explicit FTMS acquisition diagnostic (no start or resistance command). The exported app's **Device monitor** opens a decoded `ble.jsonl` or replay JSONL and follows appended records. `--device-capture=ABSOLUTE_JSONL_PATH` opens it on startup. This display observes the capture rather than owning a BLE connection.

Use `./tools/probe-devices.ps1 -SessionCheck` for the supervised short Start/Pause diagnostic. This mode writes a synchronized `session.jsonl` and sends no grade/ERG targets. It is mutually exclusive with `-AcquireControl`. See the M2 progress document for the sequence, failure behavior, and recovery command.

Use `./tools/probe-devices.ps1 -ResistanceCheck` for the stationary simulation-command check (do not pedal), or `./tools/probe-devices.ps1 -PedalCheck` for the supervised moving test. PedalCheck announces 0%, 1%, 2%, 1%, 0% grades live with ten-second holds, then restores flat grade and requests Pause. Pedal easily in one gear at roughly steady cadence to compare the steps. Close other trainer applications first. All four control modes are mutually exclusive and save captures under `local/device-probe-*`; share the folder name and your resistance observations afterward. The agent cannot access BLE in its restricted session, so hardware checks must run from your normal PowerShell session.

The app's **Trainer session** window now supports a persistent session. **Start session** connects to the selected FTMS trainer and starts the gentle 0%, 1%, 2%, 1%, 0% profile with ten-second steps, then holds flat until paused or finished. **Pause** restores flat grade and acknowledges Pause while keeping telemetry, the connection, and recording open. **Resume** requires the current paused state and fresh power telemetry; it restarts the profile from flat. **Finish** or closing the session window requests flat/Pause, observes five seconds, and disconnects. A deliberate new Start is required after disconnect/failure. Imported route elevations are not sent to the trainer.

Persistent sessions finish automatically at 40 minutes, including paused time. An outer 45-minute limit and a 90-second worker-progress watchdog cover hangs; individual command deadlines and the three-second power freshness guard still apply. These limits support the planned 30-minute supervised test but do not establish that acceptance result. The existing CLI `-PedalCheck` remains the short bounded diagnostic.

The controller runs in the bundled `device-probe.exe`, separately from Godot. Session folders are displayed in the window and live under Godot's `user://sessions` (normally `%APPDATA%/Godot/app_userdata/OpenRide Route Inspector/sessions`). Each contains `session.jsonl`, `ble.jsonl`, and `ble-errors.txt`. The controller journal is synchronized; the diagnostic mirror is not. UI sample age measures display receipt, while the controller independently enforces power telemetry freshness. Closing the app closes its control pipe and requests cleanup; a transport failure or watchdog termination still cannot guarantee trainer state.

Hardware-free integration verification uses an explicit fixture executable, which has no BLE code:

```powershell
rustc tools/fixtures/trainer-worker.rs -o local/trainer-worker-fixture.exe
./dist/OpenRide.exe --headless -- --verify-trainer-ui --worker=ABSOLUTE_FIXTURE_EXE --capture=ABSOLUTE_NEW_CAPTURE_FOLDER
```

Use `Start-Process -Wait -PassThru` when automating the exported app. Success prints `TRAINER_SESSION_OK`; tests cover zero/missing values, requested versus acknowledged grade, duplicate-worker prevention, Pause/Resume/Finish over the persistent control pipe, Resume availability, capture output, bridge-drop EOF, and launch failure. This fixture does not validate real hardware cleanup. The editor test entry point is `res://tests/trainer_session.gd` with the same `--worker` and `--capture` arguments.

## Rust importer

For recorded trainer-session FIT/CSV exports, use the app's **Activity exports** window or the `activity-core` CLI. See [activity exports](activity-exports.md) for format semantics, recovery, and interoperability validation. The 30-minute ride and deliberate dropout tests are deferred at the user's request.

From the repository root:

```powershell
cargo test -p ride-core --offline
cargo test -p ride-core --offline -- --include-ignored --nocapture
cargo clippy -p ride-core --offline --all-targets -- -D warnings
```

The second command includes the private Garmin corpus test and requires the original local samples. Ordinary tests use synthetic inline documents. Original files are read only.

```powershell
cargo run -p ride-core --bin inspect --offline -- import import/2022-03-17_kurt_rpv.gpx import/2022-03-17_kurt_rpv.tcx
cargo run -p ride-core --bin inspect --offline -- export import/2022-03-17_kurt_rpv.tcx 100 800 local/rpv-section.orroute
cargo run -p ride-core --bin inspect --offline -- validate-route local/rpv-section.orroute
```

Create the output directory first and use a new filename for each export. Export indexes are zero based and inclusive. Use a `.gpx` suffix for a GPX track; otherwise the exporter writes the draft JSON route manifest. Import JSON includes personal measurements, so keep reports in the ignored local directory.

## Godot application

Before a live ride or supervised trainer session, open Settings → **Discover
devices / save diagnostics**, inspect your Windows-paired trainer and save its
resistance/power roles. Inspect independent Bluetooth sensors to assign power,
cadence or heart rate. Cadence is optional. Choose Bluetooth HR or ANT HR in ride
setup; enabling both blocks connection. The app connects by saved device ID and
no longer defaults to SUITO by name. See [M6 progress](m6-progress.md) for setup,
freshness behavior and current transport limits.

The application uses Godot 4.6.2 with a Rust GDExtension targeting Godot API 4.5. Fetch locked dependencies once, then builds can run offline.

```powershell
cargo fetch --locked
powershell -ExecutionPolicy Bypass -File tools/build.ps1 -Offline
godot --path apps/rider
```

Import the paired RPV TCX and GPX files. Select a source, click the route or profile to inspect a sample, and set start/end indexes (or use Start here / End here). Export to a new `.orroute` file, then use Open route to inspect it. Exported manifests reopen as read-only route previews; return to the source activity for another trim. Further imports reindex all files selected during the current session. The library is currently session-local; source files and exported routes persist.

Routes retain locations and source elevations but omit biometric measurements and timestamps. Source elevation is uncorrected and must not drive trainer control yet. This is a schematic local track view with no hosted basemap.

## Verification helpers

To exercise the visual UI without claiming native integration, generate a local JSON report with the Rust CLI and pass `--preview-json=ABSOLUTE_PATH` after Godot's `--` separator. The scene-tree test uses that report:

```powershell
godot --headless --path apps/rider --script res://tests/preview.gd -- --preview-json=ABSOLUTE_PATH
```

The native test requires the compiled extension and a new output filename:

```powershell
godot --headless --path apps/rider --script res://tests/native.gd -- --fixture=ABSOLUTE_RPV_TCX_PATH --output=ABSOLUTE_NEW_ORROUTE_PATH
```

For native visual inspection, run normally with both `--import=ABSOLUTE_TCX_PATH` and `--snapshot=ABSOLUTE_PNG_PATH`; the application waits for import, saves its rendered viewport, and exits. No route image belongs in public fixtures without permission.

## Windows export

Install the Windows export templates matching Godot 4.6.2, then build and export:

```powershell
. ./build/Use-Environment.ps1
$env:APPDATA = Join-Path (Get-Location) 'local/godot-appdata'
./tools/export-rider.ps1 -Offline
```

The export script first runs the release build with locked Cargo dependencies,
including `openride-godot`, `device-probe`, `content-builder`, and `openride-author`.
Keep the entire `dist` folder together: executable, PCK, extension DLL, device
worker, and both native tools in `authoring/`. New Ride Projects need no Python
interpreter. Python files are bundled only for the legacy section builder.
Test the actual executable with the development project closed. No private inputs
are inside the Godot project or export resources.

The exported runtime includes an opt-in RPV verification command. It imports the external TCX, exports samples 100–800, validates reopening, and exits with a nonzero status on failure. Supply absolute paths and an output filename that does not exist:

```powershell
./dist/OpenRide.exe --headless -- --verify-rpv --fixture=ABSOLUTE_RPV_TCX_PATH --output=ABSOLUTE_NEW_ORROUTE_PATH
```

Success prints `NATIVE_OK`. Windows GUI executables may return control to PowerShell before exiting; automation should use `Start-Process -Wait -PassThru` and inspect `ExitCode`. The same verification logic is used by `tests/native.gd` in the editor runtime.
## Native route CLI and legacy Python

After building, run `./apps/rider/bin/openride-author.exe --help`, or use
`./dist/authoring/openride-author.exe --help` after exporting. See
[road-first authoring](road-first-authoring.md) for road choices, recorded
connections, and the project workflow shared by the CLI and UI.

For legacy Python commands, load `build/Use-Environment.ps1` or use
`./build/run.ps1 python ...`. Configure executable overrides in the ignored
`build/environment.local.psd1`; do not hardcode a contributor's installation path.
