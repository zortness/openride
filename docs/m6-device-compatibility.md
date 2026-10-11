# M6 — Multi-device pairing and friend testing

Status: implementation started 2026-10-11 with read-only paired BLE discovery,
capability diagnostics, saved source roles and independent Bluetooth HR, power
and cadence reception. See
[M6 progress](m6-progress.md) for implemented scope and verification;
physical sensor/trainer compatibility remains open.
This brief follows the repository's existing documentation layout.

## Project Description

Expand the offline Windows riding experience to equipment owned by three friends,
with explicit device pairing, virtual shifting, and diagnostics that support
development without owning every device. Compatibility claims remain specific
to tested model, firmware, transport, and feature.

## Why This Milestone

The owner currently has the tested Elite Suito, Fenix 6 Pro, Garmin ANT USB-m and
an unvalidated chainstay cadence sensor. Friends can supply physical validation
on an Echelon EX-5S, Wahoo KICKR CORE with Zwift Cog/Click, and Saris M2.
The CORE's single-cog setup makes virtual shifting part of this milestone.

## User-Visible Outcome

- Discover devices and assign resistance control, power, cadence, heart rate,
  and controller inputs independently; remember selections.
- See searching, connected, receiving measurements, and controlling resistance
  as distinct states, with supported capabilities visible.
- Ride a sample route offline, shift with supported input devices, record and
  export the activity, and save a diagnostic bundle when something fails.
- Ride with a power source without automatic resistance control; manual mode
  can prompt the rider to adjust resistance.
- Run the Windows rider package without Python, Godot or development tools.

## Completion Class

Integration and operational. Software readiness and hardware validation are
separate gates; simulated devices do not establish physical compatibility.

## Final Integrated Acceptance

**Ready for friends:** packaged discovery/pairing, standard sensor support,
bounded explicit resistance checks, virtual shifting, diagnostics and regression
checks on existing equipment are usable without a development environment.

**Compatibility validated:** at least one additional trainer model and one
independent Bluetooth sensor complete supervised testing. Record telemetry,
resistance response where supported, ride completion and exported data.
CORE/Cog shifting requires physical validation before declaring that setup usable.
Each target retains its own status; the overall gate does not certify all three.

Existing deferred endurance and deliberate dropout tests remain open. This
milestone does not silently close M2/M5 acceptance or establish broad reliability.

## Architectural Decisions

### Pair by capability and role

Keep protocol handling in the Rust device worker and the pairing experience in
Godot. Discover capabilities rather than infer support from names or brands.
Allow separate power/cadence/HR sources. Missing data remains missing; do not
silently replace a selected sensor. One owner controls a trainer across transports.

### Combine terrain and gearing through one controller

Virtual gears affect requested pedal load, not recorded route elevation or measured
power. Native gear commands are one backend; application-generated resistance
changes are another. The latter can be developed on the Suito in a fixed physical
gear with a gamepad. Actual shift feel and limits require hardware testing.
Terrain and shifting must not compete through independent command writers.

### Separate input actions from devices

Define shift up/down, choose left/right/straight, confirm/back and pause/menu
actions. Keyboard, gamepad, future Click adapters and optional OpenBikeControl
integration map into these actions. Exact Click generation/protocol is unresolved.
Do not require a commercial bridge for the open-source riding path.

### Investigate the Echelon bike separately from its screen

Try Windows Bluetooth discovery before requiring an Android port. QZ reports
EX-5/EX-5S telemetry and automatic resistance support, which warrants investigation
despite the user's manual-resistance experience. This is not verification of the
friend's exact revision. Screen sideloading and simulator performance are separate
future questions. Manual prompts remain a fallback; exact resistance-level targets
require a validated mapping and derived power must be labeled estimated.

## Error Handling Strategy

Explain unsupported capabilities, rejected commands, stale data and connection
loss. Bound control requests and require explicit recovery before control resumes.
Keep trainer and sensor failures distinguishable. Capture protocol failures for
replay, preserve recoverable activity records, and avoid competing connections
when an external bridge owns the physical trainer.

## Risks and Unknowns

| Target | Evidence and intended approach | Unverified details |
|---|---|---|
| Echelon EX-5S, ECHKIN215-01 screen | QZ reports telemetry and automatic resistance; investigate bike BLE directly, optional bridge or manual mode | Exact electronics/firmware, power provenance, screen ownership of connection, installed Android and sideloading access |
| Wahoo KICKR CORE + Zwift Cog/Click | Separate trainer telemetry/control, button input and virtual gears | CORE generation/firmware, Click generation, exposed protocols and native gear support |
| Saris M2 | Manufacturer documents Bluetooth and ANT FE-C; standard discovery with FE-C fallback | Firmware, actual Bluetooth services and available ANT adapter |

The filed ECHKIN215-family manual describes Android 9; this does not establish
the exact -01 screen's installed software or suitability for our renderer.
No additional trainer has yet been tested by OpenRide.

## Existing Codebase / Prior Art

- Existing Suito FTMS worker, acknowledged commands, captures and activity exports.
- Existing ANT USB-m receiver and selected HR integration; external cadence remains deferred.
- Existing controller navigation, action handling and simulated trainer fixtures.
- QZ: Echelon protocol/reference and potential optional bridge, with separate license terms.
- BikeControl: input and shifting reference. Its public repository explicitly
  excludes working trainer/controller emulator implementations; public stubs do
  not supply a reusable shifting engine.
- OpenBikeControl: candidate optional controller-action interoperability protocol.

Research references from the planning discussion:

- [QZ equipment compatibility](https://github.com/cagnulein/qdomyos-zwift/wiki/Equipment-Compatibility)
- [QZ Echelon adapter](https://github.com/cagnulein/qdomyos-zwift/blob/master/src/devices/echelonconnectsport/echelonconnectsport.cpp)
- [ECHKIN215-family filed manual](https://fccid.io/m/01190ad7dad6f2e641468c0c34017ff995a68dd6bbb69c98b64ef41aa76427de.pdf)
- [Wahoo CORE/Cog FAQ](https://support.wahoofitness.com/hc/en-us/articles/16470181834642-KICKR-CORE-Zwift-One-FAQ)
- [Saris M2](https://saris.com/products/m2-smart-trainer)
- [BikeControl virtual shifting](https://bikecontrol.app/virtual-shifting/)
- [BikeControl shift behavior](https://bikecontrol.app/blog/virtual-shifting-with-and-without-bikecontrol/)
- [BikeControl repository and private implementation boundary](https://github.com/OpenBikeControl/bikecontrol)
- [OpenBikeControl protocol](https://openbikecontrol.org/)

## Relevant Requirements

Offline Windows riding, MIT original code, independent device roles, durable
exports, controller-friendly interaction, and evidence-based compatibility.
No formal requirement IDs have been assigned.

## Scope

### In Scope

- Generalized trainer discovery/capabilities and remembered pairing by role.
- Bluetooth HR, cycling power, and speed/cadence sensor reception.
- Standard trainer control, virtual shifting and an input abstraction usable by Click adapters.
- Suito regression, CORE/Click and M2 validation, and a bounded Echelon feasibility check.
- FE-C brought forward if discovery establishes a need; Suito provides local test hardware.
- A sample route, setup guide, explicit diagnostic capture and compatibility matrix.

### Out of Scope

Android renderer/sideloading, watch companions, broad legacy USB trainer support,
additional ANT adapters without test evidence, and open-world road traversal.

### Non-Goals

Universal trainer certification, compulsory cloud accounts or paid bridges,
replacing vendor firmware tools, and a structured ERG workout engine.

## Technical Constraints

Windows desktop first, Godot client and Rust hardware/core modules; no TypeScript
runtime required. Original code remains MIT; external implementations and data
retain separate terms. Ordinary installed-route riding must work offline.

## Integration Points

Bluetooth FTMS/HR/Cycling Power/CSC, existing ANT HR and optional FE-C, controller
events, optional bridge/protocol adapters, activity journals and exports.
Diagnostic bundles identify app version, device capabilities and protocol events;
show their contents before sharing and omit routes/activity history by default.
Network keys and credentials must not enter diagnostic bundles or public fixtures.

## Testing Requirements

Use protocol fixtures and simulated peripherals for optional fields, rejected
commands, stale/missing values, disconnects and recovery. Regress the Suito/Fenix
paths locally. Friends establish physical telemetry/control/shift behavior.
Sanitized captures become regression fixtures. Track capability discovery,
telemetry verification, resistance verification, full rides and known issues
separately for each model/firmware/transport.

## Acceptance Criteria

- Pairing choices persist and selected source identities are recorded.
- Unsupported capabilities are visible before control is attempted.
- Gamepad shifting can be exercised on the Suito without changing route geography
  or fabricating measured power; pause, limits and control ownership remain intact.
- Friends can run discovery and share useful diagnostics without development tools.
- Physical compatibility results are attributable to exact equipment/firmware.
- Manual-only equipment remains usable with a valid power source or explicitly
  identified estimate; prompts do not pretend resistance changes were acknowledged.
- Both readiness and physical-validation gates above have explicit evidence/status.

## Open Questions

- CORE generation/firmware and Click version; M2 firmware.
- Friends' Windows versions, PCs, Bluetooth adapters and ANT dongle availability.
- Echelon BLE services with the console disconnected, exact bike revision and
  whether reported power is measured or derived.
- Native versus emulated shifting backend per trainer and initial gear model.
- Direct Click support versus optional OpenBikeControl/bridge support for first test.
- Availability of an independent Bluetooth sensor for the compatibility gate.
