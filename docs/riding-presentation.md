# Riding presentation and detailed RPV scenery

Implemented 2026-10-10 after the riding-view discussion. Open `dist/OpenRide.exe`, refresh **Offline routes**, and select **Rancho Palos Verdes - detailed scenery**. The installed package is `dist/content/RPV-detail-20261010-184836.orpack`. The original RPV package is retained. `tools/preview-riding.ps1` selects the newest detailed package by default.

## Rider display

- Primary power, cadence, heart rate and speed numbers use 52 px text at the default UI scale. **Stats: compact / large / extra large** saves the size preference in `user://riding-ui.cfg`.
- Elapsed time, distance remaining and course grade appear below the primary metrics. Diagnostic numbers no longer duplicate them in the status line.
- Cadence comes from Bluetooth telemetry and expires after three seconds. Selected HR retains its independent ANT freshness behavior. Missing readings display `--`.
- Live elapsed time starts at the first running state and includes pauses; it is a display value, not a replacement for acknowledged export timer intervals. Preview elapsed time advances while playing; replay uses capture time.
- Procedural start/finish arches mark the route. The start arch is eight metres ahead of the first point so it is visible from the initial rider view; the finish arch is at the final route point. Neither changes activity timing or route length.

## Content generation

`tools/road_scenery.py` reads the exact hash-bound OSM snapshot. `chunk-scenery.py --detail-osm PATH` emits optional `presentation.json`, road/marking meshes, terrain colors and vegetation instances. Original route JSON remains byte-identical to the accepted RPV route. The current package has 90 chunks, about 13.25 MB of content, 119 nearby road ways and 68 vegetation instances. Road attributes match 409 of 417 route points; unmatched/ambiguous lane layouts use zero lateral offset.

- Nearby roads are generated within 300 m of the route and the available terrain extent. Bridge/tunnel ways are skipped and counted. Paths and explicitly unpaved ways have a different material.
- Route widths prefer explicit `width`, then an estimated 3.2 m per mapped lane, then class defaults. These are presentation estimates, not surveyed carriageway widths. Parking/shoulder dimensions and complex junction geometry are not fully reconstructed.
- Matching requires a nearby, aligned OSM centerline; parallel ambiguous candidates fall back. Full selected tags and width provenance remain in presentation metadata.
- White edge lines, dashed one-way lane dividers and a yellow centerline on known two-way/two-lane roads follow accepted route heights. `lane_markings=no` suppresses markings. Complex directional/turn layouts are rendered conservatively. Markings are approximate, not a surveyed paint inventory.
- Rider lateral placement respects route/way direction, one-way direction, configured driving side and outer turn-only lanes. An explicit cycling lane is used only with an explicit total width; otherwise its placement is insufficiently defined. Unknown layouts remain centered. Offsets transition smoothly by route station distance and affect the camera only.
- OSM simple land-cover polygons color the terrain. Procedural world-space materials supply ground/asphalt detail. Relation-member land-cover polygons are skipped and reported rather than filling holes incorrectly.
- Vegetation is seeded deterministically within mapped grass/scrub/woodland areas, plus mapped tree nodes. Road, building, water and route clearances exclude unsuitable placements. Shrubs/trees are representative procedural models, not reconstructed individual plants. Chunk-local MultiMeshes and 160/260 m visibility ranges bound rendering work.
- Roads continue outside the activity endpoints where the nearby OSM geometry and terrain coverage permit. Simple crossing road ribbons are a first pass; detailed intersection markings/meshes remain a refinement.

Build a new package without modifying earlier packages:

```powershell
./tools/update-rpv-detail.ps1
```

## Optional aerial imagery

The importer can acquire a bounded 2048Ã—2048 PNG mosaic from the USGS NAIP imagery service, preserving requested/returned bounds, source service metadata, download time and checksum. Download time is explicitly not the photography date; per-image acquisition dates are unresolved in this first mosaic trial.

```powershell
./tools/update-rpv-detail.ps1 -AcquireImagery
```

This creates a new package; refresh the route library afterward. Acquisition requires internet access; riding remains offline. Imagery is hashed in the package, validated before loading, and blended with OSM terrain colors and procedural detail. Existing roads/buildings remain explicit geometry. Data attribution is retained in the manifest and shown in the riding view.

The agent environment's download attempt failed with Windows network-unreachable error 10051. **No real aerial imagery is included in the installed package.** The importer has mocked source-validation tests, and the exported client successfully loaded/rendered a separate synthetic PNG fixture. A real geographic snapshot still needs the command above and visual alignment review. Source: [USGS NAIP imagery service](https://imagery.nationalmap.gov/arcgis/rest/services/USGSNAIPPlus/ImageServer).

## Verification

- 44 Python authoring tests pass, including width/direction/fallback rules, unchanged route inputs, deterministic vegetation and clearance, primitive bounds, and imagery provenance/host checks.
- Windows release build and export succeeded. The exported live fixture verifies large power/cadence/HR values, stale HR, exclusive trainer ownership and Start/Pause/Resume/Finish/export flow without using hardware.
- Start, midpoint and finish screenshots were rendered and inspected. Full streamed traversal retained the nine-chunk live limit and three-chunk prefetch cache. Typical observed worst mesh commits were around 13â€“20 ms; first shader/imagery setup can cost more. This is not a new minimum-hardware performance certification.
- Accepted RPV `route.json` SHA-256 remains unchanged; simulation distance/elevation and controller behavior are unchanged.
- Real distance readability, scenery preference and simultaneous Bluetooth/ANT riding still need user feedback. No physical trainer commands were issued during these changes.


### Imagery timeout handling

The first user download saved `service.json` but timed out on `exportImage`, before a generated-image response was saved. Its 2048-pixel request is below the saved service's 4000-pixel limits. The downloader now uses a 120-second socket timeout and up to three attempts with short backoff for timeout/connection/transient HTTP failures. It prints each stage/attempt and records failed requests in `failure.json`. A socket timeout is not an overall job deadline; three slow attempts can take several minutes.

Retry the ordinary command above. If image generation remains slow, use:

```powershell
./tools/update-rpv-detail.ps1 -AcquireImagery -ImagerySize 1024
```

Optional `-ImageryTimeoutSeconds` (1?600) and `-ImageryAttempts` (1?5) override limits. Resolution is never silently reduced. Each invocation creates a fresh snapshot; earlier captures remain available for diagnosis. The importer retry/failure regression tests pass, but successful real NAIP acquisition remains unconfirmed.

### Real imagery acquired and rendered (2026-10-10)

The user reported 2048/1024 requests timing out and successfully acquired 512?512 imagery with `-ImagerySize 512`. Snapshot: `local/rpv-imagery-20261010-190103`; installed package: `dist/content/RPV-detail-20261010-190103.orpack`. This supersedes the earlier statement that no real imagery was available; the earlier package remains unchanged.

Verified PNG size/hash against the package manifest, unchanged accepted route bytes, and exported-client rendering (`IMAGERY_LAYER_OK width=512 height=512`, `STREAM_TRAVERSAL_OK`, nine live chunks). Inspected `local/naip-512-real.png`. The roughly 2028?2534 m extent yields approximately 3.96?4.95 m per pixel: useful ground color/context, with visibly soft close-up imagery. This does not establish surveyed image/road alignment or image acquisition dates.

Potential next acquisition improvement: smaller geographic tiles at 512 pixels each, with cached successful tiles and resumable retries, assembled offline under the existing image budget. Confirm service latency at the smaller footprints before promising higher-resolution downloads. Existing detailed packages have identical display names; their folders distinguish revisions. For direct preview of this successful package use `./tools/preview-riding.ps1 -Package dist/content/RPV-detail-20261010-190103.orpack`.

## Editor authoring follow-up
The desktop Build a route flow now integrates source acquisition, resumable tiled aerial imagery, repair and preview. See [Route authoring](route-authoring.md). The earlier one-request imagery limitation is superseded by 512-pixel tile mosaics up to 2048 pixels; a live tiled USGS download is still pending user validation.

