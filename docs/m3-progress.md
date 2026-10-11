# M3: scenery preparation

Transition accepted (2026-10-10): the user considers current performance adequate to move forward. [M4](m4-progress.md) has started with the Rust power-simulation demonstration. Remaining hardware benchmarks and loading refinements are follow-ups rather than blockers to M4 implementation.

Frame pacing update (2026-10-10): the [pacing investigation](m3-frame-pacing.md) supports a 60 FPS cap with VSync off on the development machine; VSync remains selectable. Full-route exported playback measured p95 16.722 ms and p99 17.020 ms, with bounded chunks. Thirty terrain-wait frames remain a separate movement-loading issue. Reference-machine/GPU residency acceptance remains open.

Desktop integration update (2026-10-10): [exported-client package loading](m3-desktop-integration.md) now discovers dropped-in streamed packages and launches the riding preview in the desktop app. Vegetation/further detail are deferred by agreement. Initial performance measurements are diagnostic; M3 acceptance remains open.

Building update (2026-10-10): [streamed OSM building massing](m3-buildings.md) adds 388 nearby footprints, including concave houses, while preserving the accepted road, terrain and coast. The riding launcher now selects this package. Forty Python tests and rendered traversal passed; vegetation, exported-client integration and performance acceptance remain pending.

Coastal update (2026-10-10): the [coastal riding preview](m3-coastline.md) adds OSM-defined ocean, a bounded coarse terrain backdrop, and right-drag look-around (F resets). The default riding launcher selects the coastal package. Road elevations are unchanged; geographic coverage and nominal sea-level limitations are documented.

Riding presentation update (2026-10-10): a [rider-height streamed view](m3-riding-view.md) now provides a connected asphalt surface with normal depth testing, a presentation-only terrain corridor, sky/haze, and compact simulated metrics. Run `tools/preview-riding.ps1`. Route elevations remain unchanged. Distant scenery, exported-client integration and performance acceptance remain open.

Async streaming update: user confirmed the initial chunked view. Chunk reads/checksums/JSON parsing now run on one worker, with nine live chunks, three cache slots, 100 m forward prefetch, and one mesh upload/build per frame. Full traversal, rapid seek/stale-result discard and corruption cleanup were exercised. Rendered maxima: 0.644 ms neighborhood update and 4.046 ms per-chunk mesh build; full frame-time/memory acceptance is still open. See [streaming details](m3-streaming.md).

Streaming update: [bounded road/terrain streaming preview](m3-streaming.md) now loads 256 m cells around simulated traversal, with a nine-chunk live limit. Full RPV traversal demonstrated 39 loads/30 unloads across a 90-chunk package, preserving all terrain/road triangles. Maximum measured synchronous update was 33.493 ms; background loading/prefetch and performance acceptance remain pending. The full route metadata and chunk index remain resident. Use `tools/preview-streaming.ps1`; the exported rider and authoring Library do not yet load this experimental stream format.

Traversal update: generated road profiles now support distance scrubbing, constant-speed Play/Pause, end-of-route stopping, and an optional follow camera. Nine deterministic traversal checks and the actual RPV package UI check passed. Terrain-derived route elevation was explicitly deferred; see [road-route decisions and traversal](m3-road-routes.md). No trainer connection or rider-power physics is involved. Bounded chunk streaming is next.

Route geometry update: [optional road-geometry route creation](m3-road-routes.md) now reconstructs the explicitly selected OSM path, with GPS mode preserved and explicit whole-section fallback. RPV generates 417 points over 1,972.489 m. The preview compares blue generated geometry against original/corrected GPS data; elevations remain derived from corrected activity measurements and unvalidated.

Package update: [local preview packages and discovery](m3-packages.md) now support self-contained JSON bundles, checksums, compatibility/attribution validation, and drop-in discovery through the scenery preview's Library dialog. This is a bounded whole-section loader; chunk streaming and exported-client integration remain pending. The RPV package retains the manual repair's unvalidated review status.

Status: started, 2026-10-09. M2's longer ride and deliberate dropout tests remain deferred. This first slice establishes standalone source preparation and a Godot road-profile preview. It is not a complete scenery package or an accepted M3 milestone.

Latest: the [converter comparison](m3-converter-comparison.md) supports owning terrain and the corrected profile in OpenRide, with OSM2World as optional scenery. The [correction workflow](m3-corrections.md) now supports explicit per-point road choices, an elevation offset, separate source-bound save/load, and offline regeneration preserving the original profile. The RPV authoring preparation starts with no applied corrections. Vertical alignment and route continuity still need review; packages, streaming and performance acceptance remain open.

Converter update: [the OSM2World baseline comparison](m3-converter-comparison.md) has now run and rendered in Godot. Its flat-ground model contains useful buildings/street detail but logs roof errors. Recommendation: retain OpenRide terrain and corrected-road ownership; evaluate OSM2World separately as an optional offline scenery source. Corrections and package/streaming acceptance remain next work.

## Sequence

1. **Preparation and source audit (implemented):** bounded route selection, local coordinates, inspectable road-profile geometry, geographic acquisition bounds, deterministic outputs and source identity.
2. **Terrain and OSM comparison (next):** obtain source snapshots for the same section, record licenses/checksums/coordinate and vertical references, compare DEM and activity elevation, identify road matches and bridge/tunnel cases. Run OSM2World and a minimal custom terrain/road pipeline against those sources before choosing the initial renderer pipeline.
3. **Corrections:** store explicit alignment/elevation edits separately, bind them to source identity, preview them, preserve them during regeneration and reject/report changed-source conflicts. Road geometry and simulated grade must consume one corrected profile.
4. **Installable content:** versioned data-only package, attribution and dependency validation, fresh-library discovery, runtime loading and bounded chunks, simulated traversal without network access.
5. **Performance and acceptance:** settle the named CPU/GPU and thresholds, measure frame times/load stalls/memory/package size/build time, then run the milestone acceptance checks.

## RPV preparation result

Input: the user's `export/rpv.orroute`, the full 2012 Culver-to-RPV route (~68.6 km). Selected distance window: 30,000–32,000 m; endpoints are the existing samples inside the window, not interpolated positions. It produces 81 points and 126 preview triangles.

Six findings need review: three raw segment grades over 15% in magnitude (approximately -15.61%, -17.56%, +19.79%) and three preserved segment breaks at source samples 1001, 1004, and 1005. These are diagnostic flags, not proof of bad geography. No elevation correction or automatic gap bridging has been applied.

The source-data inventory found Garmin activity files but no local OSM/DEM files. Java and GDAL were not found on PATH. An attempted Overpass download failed with `Unable to connect to the remote server` in the agent environment; no OSM coverage or elevation comparison is claimed yet.

Acquisition bounds, with an approximate 500 m buffer:

| Boundary | Degrees |
|---|---:|
| South | 33.77622332 |
| West | -118.42068276 |
| North | 33.79901055 |
| East | -118.39874116 |

## Run the tools

From the repository root, choose a **new** output directory:

```powershell
cargo run -p content-builder --offline --locked -- export/rpv.orroute export/rpv-m3-preview 30000 32000
./tools/preview-scenery.ps1
```

The example output already exists locally. Rebuilding requires a new directory, then pass `-Preparation NEW_DIRECTORY/preparation.json` to the preview script. `-Godot PATH` selects another Godot executable. The separate Godot project also opens in the editor at `apps/scenery-preview/project.godot`. The existing rider executable has not been changed by this slice.

Drag the background to orbit, use the mouse wheel to zoom, and move the slider to inspect points. The preview's simple ribbon has an illustrative six-metre width; it is not surveyed road geometry. It has no terrain or scenery and cannot connect to trainers. All geometry is currently loaded at once, bounded to a short section; this is not the eventual streaming runtime.

Outputs:

- `preparation.json`: source route SHA-256, original activity hash, actual/requested interval, origin, original elevations, diagnostic grades, findings, generated mesh and asset hashes. Written last.
- `road-preview.obj`: standalone Y-up mesh for inspection in Blender or other authoring tools. No engine-specific assets or scripts.
- `bounds.geojson`: geographic source-acquisition polygon.
- `osm-query.overpass`: a starting query for nearby roads, buildings and selected landcover, with referenced members/nodes. This is not a complete regional extract or a guarantee of all objects crossing the bounds.

Inputs are read-only and limited to 64 MiB. Invalid/out-of-route windows, windows longer than 5 km, polar/antimeridian sections and selections with fewer than two points are rejected. Local equirectangular coordinates use metres, X east, Y up and -Z north. Absolute activity elevation is retained separately; missing elevation stays missing. No preview faces span segment breaks, missing elevations, or spacings outside 1–100 m. A section without previewable faces is rejected. Asset generation precedes exclusive output-directory creation; I/O failure may leave a partial directory without its final preparation file. Output is deterministic for identical input bytes and options.

This draft authoring format is deliberately distinct from the future installable world format. It contains private route-derived information; its source activity is not licensed for public sharing merely because the builder is MIT licensed.

## Source choice and acquisition

Start with [USGS 3DEP 1/3 arc-second GeoTIFF elevation](https://www.usgs.gov/faqs/what-types-elevation-datasets-are-available-what-formats-do-they-come-and-where-can-i-download), available through [The National Map](https://apps.nationalmap.gov/downloader/). Treat it as a terrain candidate; road profiles still require bridge/tunnel and vertical-reference review. Prefer a small clipped authoring raster after preserving the downloaded source and metadata. Do not make GDAL a rider dependency.

Use the generated query in [Overpass Turbo](https://overpass-turbo.eu/) and export raw OSM XML into an ignored local source directory when network access is available. Only the geographic bounds need to be sent, not the activity file. Record retrieval time and [OpenStreetMap attribution/ODbL terms](https://www.openstreetmap.org/copyright). Later acquisition should include complete relevant landcover relations and validate coverage; the starting query is not a final source-ingestion pipeline. Query syntax follows the [Overpass language reference](https://wiki.openstreetmap.org/wiki/Overpass_API/Overpass_QL).

[OSM2World](https://github.com/tordanik/OSM2World) is a candidate converter with MIT-licensed code; that license does not replace source-data obligations. Its actual conversion quality, elevation integration and operational cost remain untested here. No new GIS runtime dependency has been adopted.

## Verification

Three new Rust integration tests cover local coordinate/selection semantics, geographic bounds, invalid/oversized windows, missing elevations and breaks, reproducibility, mesh export and exclusive output creation. Together with ride-core, nine tests passed with one private-corpus test intentionally ignored. Content-builder Clippy passed with warnings denied.

The real RPV output loaded in Godot 4.6.2 in headless and rendered runs, each reporting `SCENERY_PREVIEW_OK points=81 triangles=126`. The 1280×800 screenshot was inspected. This verifies the initial file-to-3D-preview path, not real terrain, source correctness, streaming, package installation, or the 1080p performance gate.

## Terrain source workflow

For the first contextual preview, use a small export from the [USGS 3DEP dynamic elevation service](https://elevation.nationalmap.gov/arcgis/rest/services/3DEPElevation/ImageServer). This is a resampled, multi-resolution service snapshot, **not** the previously proposed 1/3-arc-second product. Its native source identity and vertical datum require review of the saved service/catalog metadata before road corrections. A 256×256 output is an authoring preview resolution, not a claim about source accuracy.

Run in a normal network-enabled PowerShell from the repository root:

```powershell
python tools/acquire-scenery.py export/rpv-m3-preview/preparation.json local/rpv-sources
```

An existing output directory is refused; use a new name for another acquisition. The script requires only Python's standard library. It sends geographic bounds to USGS and Overpass, never the original activity or sensor data. It requests raw F32 TIFF with the `None` rendering rule, EPSG:4326, bilinear resampling, and no aspect-ratio adjustment; the TIFF and actual export response are saved together. The [ArcGIS export-image contract](https://developers.arcgis.com/rest/services-reference/enterprise/export-image/) distinguishes elevation values from hillshade imagery and describes the returned extent. The OSM query includes relevant nodes, ways, relations, and their descendants; it is still not a guarantee of complete regional coverage.

The snapshot contains `terrain.tif`, `usgs-service.json`, `usgs-catalog.json`, `usgs-export.json`, `map.osm`, the OSM query and `sources.json`. The manifest records timestamps, request parameters, checksums, attribution, and per-source errors. Files are saved exclusively; partial downloads remain inspectable and a failed source makes the command exit nonzero. A complete download is not geographic validation. The agent's actual attempt returned Windows error 10051 for both services, recorded in `local/rpv-sources-agent-attempt/sources.json`.

After acquisition, build into another new directory:

```powershell
cargo run -p content-builder --offline --locked -- --terrain export/rpv-m3-preview/preparation.json local/rpv-sources export/rpv-m3-terrain
./tools/preview-scenery.ps1 -Preparation export/rpv-m3-terrain/preparation.json
```

The importer checks preparation/source identity and TIFF/export checksums, validates dimensions and CRS, decodes single-band F32 elevation, and constructs a north-up mesh at pixel centres in the existing local coordinate system. Missing pixels produce holes; missing comparison values remain missing. Route comparison uses bilinear sampling. Activity elevation and route geometry are not changed, and `trainer_ready` remains false. The source manifest and attribution are embedded in the derived preview. OSM rendering is not yet implemented.

The terrain is lit for shape inspection. Toggle **Terrain** or **Road overlay (ignores terrain occlusion)** to compare the road with the ground. The cursor shows terrain elevation and the activity-minus-terrain difference. Differences can include datum, barometer bias, sampling, and ground-versus-road structure effects; they are not automatic corrections. All terrain geometry is still loaded at once, bounded to 256×256 pixels; streaming remains later work.

The standalone authoring crate uses the MIT-licensed `tiff` crate (0.11.3, deflate and LZW support). This adds no TIFF decoder or GIS dependency to the rider executable. The downloaded USGS TIFF uses LZW despite the request's `LZ77` compression setting; the initial real-data build exposed this, and LZW support was added and tested.

### Terrain verification

Six content-builder integration tests passed, including coordinate orientation/pixel-centre sampling, missing-data holes, wrong CRS/dimensions, source checksum enforcement, preservation of activity elevations and original files, and refusal to overwrite outputs. Three Python acquisition tests passed using simulated HTTP responses. Those acquisition tests do not establish that the remote services accept the requests.

The complete offline TIFF → builder → Godot path was exercised with a 64×64 synthetic raster generated by `tools/fixtures/terrain-snapshot.py`. The rendered screenshot was inspected at `local/terrain-synthetic.png`; the viewer prominently labels it **SYNTHETIC TEST TERRAIN**. It establishes the ingestion/rendering path, not real RPV terrain correctness. Actual OSM acquisition/coverage, OSM2World conversion, real elevation comparison, corrections, and performance acceptance remain pending.

## Real RPV terrain

The user completed acquisition into `local/rpv-sources` at 2026-10-10 04:14:53 UTC. Both sources report success, and all six manifest file checksums match. The 228,099-byte TIFF decoded after enabling LZW. The exported raster is 256×256 with zero missing pixels. `export/rpv-m3-terrain/preparation.json` contains 65,536 terrain vertices and 130,050 terrain triangles and is 6,152,977 bytes. These are bounded preview geometry counts, not streaming/performance acceptance.

The 6,270,707-byte OSM XML contains 27,710 nodes, 2,025 ways and 33 relations, including 352 highway ways and 1,550 building ways. These counts include referenced members and do not imply every object lies within the requested rectangle or that regional coverage is complete. OSM scenery and OSM2World comparison remain next steps.

The catalog response contains 22 entries, with no transfer-limit flag: ten primary datasets list NAVD88 and twelve overview entries have no vertical datum. This does not identify the contributing raster for every pixel or reconcile the activity's vertical reference.

All 81 activity points were compared with terrain. Activity-minus-terrain ranges from -16.704 to -13.842 m, with median -15.806 m and mean -15.661 m. A consistent offset is a plausible interpretation, not a proven cause or an applied correction. The full buffered terrain ranges from -0.366 to 274.507 m. Activity heights, road geometry and `trainer_ready=false` remain unchanged.

Open the real terrain preview:

```powershell
./tools/preview-scenery.ps1 -Preparation export/rpv-m3-terrain/preparation.json
```

Verification: six content-builder integration tests pass, including matching results from uncompressed and LZW synthetic TIFFs; Clippy passes with warnings denied. The actual RPV terrain built and the rendered Godot run exited 0 with `SCENERY_PREVIEW_OK points=81 triangles=126` (that log's triangle count refers to the road). The screenshot at `local/rpv-terrain.png` was inspected and shows the terrain context and elevation comparison. M3 remains in progress: OSM alignment, source-pipeline comparison, corrections, packages and streaming/performance gates are still open.
