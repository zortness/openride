# OSM road context and alignment diagnostics

The next M3 slice overlays downloaded OSM highway centrelines on the saved terrain and reports nearby alternatives for each activity point. It does not snap the route, resolve access/direction, or change elevation. The user has confirmed the preceding real-terrain and road-overlay view.

## Inspect RPV

The generated output is `export/rpv-m3-osm`. Open it with:

```powershell
./tools/preview-scenery.ps1 -Preparation export/rpv-m3-osm/preparation.json
```

The cyan line is the original activity profile; gold lines are OSM highway context draped onto the DEM. Toggle **OSM road context** independently. Move the cursor to see up to three nearby candidates listed directly above the slider, including name/type, distance and OSM way ID, with ambiguity status. No hover is required. USGS and OSM attribution is shown at the bottom.

To regenerate, use a new directory:

```powershell
python tools/osm-context.py export/rpv-m3-terrain/preparation.json local/rpv-sources NEW_OUTPUT_DIRECTORY
```

This is a standalone, standard-library Python authoring tool alongside the Rust content builder; it adds no Python dependency to the rider. Its narrow algorithm can move into the Rust builder after the alignment workflow is established. Generated files remain private/ignored under `export/`.

## Observed result

| Measurement | RPV section |
|---|---:|
| Highway ways considered | 352 |
| Activity samples with a candidate within 30 m | 81 / 81 |
| Median nearest-feature distance | 2.882 m |
| Maximum nearest-feature distance | 9.686 m |
| Samples with a second way within 5 m of the best distance | 46 |

The closest feature is unnamed for 50 samples, Palos Verdes Drive West for 28, and Paseo del Mar, Apsley Road and Cloyden Road for one each. Many unnamed candidates are footways. These results do **not** establish that the rider used a sidewalk; a sidewalk can be geometrically closer than the road centreline due to lane position, GPS error or differences between historical and current mapping.

Recommendation: keep explicit alternatives, then develop connected route matching with road class, heading, continuity, source breaks and author overrides. Do not select the nearest feature unconditionally. The approximately 16 m vertical offset remains a separate finding; no horizontal or vertical correction was applied in this slice.

## Contract and limitations

- Source snapshot must exactly match the terrain preparation's embedded manifest. OSM bytes and inherited assets must match their checksums. Existing output directories are refused; `preparation.json` is written last. An I/O failure may leave a partial new output directory.
- OSM is limited to 64 MiB. XML entity/DOCTYPE declarations and Overpass error remarks are rejected. Missing node references on supported highway ways produce an error.
- Matching uses local metric distance to line segments, not just OSM nodes. It retains at most three distinct-way candidates within 30 m. `ambiguous` means the second candidate is within 5 m of the closest distance; `candidate` is not an acceptance decision. Split ways and junctions can create ambiguity even on one physical road.
- Supported highway classes include roads, tracks, cycleways, footways, paths and steps. Raw bridge/tunnel/layer/access/bicycle/oneway/surface tags accompany candidates. No suitability or legal-access inference is made.
- Context lines are sampled at intervals no larger than 10 m, draped on terrain and raised 0.5 m for display. No lines bridge missing raster samples. Geometry outside raster coverage is omitted. Bridge/tunnel ways are omitted from terrain-draped display and listed, because bare earth is not their road surface. These ways remain eligible as diagnostic candidates.
- Long segments over 10 km are omitted from display. Output is capped at 200,000 line vertices and 16 MiB for combined preparation. All context is loaded at once; this is not streaming or final road geometry.
- No smoothing, source-gap repair, route connectivity, lane modelling, building mesh generation or terrain correction is implemented here.

`osm-report.json` contains candidate alternatives, source/input hashes, thresholds in the warnings, summary and attribution. The combined `preparation.json` also contains the display geometry. Original route and terrain points remain unchanged and `trainer_ready` remains false.

## Pipeline comparison status

Update: the baseline is now executed; see [RPV converter comparison](m3-converter-comparison.md) for actual metrics, the encoding workaround, roof errors and recommendation. The table below records the current integration boundary.

| Question | Custom preparation path | OSM2World |
|---|---|---|
| Actual RPV data run | Terrain and OSM context rendered | GLB rendered; roof errors remain |
| USGS elevation | Local F32 raster imported and compared | Integration needs evaluation |
| Authoritative corrected road profile | Still to implement | Must assess how to control/replace generated roads |
| Buildings, intersections and materials | Not implemented | Candidate converter to evaluate |
| Rider dependencies | Prepared data only; authoring tools separate | Prefer baked assets if selected |

The [official download page](https://osm2world.org/download/) offers release 0.4.0 and a latest build. No Java executable was found on PATH or in the usual Java installation directories, and no OSM2World archive is present locally. The documented [command-line workflow](https://wiki.openstreetmap.org/wiki/OSM2World#How_to_use_OSM2World) requires Java and accepts OSM input; verify the actual downloaded release's help, bundled licenses and asset terms before running or redistributing its output. The repository's current license must not be assumed to describe every older release or texture asset.

The initial conversion comparison is recorded separately. Terrain/profile integration and production-pipeline acceptance remain open.

### Portable comparison tools

Run this from a normal network-enabled PowerShell in the project directory:

```powershell
python tools/acquire-osm2world.py local/osm2world-tools
```

The tool downloads the [official OSM2World 0.4.0 archive](https://osm2world.org/download/files/0.4.0/) (approximately 431 MB) and a Windows x64 Temurin Java 17 JRE archive. It uses the [Adoptium archive workflow](https://adoptium.net/installation/archives) and release API, verifies Java's published SHA-256, and records the resolved Java release and both archive hashes in `tools.json`. OSM2World's hash is recorded for reproducibility; no separate publisher checksum was found in the release directory, so it is not marked publisher-verified.

Everything stays under the new output directory, with archives and bundled license files retained. System PATH/JAVA_HOME and installed applications are not changed. The script does not execute downloaded tools. Extraction rejects escaping paths, Windows reserved names, symlinks and duplicate paths and bounds total extracted size. An existing directory is refused; failures leave an error manifest and partial downloads for inspection. Use a new directory for another attempt. Allow disk space for both archives and extracted files.

The agent attempted acquisition into `local/osm2world-tools-agent-attempt`; it failed at the Java metadata request with Windows network error 10051. The normal-shell download is needed before comparison can proceed. Three extraction tests passed; successful live archive acquisition/extraction is still pending.

Once the tools are available, inspect the actual archive's licenses, launcher, `--version` and `--help`, then run the same checksum-pinned `local/rpv-sources/map.osm` into a separate output directory. Capture conversion logs, wall time, output bytes, mesh/material counts, coordinate origin, elevation behavior, and missing/problematic objects. First evaluate scenery quality separately; a default flat conversion must not be represented as a validated USGS-terrain comparison. Then assess terrain/profile integration and runtime asset import before selecting the pipeline.

## Verification

Four OSM-context tests pass: segment-interior distances and alternatives, incomplete/error input and unmatched cases, terrain sampling/structure omission/missing coverage, and source-checked exclusive bundle creation with unchanged activity points. Three existing acquisition tests also pass.

The real RPV context built and the Godot rendered run exited 0 with `SCENERY_PREVIEW_OK`. The screenshot at `local/rpv-osm.png` was inspected: terrain, gold OSM centrelines, cyan activity profile, candidate readout and attribution are visible. This verifies the context renderer, not correct route matching or M3 completion.
