# M3 frame pacing investigation

The riding preview now defaults to **60 FPS cap** (VSync disabled, engine limit 60). The adjacent **VSync** option restores display-synchronized pacing with no engine frame limit. Choose VSync if the capped mode produces visible tearing on your display. The preview saves the application's previous settings and restores them when it closes. This choice is currently per-preview, not a persisted preference.

## Evidence and interpretation

All comparisons used the exported application, the installed RPV building/coast package, 1920 × 1080 rendering, and the development Radeon 8060S with Godot Compatibility rendering. Twelve-second samples followed two seconds of warmup.

| Mode | p50 | p95 | p99 | Maximum |
|---|---:|---:|---:|---:|
| Original VSync, no frame limit | 17.327 ms | 31.441 ms | 31.921 ms | 32.425 ms |
| VSync off, uncapped (diagnostic only) | 0.318 ms | 0.502 ms | 0.744 ms | 6.314 ms |
| VSync off, 60 FPS limit | 16.665 ms | 16.804 ms | 17.015 ms | 17.191 ms |

The uncapped run reported 36,446 drawn frames and 36,446 process frames, 76 draw calls and 23,514 rendered primitives at the final monitor sample. The capped run reported 721 drawn/process frames. These counters establish that rendering remained active; they do not establish how many unique frames reached the physical display. The uncapped mode is a diagnostic, not a product default.

The original riding script's p95 execution cost was 0.217 ms, versus 0.180 ms in the capped run. There were no chunk-wait frames in these short samples. This supports a synchronization/presentation explanation for the original long intervals on this machine; it does not isolate the driver, compositor, remote desktop, or monitor as the root cause. No geometry reduction was needed for this change.

The benchmark now reports script timing, engine process monitor, draw counters, VSync mode, engine cap and embedded-window status. Engine process timing is retained only as a diagnostic: Godot has a [reported 4.6.2 monitor issue](https://github.com/godotengine/godot/issues/118935) involving frame limiting/VSync. It should not be interpreted as isolated CPU rendering cost. Frame statistics here come from wall-clock process-frame intervals.

## Reproduce

```powershell
./tools/benchmark-rider.ps1 -Visible
```

This runs the first valid installed route at simulated 60 km/h for 122 seconds, following a two-second warmup. It sends no hardware commands. The default package completes its approximately 1.97 km section during that interval. Use `-Seconds` and `-Speed` for another duration/speed. Logs and 500 ms process-memory samples are written into a new ignored `local/benchmark-TIMESTAMP` directory. Without `-Visible`, the automated helper requests a hidden window.

For controlled comparisons, keep the same display, presentation mode, package and background workload. Remote/automated results should be supplemented by a visible run on the rider's actual display. Actual GPU residency, the named 16 GB / 8 GB reference machine and agreed acceptance thresholds remain open M3 requirements.

## Verification

The exported app was rebuilt. The pacing selector check reported `cap=true vsync=true restore=true`. The normal 60 FPS mode remains selectable alongside VSync; neither changes route geometry, grade, speed calculation or device behavior.

The final 122-second exported run at simulated 60 km/h reached the route end (1,972.489 m), with 7,321 drawn/process frames, nine live chunks and three cached payloads. Frame intervals were p50 **16.665 ms**, p95 **16.722 ms**, p99 **17.020 ms**, maximum **17.530 ms**. The riding script p95 was 0.191 ms. Exit code was 0; see ignored `local/pacing-full-route.log`. The run included a brief separate pacing-selector check on the same desktop, so it is a development observation rather than a controlled certification benchmark.

There were **30 chunk-wait frames**, approximately half a second cumulatively at this cap. Rendering continued smoothly while simulated movement waited for the full neighborhood. Short samples had missed these transitions. This remaining loading behavior is distinct from the original VSync-related frame intervals and should be addressed or explicitly accepted before closing M3. Engine video-memory accounting peaked at 32,876,909 bytes; actual GPU residency is still unmeasured.

Remaining M3 gates: reduce/review movement pauses at chunk boundaries, settle detailed thresholds and named reference hardware, measure driver GPU residency, and perform a visible/offline installation acceptance run. The improved development-machine timing is not a claim that the complete 16 GB / 8 GB target has passed.
