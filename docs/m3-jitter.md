# Stop and GPS jitter review

The offline detector finds small clusters with little net progress, long elapsed time, and slow GPS-derived movement. It emits suggestions, never modifies the source activity or selects repairs automatically. Slow intentional loops can still resemble stops: all proposals require review. Recorded motion can veto a stationary interpretation; the current importer does not identify wheel-speed sensors or supply explicit pause events.

## RPV review

The preview opens with the map unobstructed. Click **Show tools**, then **Repairs** to access jitter review. **Details** holds diagnostics and layer toggles; **Road edits** holds alignment and elevation controls. **Hide tools** restores the full map view. The point slider remains at the bottom.

```powershell
./tools/preview-scenery.ps1 -Preparation export/rpv-m3-corrected/preparation.json -JitterReview export/rpv_jitter_review.json
```

Select the candidate, use **Preview candidate**, then **Inspect repair** to zoom in. Gold shows the proposal; toggle it to compare with the pink unrepaired profile. Ctrl-click allows multiple candidate selections; preview shows the first selected candidate only. **Save selected repairs** saves the chosen spans to a new JSON file. Run the repair builder below to apply that file into a new output directory. Changing road/elevation corrections invalidates the loaded review; reopen the unchanged corrected preparation before reloading its report.

RPV detection found one cluster: samples 981–1009, 658 seconds, 33.88 m accumulated GPS movement, 4.14 m net displacement, median derived speed 0.083 m/s. There are three recording gaps exceeding 60 seconds and no recorded-speed fields in this GPX. This supports a stop/drift interpretation but does not establish its cause. The report retains the matching timing/coordinate evidence and the source activity hash.

The automated proposal uses surrounding endpoints 980 and 1010: a 26.238 m chord, at most 0.859 m from removed corrected positions, 52 retained points, zero current gap/grade findings. It differs from the earlier manual 981–1010 proposal. Both are approximations for review. Original activity, accepted +16 m road corrections, and the earlier repair file are preserved; no trainer control is enabled.

## Reproduction

The Rust activity importer supplies canonical sample indexing for GPX/TCX. Its separate evidence file preserves timestamps, recorded speed or null, coordinates, and breaks without changing existing route hashes. This prevents invalidating accepted corrections merely to add detection evidence.

```powershell
cargo run --offline --locked -p content-builder -- --activity-evidence import/2012-02-12_kurt_culver_to_rpv.gpx local/rpv-activity-evidence.json
python tools/detect-jitter.py export/rpv-m3-corrected/preparation.json local/rpv-activity-evidence.json export/rpv_jitter_review.json
# After reviewing and saving selected spans to export/rpv_selected_jitter.json:
python tools/repair-profile.py export/rpv-m3-corrected/preparation.json export/rpv_selected_jitter.json export/rpv-m3-jitter-cleaned
```

Outputs must not already exist. Detection requires the exact activity hash plus matching sample coordinates (1e-10 degree tolerance for JSON float round trips). Reports and selected repairs bind the exact corrected-preparation hash. No network is involved.

## Initial heuristic and limits

Candidates use at least five samples within 10 m of the starting point, at least 30 seconds, at least 5 m accumulated movement, path/net ratio at least two, and median short-interval GPS speed at most 0.5 m/s. Intervals over 60 seconds are counted as recording gaps and excluded from the speed median; at least three shorter intervals are required. Missing/non-increasing timestamps do not produce proposals. More than 20% of available recorded speeds exceeding 1.5 m/s blocks a stop proposal. No recorded speed is treated as unknown, not zero. These initial values need tuning across more rides.

Each candidate includes one surrounding sample on either side. Repair validation restricts suggestions to one explicitly selected OSM way, short connectors and limited deviation. Suggestions whose repair geometry is unsuitable are recorded as rejected. Proposals are disjoint and capped at 300 cluster samples; preparation input is capped at 10,000 points. This detector targets stationary clusters, not general route simplification, hairpin removal or automatic roundabout/U-turn cleanup.

Verification covers stationary clusters, steady progress, fast loops, recorded-motion conflict, time gaps, absent timestamps, source conflicts, importer sample/time preservation, and refusal to overwrite. The RPV Godot select/preview/save path and subsequent offline repair regeneration were exercised; the rendered UI was inspected.
