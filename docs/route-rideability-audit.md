# Full-route software rehearsal

Before preparing more scenery, audit the saved route with the native riding
physics and inspect the remaining steep segments:

```powershell
target/release/content-builder.exe --review-audit local/my-review local/ride-audit.json
```

Choose a new output file. The command leaves the saved review and original source
files unchanged. Its report pins the review revision and selected geometry source.
Invalid or unresolved profiles fail rather than running a partial route.

The audit runs constant-power trials at 150, 200 and 250 W using the default
85 kg total rider/bike mass and the same 20 ms simulation steps used by riding.
Each trial reports completion, elapsed simulated time, peak sampled speed, or the
station/source sample where progress stopped. Less than 1 m of progress in a
60-second interval is reported as `stalled_at_test_power`. A trial stops after
24 simulated hours with `time_limit`; neither status counts as completion.

All raw segments above 15% are reported with their distance, elevation change,
and a contextual 20 m grade centered on the segment. Near route ends the window
is clipped. Context is omitted across unresolved boundaries. The contextual value
helps distinguish small height steps from sustained climbs; it never replaces
the raw grade or changes the profile. A completed trial does not accept steep
grades for scenery baking.

This is a deterministic software check, not a prediction of a rider's finish time
or a substitute for trainer, scenery, frame pacing or endurance validation. It
does not render regional seams, exercise device control, or validate a unified
full-route package. Native simulation's existing 10,000-point limit still applies.

## Current OC result

The repaired 153.126048 km recording completed all three trials: 341.2 simulated
minutes at 150 W, 296.1 at 200 W and 267.1 at 250 W. The 30 remaining raw >15%
segments span 0.36–2.22 m each, with absolute 20 m context grades below 12%.
Their elevations and prior repair decisions were retained. This supports
continuing scenery preparation while keeping the steep-grade findings visible.
