# M1: route inspection progress

Status: complete on 2026-10-05. Native integration and the exported Windows application are verified against the M1 route-inspection scope.

The original code is MIT licensed. Private Garmin inputs and generated local previews are ignored by Git and have not been designated for redistribution.

## Implemented foundation

- Independent Rust activity importer for GPX tracks and single-activity TCX documents.
- Garmin sensor extensions, lap summaries, original sample ordering, missing measurements, and diagnostics.
- Content-based matching of paired exports, preserving separate source variants; TCX is the default view when present.
- Conservative distance calculation: no connection across source segments, missing positions, non-increasing timestamps, or time gaps exceeding 60 seconds. This is a visible M1 inspection policy, not finalized road matching or pause inference.
- Inclusive sample-index subsection extraction, versioned route manifest, validation/reopening, and GPX track export without biometrics or activity timestamps.
- Export refuses to overwrite an existing file, including source activities. Atomic publication uses a same-filesystem hard link; unsupported filesystems return an error.
- Godot track and elevation plots, source selection, clickable sample inspection, range controls, and diagnostics.
- Thin Rust/Godot bridge source, threaded import wiring, build script, Windows export preset, and native integration test.

## Verification recorded during development

- Seven Rust integration tests pass, including the opt-in private-corpus test: all 31 files form 16 groups, with all source variants retained.
- RPV TCX: 1,014 samples, two laps, 12.540 recorded km, approximately 12.560 GPS km under the conservative distance policy.
- The repeated timestamp is at the second track/lap boundary, sample index 651; both facts are reported without dropping the sample.
- Seven Rust integration tests pass across the workspace, including the private corpus. Workspace Clippy passes with warnings denied.
- Godot headless preview checks passed for paired-source selection, distance labels, sample inspection, and subsection controls, using precomputed Rust output.
- A rendered RPV preview was visually inspected at 1280 x 860; route, profile, controls, and findings fit the window.

- Native integration now passes with godot-rust 0.4.5 and Godot 4.6.2: RPV import, samples 100–800 exported as 701 points, and validated reopening in the UI.
- The optimized release extension builds using `tools/build.ps1 -Release -Offline`; the native test with that DLL reports `NATIVE_OK` and exits 0.
- All 31 original inputs match the SHA-256 baseline after testing.
- A native import screenshot was rendered and inspected at 1280 x 860 (`local/rpv-native.png`).
- Import work uses a Rust thread with plain Rust values; Godot API calls stay on the main thread. Exports select the original Rust activity by SHA-256, avoiding loss of integer types through Godot's JSON representation.

The Windows release export exits 0. Running `dist/OpenRide.exe` from its own directory with `--verify-rpv` reports `NATIVE_OK` and exits 0: it imports the external RPV activity, selects samples 100–800, exports 701 points, and reopens the manifest through the app's controls. Checks use explicit conditionals because release builds strip GDScript assertions. A separate graphical launch imports the RPV activity and saves `local/rpv-standalone.png`; the render exits 0 and was visually inspected. No editor process or online service is required for these runs.

## Current environment constraints

Rust 1.93.1 and Godot 4.6.2 are installed. The CPU reports AMD Ryzen AI Max+ 395 and Godot reports AMD Radeon 8060S graphics. This machine is not the defined 16 GB RAM / 8 GB dedicated VRAM benchmark; memory inventory through CIM was denied, so no RAM capacity is asserted.

The user fetched the locked dependencies and installed Godot 4.6.2 templates. The native bridge builds offline. For sandbox verification, Windows x86_64 templates were copied into the ignored local Godot profile. Ordinary developer exports use the templates in the normal Godot profile.

Godot also reports restricted access to the Windows root certificate store. The route preview uses no network. Test runs redirect Godot application data into a local ignored directory to avoid changing global editor settings.

## Engine decision

Retain Godot for the desktop client and inspection/editor tooling, with independent Rust models and a thin GDExtension. M1 demonstrates native background import, local file access, UI inspection, and standalone Windows packaging. Keep engine APIs on the main thread; pass plain Rust values to workers and retain typed originals for export.

This decision does not establish terrain streaming performance, hardware control reliability, or the 16 GB RAM / 8 GB VRAM target. M2 must validate trainer control; M3 must validate real scenery and map size. Revisit the engine decision if those measurements expose a material limitation.

No trainer connection or control is part of M1.
