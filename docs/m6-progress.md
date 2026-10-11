# M6 progress — discovery, pairing and Bluetooth sensors

Implementation started 2026-10-11. The [M6 brief](m6-device-compatibility.md)
defines the full milestone. Discovery, saved source roles and independent
Bluetooth sensor reception are implemented. Hardware acceptance remains open.

## Current implementation

Settings → **Discover devices / save diagnostics** lists Windows-paired BLE
devices. Pair in Windows Bluetooth settings first. Select a device and inspect
its standard GATT capabilities without requesting trainer control, subscribing
to telemetry, starting a session, or changing resistance. Duplicate device names
remain separate through Windows device IDs. Listing paired devices is not a
nearby advertisement scan and does not prove a device is currently reachable.

The existing Rust worker runs off the UI thread with its 90-second watchdog.
The app excludes concurrent ride and diagnostic workers. Diagnostic CLI modes
reject additional arguments, including all control modes. Capability results
derive from characteristic properties and FTMS/CSC feature evidence; missing
evidence stays unestablished rather than being inferred from a product name.
Unknown/vendor protocols remain inspectable in the raw service/characteristic
report. Discovery does not establish successful telemetry or resistance control.

Each operation creates a new folder under `user://diagnostics`, containing
`ble.jsonl` and `ble-errors.txt`. Inspection includes app version, platform,
architecture, names, Windows device IDs, services, characteristics, allowed
device-information reads, capability evidence, and errors. **Review diagnostic
files** opens that folder for manual review and sharing. Nothing is uploaded.
Routes, activity history, ANT keys and credentials are not collected. These
files still contain device identifiers and should stay out of public fixtures.

The shipped `device-probe.exe` also accepts `--discover` or
`--inspect-id WINDOWS_DEVICE_ID`; no development environment is needed.

## Saved trainer roles

After a successful inspection, **Use inspected trainer for resistance + power**
saves the Windows device identity for both roles. **Use its cadence** is optional;
turning it off leaves cadence missing in the live display and decoded recording.
The preference lives in `user://pairing.json` and is replaced atomically. Selecting
another list entry requires inspecting that device before assigning it.

One FTMS trainer owns resistance control. Power can come from that trainer or an
independent Bluetooth Cycling Power sensor; optional cadence can come from the
trainer, CSC crank data or Cycling Power crank data. Heart rate can come from an
independent Bluetooth Heart Rate sensor or the existing ANT selection. Manual
power-only rides and selectable input devices remain future work.

Existing users must inspect and select their trainer once: the app no longer
defaults to a device named SUITO. Missing, corrupt or unsupported pairing files
stop connection without fallback. The ride setup shows saved role names; the
worker opens the exact Windows ID and rechecks FTMS capabilities before any
control request. If cadence support disappears, reselect the sources instead of
silently substituting measurements. Pairing changes are blocked during a worker
session, and each launch receives an immutable snapshot.

The durable session journal records `selected_sources` with resistance, power,
cadence, optional Bluetooth HR and connected device identity/name before control starts. Selected ANT HR
continues to be recorded separately in `ant_hr_config` without the network key.
Raw diagnostic notification bytes remain intact; disabled cadence is removed
from the decoded samples consumed by display and activity exports.

## Independent Bluetooth sensors

Pair each sensor in Windows first. In Settings, inspect and save the FTMS trainer,
then inspect each sensor and choose **Use for power**, **Use for cadence** or
**Use for heart rate** as supported. Each assignment preserves the resistance
trainer. A multi-service sensor can supply several roles. Clear cadence or heart
rate to disable that selection. Saving the trainer again selects its power and
optional cadence, while preserving independent Bluetooth HR. Saved schema-1
trainer preferences remain readable; independent selections use schema 2.

Select Bluetooth HR OR enable ANT HR in ride setup. A conflicting selection is
rejected before the worker starts. Clear Bluetooth HR to use ANT. Sensor roles
cannot be changed during an active session, and there is no fallback to another
device when a selected reading disappears.

The Rust worker opens one receive-only connection per independent peripheral,
subscribes to standard HR/Cycling Power/CSC measurements and performs no sensor
control-point writes. Each role expires three seconds after its own reading;
aggregate updates cannot refresh older cadence or HR. Zero remains a reading,
distinct from missing. Missing selected power pauses the ride; restored telemetry
does not automatically resume it. Disconnection requires a new connection.
Crank counters handle rollover, stationary notifications and rebasing after gaps.
Wheel data does not replace virtual route speed. Unsupported HR values above
254 bpm produce a diagnostic and missing data rather than being clipped to fit
the current export range.

The journal retains sensor identity, protocol, raw notification bytes, errors and
selected measurement timestamps. Activity exports use the selected samples and
recheck individual freshness; reports retain source selection metadata. Decoder
references: Bluetooth SIG [Heart Rate Service](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/HRS_v1.0/out/en/index-en.html),
[Cycling Speed and Cadence Service](https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/CSCS_v1.0/out/en/index-en.html)
and [GATT Specification Supplement](https://btprodspecificationrefs.blob.core.windows.net/gatt-specification-supplement/GATT_Specification_Supplement.pdf).

## Verification

Verified 2026-10-11: `cargo test --workspace --locked --offline` passed with
hardware/private tests still ignored; all 68 Python tests passed. Release build
and Windows export exited 0. The exported app produced `DEVICE_DISCOVERY_OK`
and `TRAINER_SESSION_OK`, both with exit code 0, using synthetic workers,
isolated APPDATA, hidden processes and bounded waits. No physical BLE inspection
or trainer commands were run for this change.

Saved-role slice verification (2026-10-11): the same workspace/Python suites,
release build and Windows export passed again. Exported-app checks produced
`DEVICE_DISCOVERY_OK`, `TRAINER_SESSION_OK`, `UX_RIDE_OK` and `UX_OK`, all with
exit code 0. Coverage includes persistence across bridge instances, duplicate
names/exact identity, invalid choices preserving the previous selection,
missing/corrupt preferences stopping before worker launch, immutable active
source snapshots, disabled cadence retaining zero versus missing power, and
route connect/resume/pause/finish using a fake worker. Physical selection and
resistance behavior remain unverified.

The integrated test exposed a finish/navigation race: hiding the ride could
disable its polling before it consumed the worker's final events. The shell now
drains those events before opening Activities. The regression explicitly suspends
ride-screen processing during finish and verifies completion still reaches the UI.

Hardware-free checks cover capability evidence, refusal to mix diagnostics and
control, duplicate names, exact selected identity, worker contention, capture
files, worker failure, and launch failure. Exported-app verification uses explicit
failure exits rather than release assertions:

```powershell
rustc tools/fixtures/discovery-worker.rs -o local/discovery-worker-fixture.exe
# Set isolated APPDATA under local/godot-appdata before automated Godot runs.
# Launch hidden with redirected logs and a bounded wait; inspect exit status.
./dist/OpenRide.exe --headless -- --verify-discovery --worker=ABSOLUTE_FIXTURE_EXE --capture=ABSOLUTE_NEW_CAPTURE_FOLDER
```

Expected success marker: `DEVICE_DISCOVERY_OK`. The fixture has no hardware I/O.

Bluetooth sensor slice verification (2026-10-11): workspace tests and all 68 Python
tests passed; release build and Windows export exited 0. Exported checks returned
`DEVICE_DISCOVERY_OK`, `TRAINER_SESSION_OK`, `UX_RIDE_OK` and `UX_OK` with exit 0.
Protocol tests cover optional fields, truncation, zero/signed power, HR contact,
counter rollover, stationary cadence, gaps and invalid resets. Integration tests
cover exact source identity, independent freshness, power dropout without fallback
or automatic resume, selected export samples, saved sensor assignments, BLE/ANT HR
conflict rejection and live display ignoring unselected raw trainer values.
These are synthetic/fake-worker checks, not physical sensor validation.

## Initial shifting and ride actions

Keyboard/gamepad ride actions now reach the same worker that owns terrain
control. This first backend is a bounded grade-offset prototype, not native
trainer gearing or a model of transmission ratios. It is intended for supervised
evaluation on the Suito in a fixed physical gear; physical shift feel is unverified.

| Action | Keyboard | Gamepad |
|---|---|---|
| Easier / harder | Q / E | Left / right shoulder |
| Pause / explicit resume | P | North button (Y) |
| Ride menu | Escape | Start |
| Confirm / back | Enter / Escape (Backspace during ride) | South / east button |
| Reserved left / straight / right choice | J / K / L | D-pad left / up / right |

Direction actions report that the current fixed route has no selectable junctions;
they do not alter the route. Existing focus, editor and file-dialog controls retain
their contexts. Shifts are blocked by menus/dialogs, text entry, paused/finishing
state and outstanding shift responses. Button holds do not repeat shifts; screen
changes, loss of window focus and controller disconnect clear held input state.
Keyboard remains usable after gamepad disconnect. Controller selection/remapping,
Click and OpenBikeControl adapters remain future work.

Each connection begins in gear 5 of 9. Each step offsets the already bounded
terrain request by 0.25 percentage points; the result remains within 0–2%.
At either load boundary, different gears can request the same load. Gear selection
persists through pause/resume within the session; pause still restores flat load,
and resuming starts flat before the existing paced scheduler applies the target.
The existing one-second acknowledgement spacing and maximum one-percentage-point
step are retained. Shifts require the current running-state token and fresh power;
a queued pause or finish wins. Missing power never triggers automatic resume.

The display distinguishes selected gear, load target and acknowledged load. The
journal records selections and target commands; route geography, course grade,
measured power and simulation physics are not changed by shifting. No separate
command writer or native gear protocol is introduced.

Verification (2026-10-11): Rust workspace tests and all 68 Python tests passed.
Release build and Windows export exited 0. Exported fake-worker checks returned
`UX_RIDE_OK`, `UX_OK`, `DEVICE_DISCOVERY_OK` and `TRAINER_SESSION_OK` with exit 0
and no error logs. New checks cover bounded gears and acknowledgement pacing,
freshness/state-token guards, stop precedence, keyboard/shoulder shifting,
held-button suppression, menu/dialog isolation, direction-unavailable feedback
and explicit pause/resume. The UI fixture retains its power and course-grade
values across shifts. No physical trainer commands were sent.

## Next slices and open gates

- Validate independent Bluetooth HR, Cycling Power and CSC sensors physically,
  including simultaneous trainer reception, dropout/reconnection and exports.
- Add nearby unpaired discovery and selectable controller input roles.
- Physically evaluate the bounded shifting prototype on Suito; establish the gear
  model/range and CORE/Cog backend before claiming usable single-cog shifting.
- Complete the friend package with a sample route and setup guide, controller
  adapters and per-model physical testing as ordered in the M6 brief.
- No friend trainer or independent Bluetooth sensor has been physically verified.
  Neither M6 readiness nor compatibility acceptance is complete. M2/M5 endurance,
  dropout, cadence, and simultaneous trainer/ANT-HR gates remain deferred/open.
