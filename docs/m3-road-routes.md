# Optional road-geometry route creation

## Deferred elevation option

Recorded decision: for unmapped paths, cleaned GPS horizontal geometry may be combined with terrain-derived elevation. Horizontal geometry and elevation should be independent choices (GPS/road versus recorded/terrain/manual heights). Terrain cell spacing is not positional accuracy; the current RPV raster is approximately 7.9 × 9.9 m per cell. Terrain cannot recover trail alignment, and GPS drift on hillsides or bridge/tunnel surfaces can make draping misleading. A recorded-elevation offset such as +16 m must not also be added to terrain-derived heights. The user explicitly deferred this capability on 2026-10-10: continue road routes without implementing terrain-derived route elevation now.

## Simulated road traversal

For a generated continuous profile, the bottom bar provides **Route distance** and **Play/Pause**. The slider changes from original source-point inspection to distance along the generated road. Position, displayed elevation, and grade are interpolated from that same profile; at an exact vertex, the displayed grade belongs to the segment arriving at that vertex. Playback stops at the end; Play then restarts it. **Road edits** contains preview speed (default 25 km/h) and a follow-camera option. Turn off Route distance to return to source inspection. Changing corrections invalidates traversal until regeneration.

This is a deterministic constant-speed visual simulation, not rider-power physics or trainer control. Profiles with gaps, non-increasing distance, or inconsistent geometry/grade are rejected for traversal. The entire section is still resident; chunk streaming, performance gates and exported-rider integration remain pending.

Route preparation now has two explicit geometry modes:

- `gps` (default): retain the existing corrected profile, or reviewed profile when present.
- `road`: reconstruct a unique connected path through explicitly selected OSM ways, preserving road vertices and inserting samples so spacing is at most 5 m.

## RPV example

```powershell
python tools/create-route-profile.py export/rpv-m3-corrected/preparation.json export/rpv-m3-road-route --mode road --osm local/rpv-sources/map.osm
./tools/preview-scenery.ps1 -Preparation export/rpv-m3-road-route/preparation.json
```

The prepared RPV output already exists. For another build, choose a new directory. Use **Show tools → Road edits → Generated route geometry (blue)** to compare the generated centreline with the corrected GPS profile. Cyan remains the original activity geometry. The slider inspects source points by default; enable **Route distance** for generated-route traversal. Editing corrections invalidates the generated overlay until rebuilt.

RPV output from the 81-point corrected preparation: 417 generated points, 1,972.489 m, no fallback, and no findings under the current 15% grade threshold. This reconstructs the road without requiring the manual stop repair. The stationary backtracking does not become traversable geometry. The route remains an unvalidated preview with trainer control disabled.

## Algorithm and boundaries

The selected-way graph is built from the exact hash-matched OSM snapshot. Start/end projections split their corresponding edges. The builder requires a unique path between those projections and checks the selected anchors against it. Disconnected ways, alternative graph paths, anchors off that path, backtracking over 10 m, missing choices, access restrictions, contrary one-way direction, and bridge/tunnel cases require review. It does not infer missing streets by routing along the nearest road or by using matching names alone. This first implementation supports a single unambiguous traversal; intentional laps and U-turns must be split/reviewed rather than silently removed.

Elevation is separate: corrected activity heights are placed along road distance, keeping monotonic anchors at least 5 m apart and preserving endpoint heights, then linearly interpolated onto the generated vertices. The accepted +16 m offset is inherited from corrected heights. This changes intermediate grade calculations; it is not an independent survey or vertical calibration. Original activity, corrections, and any reviewed profile remain embedded unchanged for comparison. Road geometry, distance and generated grade all use the same new profile.

`--fallback-gps` explicitly permits falling back to the entire GPS profile if road reconstruction fails. The reason is stored in `route_profile.fallback_reasons` and displayed in the preview. Without that flag, reconstruction reports the issue and creates no output. Partial mixed road/GPS sections are not implemented yet; split the route or use the explicit whole-section fallback. Source recording breaks are not automatically road-network breaks; this road mode intentionally constructs connected geographic geometry, while retaining the recording breaks in original data.

Output adds `route_profile` and a source-preparation hash. It embeds geometry and drops authoring sidecar references. Package creation prefers `route_profile` when available. The local package reader recognizes it. The current limits remain a small preview section, 2,000 graph edges and a generated route of 1–6,000 m; this is not a global routing engine or a completed streaming format.

Tests cover curved roads and vertex retention, grade/elevation interpolation, maximum sample spacing, ambiguous/disconnected paths, one-way conflicts, U-turns, source hash conflicts, GPS mode and explicit fallback. Godot loaded and rendered the real RPV road output.
