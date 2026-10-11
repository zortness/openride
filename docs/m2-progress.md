# M2: trainer session implementation

Status: in progress. Normal-session Windows BLE probes verified SUITO service discovery, telemetry, control acquisition, and an acknowledged Start/Pause check. The full bounded pedaling grade profile completed, and the rider confirmed resistance increased and eased with the steps. No target-power commands have been sent. The user deferred the 30-minute ride and deliberate dropout tests on 2026-10-09. FIT/CSV exports are available, and the user confirmed successful FIT import and power display in their analysis application. External summary/pause checks and independent decoder validation remain pending.

FTMS packet decoding and a transport-independent acknowledged-command state machine are implemented and tested. The BLE probe emits decoded measurements and can replay saved captures without hardware. The bounded Godot session UI has successful real SUITO captures for both the full profile and an early user-requested Pause. The persistent connection revision now also has a successful real SUITO Start/Pause/Resume/Finish capture. Longer-session and deliberate fault validation remain pending.

## Implemented

`trainer-core` is independent of Godot and the activity importer. Its deterministic session accepts monotonic elapsed timestamps and explicit events. It emits requests that a future transport adapter must acknowledge separately.

- Resume requires acknowledged control and telemetry received within three seconds.
- Disconnect clears control and telemetry freshness. Reconnection does not resume automatically.
- A periodic tick pauses a running session after three seconds without telemetry. New telemetry does not resume it.
- Synthetic grade requests are limited to ±5%, changing by at most one percentage point per second, with at most one point allowed after a long scheduling gap. These are provisional test settings, not a claim about safe or supported Suito limits.
- Missing sensor measurements remain optional; zero power is retained as zero.
- JSONL journals use exclusive creation and synchronize every appended record. Recovery accepts complete lines and reports a discarded incomplete final line; malformed complete records fail.
- CSV exports preserve missing measurements as empty fields.

Run the simulated scenario and recover its telemetry (choose new output filenames):

```powershell
cargo run -p trainer-core --offline --bin session -- demo local/m2-simulated.jsonl
cargo run -p trainer-core --offline --bin session -- recover local/m2-simulated.jsonl local/m2-simulated.csv
cargo test -p trainer-core --offline
```

The scenario uses a virtual clock and runs immediately. It exercises a grade profile, missing and zero power, telemetry timeout, disconnect, and reconnect. It neither discovers nor controls equipment. Journal commands record intended requests, not successful hardware execution. CSV is an inspection format; it is not yet a complete activity export with wall-clock timestamps or FIT metadata.

## Next slices

1. Read-only Windows BLE inventory and notification capture are verified for the SUITO. Decode its captured feature bits and measurements before selecting supported commands.
2. Implement FTMS decoding and a serialized command/response transport with timeouts, explicit control loss, and trainer ownership. Add a dedicated worker so rendering cannot stall transport. Journal failures must stop dispatch rather than silently lose recording.
3. Connect the Godot session display to that worker; show connection, telemetry age, requested versus acknowledged commands, pause/resume, and source selection.
4. Add activity timestamps, recording lifecycle/recovery UI, and FIT export; validate with GoldenCheetah and an independent decoder.
5. Perform the supervised 30-minute Suito test and fault scenarios from the milestone acceptance criteria.

The session policy alone does not enforce ownership across multiple sessions/processes, detect adapter failures, or establish trainer stop behavior. Those remain transport and hardware gates. Recovery tests cover a truncated write, not a power-loss durability guarantee for every filesystem.

The user's `export/rpv.orroute` is the full 2012 Culver-to-RPV GPX route (2,086 points, 68.611 km). Keep it as an additional content test; M2 initially uses synthetic grades rather than uncorrected activity elevation.

## Windows device probe (2026-10-08)

PnP reports the SUITO, a MediaTek Bluetooth adapter (`0E8D:7925`), and the ANT USB-m (`0FCF:1009`) as Started. The ANT dongle uses the installed Dynastream/libusb-win32 driver; the system has both 32-bit and 64-bit libusb0 DLLs. This confirms OS detection, not ANT packet communication. Cadence/speed hardware testing is deferred until the bike is available.

The new `device-probe` executable uses the MIT-compatible Microsoft `windows` crate. It enumerates paired BLE devices, selects an exact unique device name, discovers uncached GATT services, reads standard feature/range and firmware/model characteristics, and subscribes to Indoor Bike Data for ten seconds. It emits JSON lines containing raw bytes. It does not acquire control or write the FTMS control point. Enabling/disabling notifications writes only the notification configuration descriptor. A 90-second process watchdog bounds hung Windows calls. This is a diagnostic capture, not a durable session recorder or decoded telemetry feed.

Implementation references: [Microsoft GATT client sample](https://github.com/microsoft/Windows-universal-samples/blob/main/Samples/BluetoothLE/cppwinrt/Scenario2_Client.cpp) and [Bluetooth assigned numbers](https://www.bluetooth.com/specifications/assigned-numbers/). Feature and telemetry interpretation remains pending captured packets and protocol tests.

Run from a normal PowerShell session at the repository root:

```powershell
./tools/probe-devices.ps1
```

Each run creates a fresh ignored `local/device-probe-*` folder with Windows inventory, BLE JSONL, and errors. A name other than SUITO can be passed with `-DeviceName`; `-Address` accepts a known Bluetooth address for direct diagnostics. Keep other trainer apps closed during connection testing.

Verification: the probe builds offline and passes Clippy with warnings denied. In the agent environment, paired-device enumeration returns `0x80070002` and direct-address access returns a null device without a useful HRESULT. Neither attempt reaches GATT discovery. The cause is not yet established; a normal-session run is needed before claiming firmware, FTMS availability, or received telemetry. The script itself was exercised and preserves this failure with a nonzero result.

### Successful user-session capture

Inspected `local/device-probe-20261008-200355-0cfb3255` on 2026-10-08. The error log is empty, capture completion reports ten notifications, and both subscription and unsubscribe return Success (status 0).

- Manufacturer: `Elite`; hardware revision: `005`; firmware and software revision strings: `191`.
- Fitness Machine service, Indoor Bike Data, and a writable/indicating Fitness Machine Control Point are present. Cycling Power and Cycling Speed and Cadence services are also enumerated; those notification streams have not been tested.
- Feature bytes: `86 50 00 00 0C E0 00 00`.
- Supported resistance range bytes: `00 00 C8 00 01 00`; supported power range bytes: `00 00 A0 0F 01 00`. Preserve these raw values until protocol decoding is implemented.
- Ten identical Indoor Bike Data payloads: `54 08 00 00 00 00 01 00 00 00 00 00 00`.
- `control_requested` is false. The initial connection-state snapshot preceded service discovery and does not negate the subsequent successful GATT reads/notifications.

The successful normal-session run resolves the device-reachability question. The earlier agent-session API failures remain an environment-specific diagnostic limitation, with their exact cause unknown. Next: implement and test the FTMS decoder against these packets, then build the acknowledged command transport. Active trainer behavior and external cadence remain unverified.

### FTMS decoding and command procedures

`trainer-core::ftms` decodes feature masks and selected Indoor Bike Data fields. Replay of all ten captured packets produces 0 W, 0 rpm, 0 km/h, elapsed time 0 s, a reported distance of 1 m, and absent heart rate. The distance is preserved as reported, not interpreted as actual motion. Feature bits advertise cadence, power measurement, target power, and indoor-bike simulation.

```powershell
cargo run -p device-probe --offline -- --replay local/device-probe-20261008-200355-0cfb3255/ble.jsonl
cargo test -p trainer-core --offline
```

The decoder retains signed power and distinguishes absent fields from zero. It validates packet lengths and reserved flags. Each notification remains separate; multipart assembly and per-field freshness are not yet implemented. Average/energy/remaining-time fields are checked for length but not exposed. Resistance-level fields are explicitly rejected because their deployed layouts require verification; raw bytes remain available. No unsigned conversion of negative power into session records has been introduced.

The command state machine encodes acquire-control, start/resume, pause, and simulation requests. It allows one pending procedure and requires both a successful GATT write and the matching successful FTMS indication before accepting a command. Rejections clear control. Disconnect, reset, control-permission loss, mismatched replies, write failure, or a five-second timeout invalidate the connection. After invalidation, create a new instance only for a fresh physical connection; no automatic retry is performed because FTMS replies have no transaction IDs. The five-second timeout is an initial application policy, not a protocol compliance claim.

The future transport must enable indications before `begin`, call `tick` regularly, forward status/control-loss events, isolate callbacks by connection, and enforce one owner per device. The wire encoder accepts protocol-unit integers; the session layer must apply grade/rate limits before calling it. It is not yet wired to hardware writes or the existing session policy, so these tests do not establish actual trainer control behavior.

Protocol references: [Fitness Machine Service 1.0](https://www.bluetooth.org/DocMan/handlers/DownloadDoc.ashx?doc_id=423422), sections 4.3, 4.9, 4.16 and 4.17, and the [GATT Specification Supplement](https://btprodspecificationrefs.blob.core.windows.net/gatt-specification-supplement/GATT_Specification_Supplement.pdf). Original code is MIT; specification documents are not redistributed.

### Control acquisition diagnostic and capture monitor

```powershell
./tools/probe-devices.ps1 -AcquireControl
```

This explicit mode captures ten seconds of telemetry, enables FTMS control indications and status notifications, writes Request Control (opcode 0), and waits for both GATT write completion and a matching successful FTMS response. It sends no start, grade, power, or pause command. The connection is closed after the diagnostic; notification cleanup is best effort with bounded waits. The default probe remains read-only apart from notification configuration.

A named Windows mutex prevents competing OpenRide control probes for the same Bluetooth address in the same login session. This does not lock out other applications or other Windows sessions. Control callback queue errors/overflow fail the diagnostic; disconnect and status reset/control loss invalidate pending work. A five-second procedure deadline and a 90-second process watchdog bound failure paths. Unexpected OS stalls can force process termination before graceful cleanup.

The acquisition path builds and passes lint, and has received a successful hardware acknowledgement in the capture below. Its state-machine tests and mutex test pass. Capture results include `control_request`, `control_write_complete`, `control_indication`, `control_acquired` or `probe_error`, and `device_closed`. Only `control_acquired` establishes a successful acknowledgement; discovering the control characteristic does not.

The exported `dist/OpenRide.exe` now has a **Device monitor** button. It tails a selected decoded probe/replay JSONL file, preserving zero versus missing sensor values and displaying recorded command outcomes. It is a capture viewer, not an active session controller; the age shown is time since the viewer observed a file update, not sensor freshness. A running external probe can update it without blocking the renderer. Existing raw-only captures can be replayed into decoded JSONL first; `local/suito-decoded.jsonl` contains the successful SUITO capture in that format.

The monitor was rendered and visually checked in the exported Windows executable. Headless checks cover zero/missing values and acknowledgement display. UI control of a continuous BLE worker, actual grade changes, durable command journaling, and supervised ride testing remain future work within M2.

### Successful acquisition capture (2026-10-08)

Inspected `local/device-probe-20261008-202055-dd9beccc`: ten decoded telemetry packets, an empty error log, Request Control `[00]`, successful response `[80 00 01]`, successful GATT write completion, `control_acquired`, successful device-handle closure, and `probe_complete`. The indication arrived before write completion, exercising the tested out-of-order acknowledgement path on real hardware.

The trainer also emitted Machine Status `[04]` during acquisition. This is the protocol's started/resumed-by-user status ([Bluetooth SIG FTMS test suite](https://files.bluetooth.com/wp-content/uploads/dlm_uploads/2024/10/FTMS.TS_.p6.pdf)). The probe sent no Start command; the capture does not establish why the trainer emitted this status or whether it describes a new transition. Keep requested, acknowledged, and device-reported session states distinct in the continuous controller. Do not infer physical behavior or an active OpenRide session from acquisition alone.

The ten preceding packets report zero power/cadence/speed, elapsed time zero, absent HR, and distance 15 m. Preserve this device counter as reported; do not use its absolute value as OpenRide session distance. Next: continuous telemetry/status processing and explicit start/pause handling before supervised grade testing.

### Bounded session check (2026-10-09)

```powershell
./tools/probe-devices.ps1 -SessionCheck
```

This is an explicit hardware operation: after discovery and the initial telemetry capture, it subscribes to telemetry, status, and control responses together; acquires control; waits for fresh power telemetry; sends Start; observes for 15 seconds after acknowledgement; sends Pause; and observes for another five seconds. It sends no grade or ERG targets and does not reset existing trainer settings. Stay near the powered trainer for this test; pedaling is not required because a reported zero power value is valid telemetry. Close competing trainer applications.

Notification receipt timestamps are captured at the callback, so queued old data cannot refresh freshness merely by being processed later. Three seconds without a power-bearing notification triggers an early Pause if control is still valid. Missing power does not refresh freshness. A reported stop/safety-stop latches an interruption; neither fresh telemetry nor a started/resumed status automatically resumes the app. Unexpected stop before Start prevents dispatch. A scheduled Pause, early Pause, and failure remain distinct outcomes.

Disconnect, control loss, queue overflow, malformed packets, or a command timeout terminate the check. A failed or timed-out procedure is not retried. Closing a connection is not proof that the trainer paused; failures explicitly report trainer state as unconfirmed. The 90-second process watchdog is the outer bound. This short diagnostic is not yet the 30-minute M2 acceptance session.

Each run adds `session.jsonl` to its capture folder. It contains a UTC start timestamp, monotonic elapsed timestamps, sequence numbers, raw and decoded telemetry, status reports, requested commands, GATT write completions, and trainer acknowledgements. Every row is synchronized to disk; command intent must be saved before the corresponding hardware write. If recording fails, further dispatch stops and the connection closes. A committed intent without acknowledgement must not be treated as success. File synchronization can stall this diagnostic worker; it is independent of Godot but not yet a hard real-time controller.

Recover complete rows from a damaged/torn capture without changing the source:

```powershell
cargo run -p device-probe --offline -- --recover-session PATH_TO_SESSION_JSONL
```

Recovery validates schema, sequence and elapsed order, reports a discarded incomplete final line, and rejects malformed complete records. This command journal has its own schema and is separate from the earlier simulated `trainer-core` session journal. FIT export and schema consolidation remain pending.

Verification: 20 tests pass across `trainer-core` and `device-probe`; Clippy passes with warnings denied. Tests include stale/missing telemetry, unsolicited started status, latched stops, command ordering/failures, mutex ownership, synchronized records, and torn-tail recovery. The Godot monitor's acknowledgement/failure checks pass, the PowerShell script parses, and the Windows export succeeds. The session check subsequently passed on the real SUITO, as recorded below. The monitor can tail either the resulting `ble.jsonl` or `session.jsonl` to display recorded outcomes.

### Successful Start/Pause capture (2026-10-09)

Inspected `local/device-probe-20261009-180635-0e6bcae9`. The error log is empty. Control, Start, and Pause all received successful FTMS indications and GATT completions. Start was acknowledged at 391 ms; the scheduled Pause was requested at 15,402 ms and acknowledged at 15,472 ms. The check completed at 20,476 ms, followed by successful device-handle closure and probe completion.

The journal contains 39 valid sequential records, including 21 telemetry notifications; recovery reports no discarded partial line. The maximum callback receipt gap is 1,013 ms, below the three-second stale threshold. The first sample reports 10 W, 3 rpm and 5.59 km/h; subsequent values settle to zero. These are trainer measurements, not independently verified external cadence/speed readings. HR remains absent. The trainer elapsed-time value remains at 15 seconds throughout the five-second post-Pause observation.

Device behavior to retain in integration: after receiving Pause `[08 02]`, the SUITO emits status `[02 01]` (stopped), rather than `[02 02]` (paused), while acknowledging the Pause procedure successfully. Preserve both the requested/acknowledged command and the device-reported status. Do not infer a resumable paused session solely from that acknowledgement. No resistance target was sent, and this run does not validate pedaling load, reconnect behavior, or the longer M2 acceptance criteria.

### Stationary resistance-command diagnostic

The user requested resistance-command testing with the bike attached and before pedaling. Run:

```powershell
./tools/probe-devices.ps1 -ResistanceCheck
```

This explicitly changes simulation resistance parameters. It first acquires control and verifies fresh power and speed telemetry with trainer speed at or below 0.5 km/h. It requires advertised simulation support, sets a flat target before Start, then exercises 0%, 1%, 2%, 1%, 0% grade with two-second holds after acknowledgements. Wind is 0 m/s, rolling coefficient 0.004, and wind coefficient 0.51 kg/m. It restores/acknowledges flat grade once more, requests Pause, and observes five seconds. Flat grade is not a claim of zero mechanical resistance. No ERG/target-power commands are used.

Positive targets require fresh telemetry after the command intent is synchronized as well as before dispatch. Stale telemetry, reported stops, or motion interrupt the profile; when the control connection remains valid, the interrupted path restores flat grade and requests Pause. Transport/recording failures terminate without retries and leave trainer state explicitly unconfirmed. Do not interpret a failed check as a guaranteed reset; flat restoration is established only by its acknowledgement record.

All commands, response bytes, acknowledged grade steps, flat restoration, and Pause outcomes are recorded in `session.jsonl`. This test can establish protocol acceptance while stationary. Mechanical resistance and load response still require later pedaling measurements.

Verification: 21 Rust tests and Clippy pass, including the bounded profile and exact simulation encoding. An agent-side attempt (`local/device-probe-20261009-181443-ddaa180e`) failed during BLE enumeration with `0x80070002`, before any command was sent. The user must run this diagnostic from the normal Windows session; the subsequent user-session result is recorded below.

### Resistance capture: accepted commands, interrupted stationary profile

Inspected `local/device-probe-20261009-190440-bd7c0ea9`. The SUITO acknowledged all five simulation writes: initial 0%, 1%, 2%, 1%, and cleanup 0%. Every simulation response was `[80 11 01]`; accompanying parameter-change status payloads match the written values. Start and Pause were also acknowledged. This establishes command acceptance, not measured mechanical load.

At 8,104 ms, reported speed changed from zero to 3.23 km/h (received at 8,098 ms), exceeding the 0.5 km/h stationary threshold. Power was 18 W and cadence remained zero at that point. The diagnostic interrupted the descending 1% hold, sent cleanup 0% at 8,110 ms, received its acknowledgement at 8,159 ms, and received Pause acknowledgement at 8,222 ms. Thus the last flat target came from the interruption cleanup, rather than completion of the full scheduled profile. The process correctly returned a nonzero result for `stationary_check_failed`.

Reported speed continued to increase during the post-Pause observation, reaching 8.42 km/h with 19 W and 4 rpm in the last sample. The user subsequently confirmed moving the pedals during this test, explaining the motion-triggered interruption. The stationary threshold remains appropriate for the stationary mode. Pause acknowledgement is not proof of mechanical braking or zero wheel speed.

The journal recovers all 63 records without a partial tail. Device-handle closure succeeded. This is a successful resistance-command exchange and cleanup, but an interrupted stationary test; it is not a complete stationary-profile pass or a load-response validation.

### Supervised pedaling diagnostic

Run `./tools/probe-devices.ps1 -PedalCheck` from normal PowerShell. This separate mode allows wheel movement and uses the same 0%, 1%, 2%, 1%, 0% profile with ten seconds per acknowledged step (50 seconds total), followed by acknowledged flat restoration, Pause, and five seconds of observation. Initial discovery and read-only telemetry capture add setup time. The script announces acknowledged grades live. Pedal comfortably in one gear at roughly steady cadence and report whether resistance rises and falls with the steps.

Wind remains 0 m/s, rolling coefficient 0.004, and wind coefficient 0.51 kg/m; no ERG targets are sent. Fresh power telemetry, latched device stops, procedure deadlines, synchronized command intent, and cleanup rules remain enforced. The 15-second Start/Pause timer does not truncate this profile; the pedaling process watchdog is 180 seconds. A stale-power or reported-stop interruption attempts flat restoration and Pause while control remains valid. Transport/recording failure still leaves trainer state unconfirmed.

The stationary diagnostic retains its motion gate and two-second holds. Pedaling hardware validation is pending the next user capture and subjective load report; acknowledgement alone does not establish physical resistance or power accuracy. M2's longer ride, fault checks, full exports, and app integration remain pending.

Verification: 22 Rust tests pass across trainer-core and device-probe; formatting, Clippy with warnings denied, and the offline probe build pass. PowerShell checks cover syntax, buffering an incomplete JSONL record, emitting the completed grade announcement once, and preserving the child process exit status using an offline journal recovery run. No hardware commands were issued for these checks.

### Completed pedaling profile (2026-10-09)

Inspected `local/device-probe-20261009-191212-caa1dcdc`. All five ten-second grade steps completed in order: 0%, 1%, 2%, 1%, 0%. All six simulation writes (including the final extra flat restoration), control acquisition, Start, and Pause received successful responses and GATT write completions. Flat restoration was acknowledged at 51,314 ms and Pause at 51,379 ms. The scheduled check completed at 56,381 ms, followed by successful device closure and probe completion. The error log is empty.

The journal recovers all 111 records with no discarded partial line. Its 57 telemetry samples have a maximum callback receipt gap of 1,035 ms, below the three-second timeout. No stale-telemetry or motion interruption occurred. The SUITO again reported stopped status `[02 01]` after Pause, with its elapsed-time counter held at 75 seconds during post-Pause observation; ongoing speed/cadence values do not indicate mechanical braking.

Sample means grouped by callback receipt time between grade-step acknowledgement records (last interval ends at the cleanup request):

| Grade step | Power (W) | Cadence (rpm) | Speed (km/h) |
| --- | ---: | ---: | ---: |
| 0% initial | 30.7 | 35.3 | 11.09 |
| 1% ascending | 39.0 | 39.5 | 11.60 |
| 2% | 62.5 | 42.9 | 12.07 |
| 1% descending | 55.6 | 46.7 | 13.39 |
| 0% final | 39.7 | 44.6 | 13.42 |

The power trend is consistent with a load response, but cadence and speed changed and the short windows include transition lag. These are trainer-reported measurements, not an independent calibration or proof of grade accuracy. The rider subsequently confirmed feeling resistance increase toward 2% and ease afterward. This establishes perceived physical resistance response alongside the successful command/telemetry diagnostic; it does not complete M2's longer ride, fault checks, export, or application-integration gates.

### Godot bounded trainer session

The app now includes a **Trainer session** window with explicit Start, Pause and finish, trainer-reported power/cadence/speed/HR, requested versus acknowledged grade, last displayed sample age, and recording location. Start launches the verified 50-second pedaling profile against the paired SUITO. Pause ends this bounded session; it does not retain a resumable connection. Device/source selection, free-running sessions, and Resume remain later work.

The Rust extension launches a hidden `device-probe.exe` child with an owned stdin control pipe and a dedicated stdout reader. The existing BLE worker retains ownership, synchronized journal, serialized commands, freshness checks, and watchdog. Godot polls a bounded display queue; rendering stalls do not block the BLE worker. Overflow is reported and requests Pause when consumed, while the authoritative journal remains in the worker. The UI's age label describes display updates, not callback freshness.

Pause sends a one-shot pipe message. Window closure requests Pause; bridge destruction/app exit closes the pipe. The worker latches either request, finishes any pending serialized procedure, restores flat grade, and requests Pause while control remains valid. Cancellation before Start prevents starting; transport or journal faults can still leave state unconfirmed. Journal stdout mirroring now tolerates a closed display pipe instead of panicking before cleanup. The 180-second process watchdog remains the outer bound.

Verification: 22 Rust tests and Clippy pass. The rendered window was inspected, and the actual Windows export prints `TRAINER_SESSION_OK` with the hardware-free worker fixture. Checks cover zero/missing telemetry, requested/acknowledged grade distinction, duplicate-worker rejection, Pause stdin delivery, stdout display and capture, EOF on bridge destruction, and executable launch failure. The exported bundle includes `device-probe.exe`. These tests establish the process/UI path, not real trainer Pause-on-close behavior. The subsequent real trainer captures below establish full-profile and early user-pause behavior. M2 remains in progress.

### Successful app sessions (2026-10-09)

Inspected two sessions in the user's Godot `app_userdata/OpenRide Route Inspector/sessions` directory without changing the source files:

| Session | Outcome | Flat acknowledgement | Pause acknowledgement | Completion | Samples / largest receipt gap | Recovered records |
| --- | --- | ---: | ---: | ---: | --- | ---: |
| `session-1791599965-331-6016560` | Full 0%, 1%, 2%, 1%, 0% profile; scheduled Pause | 51,152 ms | 51,222 ms | 56,231 ms | 57 / 1,012 ms | 111 |
| `session-1791600078-893-119577544` | User Pause during ascending 1% hold | 14,317 ms | 14,368 ms | 19,379 ms | 20 / 1,013 ms | 56 |

Both captures have empty error logs, successful command responses/write completions, five seconds of post-Pause observation, successful device closure, and probe completion. Recovery reports no partial tail in either journal. The second session records `user_pause`, restores flat, and acknowledges Pause without dispatching the next 2% step. The observed cleanup request-to-Pause-ack interval is 92 ms; this is not button-to-ack latency because the UI click time is not journaled.

This validates the bounded app-launched profile and early user-pause cleanup on the SUITO. The current log uses the same `user_pause` reason for the button, window closure, and control-pipe EOF, so it cannot distinguish which triggered cancellation. App-crash/close cleanup, deliberate BLE disconnect, sensor dropout, Resume, and the longer acceptance ride remain separate checks. Next implementation slice: a persistent session lifecycle with explicit Pause/Resume before the 30-minute/fault test, followed by activity export validation.

### Persistent session lifecycle

The app now launches `--live-session` instead of the bounded diagnostic. Start acquires control and acknowledges flat grade and Start. It performs 0%, 1%, 2%, 1%, 0% with ten-second steps, then holds flat. Pause acknowledges flat restoration and Pause but retains the BLE connection, telemetry subscriptions, and synchronized recording. Resume explicitly clears the previous stop latch only with fresh power, acknowledges flat and Start, and restarts the profile. Resume requests must carry the current pause epoch; duplicates or requests from an earlier pause cannot silently resume a later pause. Device stops and stale power pause the session; fresh data alone never resumes it.

Finish, window closure, or pipe EOF requests flat/Pause and a five-second observation before disconnect. Finish has priority over queued Resume commands. Input is bounded to 16 queued commands; overflow or invalid input requests Finish. Journals distinguish `user_finish`, `control_pipe_closed`, stale-power/device-stop pauses, and explicit Resume. A state heartbeat every second records running/paused/finished state, pause epoch, and Resume availability. The app exposes separate Pause, Resume, and Finish buttons and does not advertise Resume until the controller reports an acknowledged paused state with fresh power.

The worker finishes at 40 minutes including paused time; a 45-minute process cap and a 90-second progress watchdog bound exceptional stalls. Per-command deadlines and the three-second telemetry guard remain unchanged. Disconnect or a transport fault closes the session without retry or automatic reconnection. The original CLI diagnostics retain their previous short durations.

Verification: 26 Rust tests and Clippy pass, including current/stale pause epochs, fresh-power requirements, explicit stop-latch clearing, Finish/EOF priority, and queue overflow. The hardware-free process test passes a complete Start/Pause/Resume/Finish sequence, verifies the worker remains alive during Pause, checks disabled Resume without fresh-power availability, and retains launch-failure/EOF coverage. The rendered window was inspected. These tests do not establish the SUITO's actual Resume behavior. Next hardware check is a short Start, Pause, explicit Resume, then Finish in one session journal before attempting the 30-minute acceptance ride.

The verified Windows bundle is staged at `dist/persistent-session/OpenRide.exe` because the previous `dist/OpenRide.exe` was still running. Its actual executable reports `TRAINER_SESSION_OK`; the bundled device-probe hash matches the release build. Close the older app before testing the new bundle. Session recordings use the same Godot user-data directory as before.

### Successful persistent Pause/Resume capture (2026-10-09)

Inspected `session-1791600730-87-8888051` in the user's Godot session directory, without modifying its files. The session completed Start, user Pause, explicit Resume with pause epoch 1, and user Finish on one connection and in one journal.

- Initial Start acknowledged at 952 ms; 1% grade acknowledged at 11,010 ms.
- User Pause received at 12,344 ms; flat acknowledged at 12,384 ms and Pause at 12,455 ms. Paused state published with epoch 1 and Resume available. Telemetry continued during the pause.
- Explicit Resume received at 18,560 ms with epoch 1; flat acknowledged at 18,620 ms and Start at 18,659 ms. Running state followed at 18,660 ms, about 6.2 seconds after Paused. There was no automatic resume or new connection. The restarted profile subsequently acknowledged 1% and 2%.
- User Finish received at 44,229 ms during the 2% hold; flat acknowledged at 44,312 ms and Pause at 44,362 ms. Five seconds of observation followed, with completion at 49,366 ms and successful device closure/probe completion.

The error log is empty. All 166 journal records recover with no partial tail. Its 50 telemetry notifications have a maximum callback receipt gap of 1,013 ms, below the three-second timeout; reported power ranges from 0 to 88 W. The SUITO again reports stopped status for Pause and started status on Resume, while acknowledging the requested procedures. This validates the short persistent lifecycle on hardware; it does not establish 30-minute continuity, disconnect recovery, dropout handling, or activity-export interoperability. Next acceptance work: the supervised longer ride and deliberate fault scenarios, plus FIT/CSV export implementation and validation.

### Activity exports and deferred hardware acceptance

The user explicitly deferred the 30-minute ride and deliberate dropout tests. No additional hardware test was run for this slice. New `activity-core` code exports controller journals through both a Rust CLI and the app's **Activity exports** window. The native bridge performs export work off the render thread. Original recordings remain unchanged, each output directory is exclusive, and the report records source SHA-256 and recovery/precision warnings. See [activity exports](activity-exports.md) for exact timestamp, pause, missing-value, and recovery rules.

The real `session-1791600730-87-8888051` capture exported to `export/activities/session-1791600730`: 43.410 s elapsed, 37.206 s timer time, two running intervals, 50 CSV samples, and 37 FIT records. The CSV retains every telemetry notification; FIT includes running samples at whole-second resolution with explicit timer events. FIT currently omits GPS, altitude, distance, and synthetic target grade rather than presenting trainer counters or commands as route measurements.

Verification: 32 Rust tests pass across activity-core, trainer-core, and device-probe, including six export tests. Clippy and formatting pass. The native export window was rendered and inspected. The actual updated `dist/OpenRide.exe` reports `ACTIVITY_EXPORT_OK`, and its FIT/CSV bytes match CLI output. The original source hash is unchanged. The `fitdecode` validation script passes syntax checking, but external decoder execution remains blocked by unavailable network/package cache; GoldenCheetah was not found in the usual install locations. Neither external interoperability gate is claimed complete. The standard `dist/OpenRide.exe` bundle now includes the persistent trainer session and activity-export UI; older staged bundles need not be used.
### User-confirmed FIT import (2026-10-09)

The user successfully imported the exported session FIT into their analysis application (reported as "opencheetah" after the GoldenCheetah discussion) and saw a power graph spanning approximately 42 seconds. The exported running samples begin at 02:52:25.296 UTC and end at 02:53:07.462 UTC; their FIT whole-second timestamps span 42 seconds. This is consistent with the reported graph extent, while acknowledged session boundaries retain 43.410 seconds elapsed and 37.206 seconds active timer time.

Import and power display are now user-confirmed. Summary-duration interpretation, pause handling, other sensor fields, and the independent decoder check remain unverified externally. The 30-minute ride and deliberate dropout tests remain deferred at the user's request.
