# M4: first complete offline ride

Status: first offline ride accepted for the tested Windows/Suito BLE/RPV configuration on 2026-10-10. FIT and compatible TCX single-file imports are user-confirmed in GoldenCheetah 3.8. The chronological entries below retain earlier findings; the final acceptance entry supersedes their pending export checks. Started 2026-10-10. The user accepted the current scenery performance as adequate to advance. M3 reference-hardware/GPU-residency checks, small movement waits during chunk loading, and further scenery polish are tracked follow-ups. M2's 30-minute hardware ride and deliberate dropout tests remain deferred.

Completion feedback update: both constant-speed playback and the power demo now show a persistent **Route complete** banner with distance and a Replay button. Replay clears the banner and resets progress; pausing or stalling before the finish does not show it. Exported checks passed for both completion paths and replay, and the rendered banner was inspected.

## First slice: power-driven simulation demonstration

Open `dist/OpenRide.exe`, then **Offline routes → RPV → Preview route**. Select **Power demo**, set watts, and press Play. Zero watts means coasting. Pause freezes the simulated state; Play resumes. Switching into Power demo resets the route. Route scrubbing is disabled in this mode. Constant-speed inspection remains available.

The demo assumes **85 kg combined rider/bike mass**, explicitly shown in the UI. It has no device connection and does not record/export an activity. The standalone authoring preview does not load the Rust extension and therefore continues to offer constant-speed traversal only.

### Core model

`ride-core::simulation` is independent of Godot and device I/O. It uses fixed 20 ms steps, course grade, gravity, rolling resistance, aerodynamic drag and drivetrain efficiency. Defaults are Crr 0.004, CdA 0.32 m², air density 1.225 kg/m³ and drivetrain efficiency 0.97. These are prototype parameters, not a calibrated athlete/bike model.

Power is converted to force using a 1 m/s denominator floor at launch; forward acceleration is capped at 3 m/s². Speed is forward-only, bounded to 50 m/s. There is no wind, drafting, braking or reverse motion. Speed represents distance along the road; route progress is horizontal station distance, with the slope cosine applied during integration. Course grade comes from the corrected profile, independent of any future trainer command limits/difficulty settings.

Missing power pauses the core and requires an explicit resume. Zero power continues coasting. Nonfinite/out-of-range power is rejected. A caller gap above 250 ms pauses with a timing-gap status instead of applying a large catch-up step. Route completion clamps distance and stops. Manual pause preserves speed for resumption.

`RideSimulation` is a thin GDExtension adapter. The demonstration currently calls the fixed-step core from the preview using monotonic elapsed milliseconds and a manually held power value. Movement still waits for the preview's terrain neighborhood. This is the integration demonstration; the live ride must move simulation/recording into the independent worker and handle timestamped telemetry freshness there before connecting hardware. Fixed-step partition independence with constant input does not imply arbitrary delayed telemetry can be replayed without timestamp handling.

### Verification

- Nine ride-core integration tests passed (one private-corpus test remains intentionally ignored), including three simulation tests covering uphill/flat behavior, fixed-step call partitioning, zero versus missing power, explicit resumption, downhill coasting, completion, invalid input and timing gaps.
- Clippy passed for ride-core and openride-godot with warnings denied.
- Windows release build/export succeeded. The exported RPV demo reported `POWER_DEMO_OK`: 200 W for ten simulated seconds produced 35.434 m progress and 17.047 km/h at the current +3.324% grade.
- The rendered Play/Pause check advanced to approximately 40.102 m, then held position while paused. Screenshot inspected; exit 0. No hardware commands or activity records were produced.

## Second slice: timestamped capture replay

The exported riding view now offers **Capture replay > Open capture > Play**. Select a device probe's `session.jsonl` (not its diagnostic `ble.jsonl`). The working sample is `local/device-probe-20261009-191212-caa1dcdc/session.jsonl`. Power, capture time, virtual speed and distance appear in the existing bottom status strip. Pause freezes both capture time and movement. Capture completion has its own banner and Replay action, distinct from finishing the route.

`ride-core::replay` owns simulation on an independent Rust thread. Godot polls one latest snapshot; terrain availability and renderer updates do not advance or gate this worker. Commands use a bounded 16-entry queue. Switching mode, replacing a capture or closing the view stops and joins the worker. Historical trainer commands in a capture are ignored; this mode performs no hardware I/O or activity recording.

Replay normalizes the first power notification to time zero, preserves relative `elapsed_ms` timestamps, and selects the latest notification at each 20 ms tick. Thus power transitions/freshness have up to one tick of quantization. Zero watts coasts; missing power or a sample age of three seconds pauses movement. The capture clock continues to allow later telemetry to arrive, but movement requires **Resume movement** once fresh power returns. Pause can freeze that clock too. A worker scheduling gap over 250 ms pauses replay rather than catching up. The final incomplete physics tick is not extrapolated beyond the last notification.

Input checks cover schema/header, sequence order, monotonic timestamps, power bounds, complete JSONL records, 16 MiB/100,000-row limits and a 24-hour duration cap. Snapshots include the source SHA-256. Bounded file loading/parsing happens synchronously before starting the worker; asynchronous capture loading is a later improvement if large captures warrant it. The adapter still uses the visible **85 kg combined mass** and current model defaults; profile management remains deferred. Manual Power demo remains a render-thread demonstration.

Verification:

- Fourteen normal ride-core integration tests passed; two private-data tests are opt-in. Five replay tests cover timestamps/partitioning, zero versus missing power, stale latching, explicit resume, worker progress without polling, pause, invalid data, timing gaps and capture completion.
- The opt-in replay test ran the supplied Suito capture against the installed RPV package's `route.json`: 54.900 seconds, 96.259 m virtual progress, `replay_complete`. These are new simulated route results, not the original recorded distance.
- Windows release build/export and scoped Clippy completed successfully. Exported-app verification observed 380 ms of replay advancement while rendering was deliberately blocked for 400 ms; pause held both time and distance. Full exported playback reached the same 96.259 m at 54.900 seconds, displayed Capture complete, then restarted and released the worker on mode change (exit 0). Completion screenshot inspected.

## Third slice: live route control and recording

Implemented 2026-10-10; physical Suito acceptance remains pending. The user confirmed the capture replay displays changing power correctly.

### Short supervised check

1. Open `dist/OpenRide.exe`, then **Offline routes > RPV > Preview route > Live SUITO**.
2. Choose **Connect SUITO**. The worker acquires control and restores flat grade, then shows Ready to start. It does not send Pause before the first acknowledged Start.
3. Once fresh power is available, select **Start ride** and pedal for a minute or two. Compare displayed watts, movement, Course grade and Trainer grade.
4. Select **Pause**, then **Resume**; finally **Finish ride**. Finish restores flat grade and, if the app has an acknowledged running session, requests Pause. It observes telemetry for five seconds, then disconnects. Finishing before Start does not send Pause or claim a Pause acknowledgement.
5. Retain the new `route-*` folder under `%APPDATA%/Godot/app_userdata/OpenRide Route Inspector/sessions`. The UI shows its path after finishing. Send that folder name for verification.

The existing 30-minute ride and deliberate dropout tests remain deferred. No physical trainer commands were sent during this implementation turn.

### Startup correction after the first hardware attempt

Session `route-1791664260-369-76097096` connected successfully, acquired control and acknowledged flat grade. The Suito then rejected `[8,2]` (Pause) with response `[128,8,4]` before any Start had been issued. The initial implementation incorrectly required this premature Pause.

The route controller now tracks accepted Start/Pause commands separately from device status reports. Startup restores flat and remains application-paused, labelled Ready to start. Finish before Start skips Pause; repeated Pause/Finish after an acknowledged Pause also skips a duplicate Pause. Actual rejection after an acknowledged Start remains an error, and the journal never fabricates a Pause acknowledgement. The original diagnostic session behavior remains unchanged.

Verification: 28 trainer-core/device-probe tests passed, including the new command-sequence regression; scoped Clippy, release build/export and exported live UI fixture passed. Physical retry is pending.

### Ownership and lifecycle

The existing `device-probe` BLE controller process owns live physics as well as hardware and journal writes. Godot only displays its updates and sends explicit Pause/Resume/Finish requests. Renderer stalls and chunk loading do not gate movement. Switching modes is disabled during an active session; closing the view or a fatal scenery error requests Finish. Bridge destruction closes the command pipe, retaining the existing EOF shutdown behavior. A shared bridge lock prevents competing sessions inside the app; the existing per-device Windows owner still protects separate OpenRide processes.

Missing/invalid power or power older than three seconds pauses the simulation; fresh samples alone do not resume. The controller restores flat grade and acknowledges Pause before offering explicit Resume with its current epoch. Worker timing gaps over 250 ms also interrupt simulation. Failed control procedures/disconnections retain the existing unconfirmed-state handling; the UI never treats a requested grade or Pause as acknowledged. Hardware validation of these combined paths is still needed.

### Initial control envelope

Virtual motion uses full corrected course grade and the visible 85 kg rider/bike estimate. Trainer grade is clamped to **0-2%**, changing by at most one percentage point per acknowledged one-second interval. Descents send flat resistance. This first supervised envelope stays within the previously tested Suito range. Broader grade ranges and difficulty settings are future work. The existing diagnostic fixed grade sequence is bypassed for route sessions.

### Durable evidence and export boundary

The worker verifies the route profile against the hash of the loaded preview and saves its exact bytes as `route-profile.json` before control acquisition. `session.jsonl` retains:

- `route_session_config`: route SHA-256, package manifest SHA-256, complete current physics settings, and trainer grade limits.
- Existing raw trainer measurements, receipt timestamps, lifecycle changes, control requests and acknowledgements.
- `virtual_state` snapshots approximately every 200 ms: virtual distance/speed, course grade, acknowledged trainer grade, simulation state, power and its receipt time.
- `route_grade_target` records separating course grade from the limited trainer target.

All these records use the same synchronized journal and monotonic clock. The copied profile and hashes identify the route and world versions; users must retain the corresponding scenery package to reproduce its visuals. Pause/finish and hardware telemetry remain distinguishable from virtual motion. Route packages are still prototype, unvalidated content; this does not change their `trainer_ready` flag or establish elevation accuracy.

**FIT/CSV export of the new virtual route journals is deferred** until virtual timing/distance semantics are implemented. The current exporter explicitly rejects these journals so it cannot silently substitute trainer distance. Existing M2 session exports still work. JSONL and the profile snapshot preserve the new rides for later export; TCX/GPX remain in the subsequent interchange slice.

### Verification

- `cargo test --workspace --offline --locked`: **57 passed, 0 failed**, two private-data tests intentionally ignored. New coverage includes live missing/stale power, explicit resumption, timing-gap holds, bounded/ramped grades, and virtual-export rejection.
- Scoped Clippy passed with warnings denied. Release build and Windows export succeeded.
- Exported `LIVE_RIDING_FIXTURE_OK`: connect paused, reject a competing owner, display virtual state/power, explicit resume/pause/resume/finish with the expected command epochs. Screenshot inspected.
- Existing exported `TRAINER_SESSION_OK` regression passed, including control-pipe EOF shutdown and launch-failure display.
- The process fixture simulates trainer replies; it does not verify new physical resistance behavior or actual BLE latency. The short supervised check above is next.

## Next slices

Future requirement: [multiple rider and bike profiles](backlog.md#rider-and-bike-profiles) will persist masses and selectable combinations. Profile management is deferred; keep session parameter handling compatible with that future model.

1. **Worker/replay integration:** implemented for offline capture playback as described above. Live telemetry is now wired into the controller process; configurable session parameters remain future work.
2. **Route-grade control and recording:** implemented with the initial 0-2% envelope and durable virtual-state journal; physical integration validation remains pending.
3. **Supervised integrated Suito ride:** pairing, start, pause/resume, completion and export in the riding flow. Resume stays explicit after telemetry/control loss.
4. **Activity interchange:** add TCX/GPX alongside FIT/CSV; distinguish virtual positions and verify the integrated activity in external tools.

M4 is not complete until a real offline route ride, control/recording agreement and activity interchange meet the milestone criteria.


## Short live ride verified (2026-10-10)

User confirmed successful connection after the startup fix. Session `route-1791669428-795-28244936` contains 519 complete, sequential records and a matching route-profile SHA-256. Start-to-Pause acknowledged time was 44.601 seconds, with 119.426 m virtual progress, recorded power 0-148 W and cadence 0-69 rpm. All 16 control requests were acknowledged; nine course-driven grade targets stayed within the initial envelope. Recorded acknowledged grades agree with the corresponding hardware command acknowledgements. Distance was monotonic and held through Pause/Finish. Flat restoration, Pause and subsequent disconnect completed without controller errors.

This validates a short Start/ride/Pause/Finish path. It does not establish mid-ride Resume, full-route completion, long-duration reliability, deliberate dropout recovery or virtual export acceptance. Those remain open or previously deferred.

### Proposed everyday ride entry

The current preview mode selector is a development/testing flow. Recommended product flow: route library > Ride > device and rider/bike readiness > explicit Start > riding screen > finish summary/export. Remember compatible selections locally as those features arrive. Keep route editing, constant-speed inspection, manual watts and capture replay in authoring/diagnostic tools. This is a proposed UX direction, not an implemented flow or a requirement to build full profile management before the next M4 validation step.


## Virtual exports and direct ride entry (2026-10-10)

Implemented after the short live ride validation:

- Normal launch opens the route library. Select a route and **Ride** to reach the readiness screen; **Preview / tools** retains inspection/replay modes.
- Readiness shows SUITO/Bluetooth, the current 85 kg assumption and 0-2% envelope. Connecting and starting remain separate explicit actions. Profile editing and device selection are not implemented yet.
- Normal riding hides the development motion-mode selector. Finish shows a saved-distance summary and **Export ride**, which preselects the session in the export window.
- Virtual FIT/CSV exports are now implemented, superseding the earlier rejection guard. Raw trainer CSV and virtual snapshot CSV remain distinct; copied profile hashes are checked, and report schema 2 preserves settings and provenance. See [activity exports](activity-exports.md#virtual-route-rides) for timing, sampling and distance semantics.

The Windows app exported the user's successful session to `export/rpv-live-ride/activity.fit`: 45 records, 44.601 seconds, 119.43 m FIT distance. All 60 workspace tests passed (two private-data tests ignored), scoped Clippy passed, and exported-app checks passed for disconnected readiness, explicit start/pause/resume/finish through a fake trainer, saved summary/export handoff, and the real-session export. Readiness/export windows were rendered and inspected. A structural FIT decoder verified CRC, virtual classification, distance, timing and record count on the real output. The optional external fitdecode package is unavailable locally; its validation script was updated, but was not successfully run. The user subsequently confirmed successful GoldenCheetah import with visible power and speed graphs. Distance totals, timer interpretation and pause handling in GoldenCheetah remain unconfirmed.

Remaining M4 work includes TCX/GPX, a full integrated route ride/completion and remaining external distance/timer/pause checks. Existing long-duration/dropout deferrals remain in force. No live hardware commands were sent for these UI/export checks.


## TCX/GPX export slice (2026-10-10)

Implemented TCX alongside FIT/CSV and GPX for hash-verified geographic routes. Future live sessions preserve their exact package manifest. Existing sessions can supply the matching original manifest through the export window or CLI without changing their recordings. Report schema 3 records XML point counts and geographic provenance. TCX/GPX positions are explicitly derived virtual positions; pause intervals split tracks. Missing geography omits GPX and positions, while mismatched supplied geography rejects the export.

Verification: workspace tests passed (61 passed, two private-data tests ignored), scoped Clippy passed, release build/export succeeded. The exported Windows app produced `export/rpv-live-xml` from the existing real RPV session and displayed the export result correctly. Independent Python XML parsing/semantic checks verified 208 TCX and GPX points, 44.601 seconds and 119.426 m against the saved streams/profile/manifest. Original session SHA-256 remained unchanged. XSD validation is not established. The user subsequently confirmed both GPX and TCX files verified in GoldenCheetah (2026-10-10), establishing external application acceptance of these short-ride exports. No live hardware commands were sent; saving the manifest on a future physical ride remains to be observed.

Next: perform the full integrated RPV route ride with an intentional ordinary pause/resume and end-of-route completion/export check. The 30-minute endurance and deliberate dropout tests remain deferred. M4 stays open pending its integrated ride/interchange acceptance.

## GoldenCheetah acceptance (2026-10-10)

User confirmed: "GPX and TCX files verified in GoldenCheetah." Short-ride FIT, TCX and GPX interoperability is now user-accepted. No additional field-by-field measurements were reported. The remaining M4 acceptance step is a full RPV ride with ordinary pause/resume, automatic route completion, saved geographic provenance and export review. Long-duration and deliberate dropout tests remain deferred.

## Full RPV ride and export follow-up (2026-10-10)

Session `route-1791677726-822-147140954` reaches exactly the saved profile endpoint, 1,972.489 m. Acknowledged running time is 336.555 s; elapsed time is 340.125 s, including one 3.570 s pause/resume. All 48 command requests were acknowledged. The journal ends with `session_check_complete`, reason `route_complete`, flat restoration and Pause acknowledged. The exact package manifest was saved successfully. These establish full-route progress, ordinary pause/resume, automatic completion and portable recording in the physical ride.

The user reported combining TCX/FIT imports to obtain all data and no GoldenCheetah map placement. The original TCX and GPX contain 1,613 valid geographic points; original FIT omitted geography. Added FIT latitude/longitude and standard/enhanced altitude from the verified route origin/profile. New export: `export/rpv_session_export/with-fit-geography`. Ten activity-core tests and scoped Clippy passed; release build and exported-app real-session export passed. A separate structural decoder checked all 338 FIT records against TCX for coordinates, altitude, power and speed, plus CRC; XML semantic checks passed and original journal SHA-256 remained unchanged.

M4's physical full-route check is now evidenced. Single-file GoldenCheetah acceptance remains open while the user tests the corrected FIT and clarifies other missing fields. No cause for the TCX map/import behavior has been established. Endurance and deliberate dropout tests remain deferred. No hardware commands were sent during this investigation.

### Power-import clarification (2026-10-10)

The user clarified that neither TCX nor GPX displayed power in GoldenCheetah; power appeared after importing FIT. Earlier blanket TCX/GPX acceptance therefore establishes file import, not complete sensor interoperability. The original TCX contains 1,613 Watts entries (0-185 W, 1,485 nonzero). GPX power uses the documented custom OpenRide extension, whose interpretation is not guaranteed.

GoldenCheetah's current upstream TCX reader accepts both prefixed and unprefixed Watts, so a cause is not established. Its exporter uses an unprefixed TPX/Watts/Speed with a locally declared ActivityExtension namespace. Prepared `export/rpv_session_export/tcx-compatibility-check/activity.tcx` in that equivalent form for a diagnostic import; a recursive namespace-aware XML comparison confirmed identical elements, attributes and values. Application code and original exports are unchanged in this check. The user identified GoldenCheetah 3.8, dated September 19, 2026. Retrieval of the exact v3.8 parser source was unsuccessful; the inspected current upstream reader is not evidence of exact release behavior. The diagnostic TCX import result remains pending. The corrected FIT remains the recommended single-file retest for sensor data plus geography.

References: https://github.com/GoldenCheetah/GoldenCheetah/blob/master/src/FileIO/TcxParser.cpp and https://github.com/GoldenCheetah/GoldenCheetah/blob/master/src/FileIO/TcxRideFile.cpp .

## M4 acceptance: single-file exports (2026-10-10)

User confirmed the diagnostic TCX loads both power and map data into GoldenCheetah 3.8, and the FIT in `with-fit-geography` works with all data. Promoted that exact TCX spelling to the production exporter: TPX declares the local ActivityExtension/v2 default namespace, with unprefixed Speed and Watts. The namespace and values are unchanged. This establishes an observed compatibility remedy for this installation, not a proven internal GoldenCheetah parser defect.

Fresh verification after the change: ten activity-core tests passed, including the exact compatible extension form and FIT geographic fields; release build/export passed. The exported Windows app processed the full RPV journal into `export/rpv_session_export/accepted-formats`. Generated TCX and FIT files are byte-identical to the corresponding user-accepted files. Python XML semantic checks passed for 1,613 points, 336.555 s timer and 1,972.489 m. Original recordings and earlier exports were preserved.

M4's integrated offline ride is accepted for the tested Windows desktop, Suito BLE and prepared RPV route, within the documented 0-2% trainer envelope. Evidence covers route progress, one ordinary pause/resume, automatic completion, saved route/package identity, and single-file FIT/TCX analysis/map import. GPX remains a track interchange format; its custom power extension is not GoldenCheetah-validated. Missing HR cannot establish HR interoperability.

Next milestone: M5 hardware/content hardening. Recommended first slice: ANT+ sensor ingestion for cadence/heart rate while retaining BLE as the sole trainer controller; follow with FE-C control under single-controller enforcement and additional route fixtures. No M5 hardware implementation has started in this acceptance step. The 30-minute endurance test, deliberate dropout tests, reference-hardware performance checks and rider/bike profiles retain their existing deferred status.
