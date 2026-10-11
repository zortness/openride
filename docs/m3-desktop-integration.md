# M3 desktop content integration

## Use the exported app

Open `dist/OpenRide.exe`, click **Offline routes**, select the RPV package, and click **Preview route**. The reviewed building/coast package is installed locally at `dist/content/RPV.orpack`.

The default library is a `content` folder beside the executable. Drop self-contained package directories into it and click Refresh. Choose folder supports another library location; that selection is saved in the existing application user-data directory. Older whole-section authoring packages and incomplete/unsupported packages remain listed with an explanation and cannot be selected for riding.

The preview opens in a separate window within the same application process. Closing it returns to the unchanged route inspector. It provides simulated speed, scrubbing and look-around; it does not connect the trainer or record an activity. Real power, route-grade control and ride recording integration belong to M4.

Neither Godot authoring tools nor GIS/Python are required to load an installed package. All source processing remains offline authoring work. The private RPV content stays in ignored local/dist folders and is not part of the application source distribution.

## Build/export

```powershell
./tools/build.ps1 -Release -Offline
./tools/export-rider.ps1
```

Godot 4.6.2 Windows export templates must be installed for authoring/export. The distribution contains `OpenRide.exe`, `OpenRide.pck`, `openride_godot.dll` and `device-probe.exe`; content remains separately installable.

`tools/sync-scenery-runtime.ps1` copies the five canonical runtime files from `apps/scenery-preview` into ignored `apps/rider/scenery`, rewriting resource paths for the rider project. Both build/export scripts run this step. Edit the canonical preview files, not generated copies. This keeps one authored renderer while preserving the standalone scenery project. Directly opening the rider editor after a new checkout requires this sync/build step.

## Validation

- Library validation checks package type/version, streaming settings, fixed payload paths, duplicate cells, file sizes and presence. Payload checksums and geometry validation run when the runtime loads each file; library listing is not a full integrity audit.
- Nine content-index tests passed: valid index, missing package, incompatible version, unsafe route/coast paths, fractional/duplicate cells, incomplete payload and missing riding surface.
- A newly copied library was discovered by the exported executable: `CONTENT_LIBRARY_OK valid=1 invalid=0`.
- The exported executable launched with the working directory set to Windows Temp and traversed the full section: nine live chunks, three cached payloads, 48 loads / 39 unloads. Rendered screenshot inspected; exit 0. No development executable was launched for this check. This is a local deployment test on the development machine, not a separate clean-PC installation or a physical network-disconnect test.
- The export and runtime print the pre-existing Windows root-certificate-store diagnostic; the local preview completed successfully.

## Initial performance measurement

Follow-up: [frame pacing investigation](m3-frame-pacing.md) improves the measured frame intervals and records full-route loading waits. The numbers below are the historical VSync baseline.

The exported app has an opt-in diagnostic:

```powershell
./dist/OpenRide.exe -- --verify-content-library --benchmark-seconds=30
```

It selects the first valid package, renders at 1920 × 1080, warms up for two seconds and samples wall-clock intervals between process frames during playback at 25 km/h. Console/log output includes `SCENERY_BENCHMARK`. This is scenery playback, not the deferred 30-minute hardware ride.

Development machine: previously inventoried Ryzen AI Max+ 395; runtime reports AMD Radeon 8060S, OpenGL Compatibility driver context 26.5.2.260505. Installed RAM remains unverified because CIM inventory was denied. This is not the agreed 16 GB / 8 GB dedicated-VRAM reference machine.

Latest 30-second sample (2026-10-10):

| Measurement | Result |
|---|---:|
| Render viewport | 1920 × 1080 |
| Frames / elapsed | 1,693 / 30.007 s |
| Frame interval p50 / p95 / p99 | 17.219 / 31.503 / 32.228 ms |
| Maximum frame interval | 33.008 ms |
| Frames waiting for chunks | 0 |
| Peak live chunks / cache entries | 9 / 3 |
| Peak sampled process working set | 232,742,912 bytes (~222 MiB) |
| Peak sampled process private memory | 406,884,352 bytes (~388 MiB) |
| Engine-reported video memory | 32,725,661 bytes |

Process memory was sampled every 500 ms, so brief peaks can be missed. Engine video-memory accounting is not measured driver allocation/residency or total GPU consumption. The frame intervals include scheduling/presentation effects; this test does not isolate the cause of slower frames. It covered approximately the first 222 m including warmup, not sustained full-route performance. Logs and memory results are in ignored `local/benchmark-1080*` files.

M3 performance acceptance remains open: investigate frame pacing, measure longer representative playback and GPU residency, agree the named reference hardware and detailed thresholds, and verify the target there. Vegetation and additional surface detail are explicitly deferred by user agreement. M2's long ride and deliberate dropout tests remain deferred separately.
