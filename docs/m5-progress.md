# M5: hardware and content hardening

Status: started 2026-10-10, following user acceptance of the full RPV ride and single-file FIT/TCX imports in GoldenCheetah 3.8.

Scope update (2026-10-11): M2 and M3 are now closed by user acceptance. Export
validation and trainer disconnect/reconnect are user-confirmed. The 30-minute
endurance test and reference-hardware performance follow-ups move to M7.
Separate sensor dropout, cadence and simultaneous trainer/ANT-HR validation
remain open in M5/M6. Earlier endurance deferrals below are historical.

## First slice: ANT adapter and sensor reception

Implemented a standalone Rust receive diagnostic in `device-probe`. Bluetooth remains the riding application's only trainer controller. No FE-C channel, resistance command, automatic source replacement or ride integration is added in this slice.

- Uses the installed system `libusb0.dll` and libusb-win32 driver for ANT USB-m `0fcf:1009`. No driver replacement or bundled native USB library. Other adapter models/backends are not supported yet.
- Requires exactly one supported adapter; claims USB interface 0 and holds a Windows-session OpenRide ownership guard. Other ANT applications should be closed. The guard coordinates OpenRide processes, not arbitrary external software.
- Reads firmware/capabilities, serializes radio configuration against acknowledgements, and supports a single receiving HR, cadence, wheel-speed or combined speed/cadence profile per diagnostic.
- Captures raw framed packets, extended sensor identity (device number/type/transmission type), decoded values and timed freshness status in an exclusive JSONL file. The network key is excluded from logs.
- Handles fragmented/coalesced USB reads, checksum errors, counter wrap, repeated samples, implausible deltas and stale telemetry. Cadence/speed need two valid counter observations. Three seconds without measurement packets yields missing data; repeated unchanged counters while packets continue yield zero after three seconds. Slow revolution rates can consequently show zero between events; this policy needs physical validation.
- Wheel data is revolutions/second, not bicycle or virtual speed. Wheel circumference and sensor source selection belong in the integration slice.
- Identity changes reset sensor history. Unknown/common pages do not refresh measurement freshness. Missing HR stays missing.
- Each diagnostic resets the adapter at startup and shutdown; successful cleanup requires its startup response. USB transfers, command waits, capture duration and the outer process watchdog are bounded. This is a diagnostic capture, not a fault-tested long-running receiver.

### Hardware evidence

`local/ant-probe-20261010-174424-91dfea15/ant.jsonl` confirms communication with firmware **AJK2.07RAF**, capabilities reporting eight channels, and acknowledged cleanup. This supersedes M2's OS-enumeration-only evidence.

The first direct USB configuration-descriptor read returned zero bytes. Using libusb's already-enumerated descriptors resolved adapter access; endpoint/interface checks remain mandatory.

`local/ant-cadence-first.jsonl` confirms successful network/channel configuration, a 15-second receive window, zero framing errors, and acknowledged cleanup. It received **zero sensor packets**. At that point no sensor packets had been received. The later combined-profile capture below establishes radio reception, but not the physical source identity or accuracy. Synthetic decoder tests are not evidence of on-bike readings.

### Run a capture

From the repository root in PowerShell, close other ANT applications, wake the sensor, and run:

```powershell
./tools/probe-ant.ps1 -Profile adapter
./tools/probe-ant.ps1 -Profile cadence -Seconds 20
./tools/probe-ant.ps1 -Profile hr -Seconds 20
```

The adapter check requires no network key. Receive checks read `OPENRIDE_ANT_NETWORK_KEY` or the ignored local file `local/ant-network-key.txt` (16 hexadecimal characters). This workspace has that local configuration prepared from Garmin's published network-key documentation. No key is embedded in MIT source or the executable; keep the local file out of version control. The script restores the caller's environment afterward. Garmin's protocol/profile/network-key materials have separate terms; this prototype makes no ANT+ certification claim.

For cadence, rotate the crank before and during capture, then stop while leaving the sensor awake so zero/stale behavior can be inspected. For HR, enable the Fenix's HR broadcast mode. `speed` and `speed-cadence` are also supported profile names; use the latter for a combined sensor. `-DeviceNumber` selects a known 16-bit device number; zero searches for the first matching profile. This diagnostic is not a multi-device discovery UI.

The script prints the new ignored `local/ant-probe-*` directory. `ant.jsonl` is authoritative; `stdout.jsonl` and `errors.txt` preserve console output. A successful process can still have `sensor_packets: 0`; only received identified packets establish reception. Do not export this diagnostic as a cycling activity.

## Next slices

1. Physically validate cadence and watch/strap HR reception, source identity, stopped behavior and freshness. Retain private captures and add synthetic regressions for observed edge cases.
2. Add persistent sensor selection, simultaneous receive channels, worker/session recording and display/export of selected sensor values. Preserve explicit source provenance and BLE power/grade ownership. A sensor failure must not silently select a nearby device.
3. Add Suito FE-C only after receiver integration, with one selected trainer control transport and explicit conflict handling.
4. Expand content fixtures to Torrance Beach, Death Ride and difficult historical routes; improve package/recovery tools and compatibility/attribution documentation.

M5 remains in progress. The 30-minute endurance ride and deliberate dropout tests remain deferred by the user. Rider/bike profiles and reference-hardware performance work retain their existing backlog status.

## References and dependencies

Original implementation is MIT. Wire/API references: [Garmin ANT network keys](https://developer.garmin.com/ant-program/ant-ant-plus/network-keys/), [ANT message protocol](https://www.thisisant.com/assets/resources/Deprecated_D00000652_ANT_Message_Protocol_and_Usage_Rev_5.1_1.pdf), [libusb-win32 public C ABI](https://github.com/mcuee/libusb-win32/blob/master/libusb/src/lusb0_usb.h), and the [ant-plus project's profile implementations](https://github.com/Loghorn/ant-plus/tree/master/src). The latter is a reference only; no TypeScript runtime is introduced. libusb-win32 is a separately installed LGPL library, dynamically loaded from the Windows system directory. No Garmin SDK or shared-source implementation is bundled.

## First sensor captures (2026-10-10)

After the user reported cadence and watch HR broadcasting:

- `local/ant-probe-20261010-174526-f5bbcc64`: dedicated cadence profile, 20 seconds, no sensor packets.
- `local/ant-probe-20261010-174546-840450b9`: HR profile, 15 seconds, no sensor packets.
- `local/ant-probe-20261010-174625-a1586631`: combined speed/cadence profile, 20 seconds, 13 identified packets from device number **35786**, type **121**, transmission type **1**. Zero framing errors; cleanup reset acknowledged. The physical device is not identified: it could be a separate sensor or a trainer's broadcast.
- `local/ant-probe-20261010-174718-2addc951`: HR profile, 30 seconds, no sensor packets; zero framing errors and cleanup reset acknowledged.

Combined-profile reception was intermittent, with gaps exceeding the three-second freshness limit. Valid calculated cadence observations were approximately 9.35 and 15.31 rpm. Some wheel counter deltas exceeded the diagnostic's 20 revolutions/second plausibility ceiling and were reported missing; no wheel circumference or bicycle speed is inferred. These results require source identification and a controlled cadence comparison before riding integration. They do not establish Garmin sensor accuracy or Fenix HR reception.

Verification after implementation: 68 workspace tests passed, two private-data tests ignored; scoped Clippy passed with warnings denied; Windows release build/export succeeded. Seven new tests cover ANT framing, cadence/combined/HR decoding, packed endpoint handling, local key validation and OpenRide adapter ownership. The PowerShell capture script was exercised against the real adapter and combined-profile receiver. Local key configuration is ignored by git. No BLE or FE-C control commands were issued in these diagnostics.

## Fenix HR broadcast verified (2026-10-10)

The user clarified that HR broadcast needed to be enabled manually on the watch, then requested another capture. `local/ant-probe-20261010-174914-5eb2de3d/ant.jsonl` records a successful 30-second HR-profile check: 104 HR packets, 70-79 bpm, maximum packet gap 2215 ms; zero framing errors and clean acknowledged reset. Identity: device number **40658**, device type **120**, transmission type **241**. All 104 decoded HR values were present. Journal sequence/time ordering passed inspection. No trainer control commands were sent.

This establishes short ANT HR reception after enabling the watch broadcast, superseding the earlier unverified HR status. It does not validate optical HR accuracy or long-duration/dropout behavior. Next integration can use this full source identity explicitly; combined speed/cadence source 35786 still needs physical identification and cadence validation.

## Controlled speed/cadence check started (2026-10-10)

Requested a steady-pedal / stopped check with explicit ANT device number 35786:

- `local/ant-probe-20261010-175824-d427db6f`: 35 seconds, 57 packets, zero framing errors, acknowledged cleanup. Cadence rose into the 40s (maximum 49.11 rpm); counter pair 1 advanced from 431 to 452 revolutions. Maximum packet gap was 2,221 ms.
- `local/ant-probe-20261010-175909-9a45cb8b`: separate intended stationary check, 15 seconds, 25 packets, zero framing errors, acknowledged cleanup. Cadence remained 48.00-54.42 rpm; counter pair 1 advanced from 461 to 472 and wheel counter from 27,411 to 28,050. Maximum packet gap was 1,481 ms.

The user confirmed they were still pedaling during the second check, explaining the advancing crank counters. A stationary capture is awaiting explicit stopped confirmation; physical sensor mounting/source identification is still pending. No stationary-state or cadence-accuracy acceptance is claimed. The high wheel-counter rate is mostly excluded by the current 20 revolutions/second plausibility ceiling; its physical interpretation remains unresolved. No decoder thresholds or trainer controls were changed during this check. Further captures should wait for physical-source/motion clarification.

### Confirmed stationary capture

After the user explicitly confirmed the pedals were stopped, `local/ant-probe-20261010-180031-a78828d5` received 46 packets over 15 seconds from 35786/121/1. All eight-byte measurement payload counters were unchanged ((40856, 501, 47594, 29779)). Both cadence and wheel rate settled to zero and remained zero; first decoded zero was at 3334 ms. Maximum receive gap was 494 ms. No framing errors; cleanup reset acknowledged. This validates stationary behavior for this broadcasting source, alongside moving-counter reception. Physical source identity and absolute cadence/wheel-rate accuracy still require confirmation.

### Physical mounting clarification

The user confirmed the Garmin cadence sensor is mounted on the lower chainstay, with no rear wheel installed to trigger its wheel-speed input. During crank movement the intended sensor should therefore show changing cadence counters and fixed wheel counters. Device 35786 showed rapidly advancing wheel counts in the moving captures, so its identity remains unresolved; a trainer broadcast is a hypothesis, not an established identification. Requested a controlled capture with Suito power unplugged and the crank turned by hand to isolate the Garmin source. Do not select 35786 as the Garmin sensor or relax wheel-rate bounds based on the existing evidence.

### Suito-powered-off isolation check

The user confirmed Suito power was unplugged and they were ready to turn the crank by hand. Wildcard searches (not restricted to device 35786) then found no sensor packets:

- `local/ant-probe-20261010-180231-cc51ca38`: combined speed/cadence profile, 25 seconds.
- `local/ant-probe-20261010-180310-cf2a0897`: cadence-only profile, 20 seconds.

Both radio configurations succeeded, with zero framing errors and acknowledged cleanup reset. This supports a trainer origin for 35786 but is not definitive source identification; no power-on repeat was performed. The physical chainstay Garmin sensor is not yet detected or validated. Next information needed: exact sensor model and confirmation it is awake/detecting crank movement (battery/magnet setup). Do not count the prior moving/stopped captures as Garmin sensor acceptance.

## Selected HR ride integration (2026-10-10)

The user deferred the physical Garmin chainstay sensor. Treat 35786/121/1 as a **presumed Suito broadcast**, not a validated Garmin source. Continue using Suito Bluetooth power/cadence and control; do not open FE-C or select this ANT cadence source.

Implemented an optional HR receiver alongside the BLE controller:

- A dedicated thread owns the ANT adapter and its bounded USB calls. FTMS control never waits on USB receive/search. A bounded queue transfers receipt-timestamped HR events into the durable session journal and UI.
- Exact device number, HR profile and transmission type are required; no wildcard pairing or automatic replacement. Search timeout reopens the same receive channel. A USB/initialization error makes HR unavailable for this session; reconnect by finishing and connecting again. HR faults do not pause the trainer.
- The Ready to ride dialog saves optional HR selection and a local key-file path in `user://sensors.cfg`. Default is disabled. No personal watch identity or network key is compiled into the application.
- Live HR expires after three seconds. Export joins use controller receipt timestamps, validate identity and keep missing values missing. FIT/TCX/GPX include selected HR; raw BLE CSV stays unchanged; `heart-rate.csv` and report schema 4 carry separate HR evidence/provenance.
- Receiver shutdown requests a radio reset; cleanup timeout is recorded explicitly. Other ANT applications must release the adapter first.

### Try the selected watch

1. Enable HR broadcasting on the Fenix and reconnect Suito power if still unplugged.
2. Open `dist/OpenRide.exe`, select RPV and Ride. Enable **Use ANT heart rate**.
3. Device number: **40658**; transmission type: **241**. Key file: the absolute path to this workspace's `local/ant-network-key.txt` (already prepared and ignored by git).
4. Connect, confirm BPM appears, then start a short ride and finish normally. These settings persist for later rides.
5. Export and inspect either FIT or TCX in GoldenCheetah for HR alongside power, cadence, virtual speed and route geography.

### Verification and limits

Synthetic tests cover full identity filtering, common-page exclusion, missing HR, timestamp validation, expiry/no fallback and FIT/TCX/GPX HR encoding. The exported application fixture checks HR display, missing HR without stopping the ride, expiry, and existing Start/Pause/Resume/Finish/export behavior. The readiness dialog was rendered and inspected.

The receive-only hardware test exercised the new worker for 12 seconds (`local/m5-hr-receiver-check.json`): zero packets, searching state and acknowledged cleanup reset. Thus this run verifies adapter setup/cleanup, **not new live watch reception**. Earlier diagnostic reception from the Fenix remains the hardware evidence; simultaneous BLE riding + ANT HR and GoldenCheetah HR import still need a short physical ride. No trainer commands were sent in this receiver test. Endurance and deliberate hardware dropout testing remain deferred as requested.

Further M5 work includes additional content/import cases, packaging/recovery/documentation, broader compatibility and optional FE-C control. Cadence hardware validation is deferred. Structured ERG workouts remain a later feature; upcoming riding/workout-view discussion does not imply a workout engine already exists.

Final software checks: 70 workspace tests passed (three opt-in hardware/private-data tests ignored); scoped Clippy passed with warnings denied; Windows release build/export and exported live fixture passed. A fresh export of the accepted full RPV session produced byte-identical FIT, TCX and GPX compared with `export/rpv_session_export/accepted-formats`. The separately invoked receive-only hardware worker check passed as described above.

## Riding presentation follow-up

Implemented the approved large HUD, start/finish arches, nearby roads, road attributes/markings, camera lane offsets, ground materials and seeded foliage. See [riding presentation](riding-presentation.md) for package selection, evidence, limitations and the optional imagery acquisition command. Actual NAIP download is blocked in the agent network environment; the OSM-based package is installed and the optional imagery renderer is fixture-tested.

### Route authoring follow-up
Added the saved desktop authoring workflow described in [route-authoring.md](route-authoring.md), including KML LineString import and resumable imagery tiles. Existing RPV sources/corrections are available in local/rpv-route-project. This does not close the deferred hardware/endurance checks.

