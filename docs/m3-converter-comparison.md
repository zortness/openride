# RPV converter comparison

Status: baseline comparison executed on the same saved OSM snapshot. The OSM2World result loads in Godot, but is not a validated rideable world. Terrain integration, corrections, coverage and performance acceptance remain open.

## Inspect the result

```powershell
./tools/preview-osm2world.ps1
```

Drag to orbit and scroll to zoom. `-Zoom 1` shows the full extent; the default is 3. This separate diagnostic viewer loads the trusted local converter GLB at runtime. It is not the final community-package loader. The existing USGS/route viewer remains available with `./tools/preview-scenery.ps1 -Preparation export/rpv-m3-osm/preparation.json`.

The model is `local/osm2world-rpv-verified/rpv.glb`; the measured run and source/tool hashes are in the adjacent `comparison.json`. Screenshots inspected: `local/osm2world-rpv.png` (overview) and `local/osm2world-rpv-close.png` (closer view). These initially used the preceding UTF-8 run of the same conversion; the reproducible wrapper output was also loaded and rendered afterward.

## Actual run

- OSM2World reports version 0.4.0. Portable Java reports Temurin 17.0.20.1+1. The archive's `LICENSE.txt` contains LGPLv3; do not substitute the current repository's MIT license for this release's bundled license. The standard properties file carries a CC0 dedication; the full texture/model dependency inventory remains to be established before public redistribution.
- Input is the unchanged downloaded `map.osm`, SHA-256 `6385bb87bbbae24e74eff1814148359c62525f93fa983dac86d1f141cba7431b`.
- Parameters: LOD 2, bundled `standard.properties`, 2 GiB Java heap cap, explicit UTF-8. The bundled style uses `locale=DE`; localized signs/markings have not been validated for California.
- USGS terrain was **not** supplied. The converter reports 0.0 seconds in its elevation phase. The model's vertical extent is only 15.6 m, consistent with flat ground plus structures; it does not reproduce the 275 m terrain range in our buffered DEM.
- Wall time: 19.42 seconds. Converter-reported internal time: 18.957 seconds. These are observations from this development run, not performance acceptance on the 16 GB / 8 GB reference configuration.

| Output measurement | Result |
|---|---:|
| GLB bytes | 46,157,572 |
| Meshes / Godot mesh instances | 4,228 |
| Triangles in triangle primitives | 86,833 |
| Materials | 23 |
| Images / textures | 39 / 39 |
| Nodes | 5,941 |
| External image references | 0 |
| Godot bounds X × Y × Z | 3,325.6 × 15.6 × 3,827.9 m |

The model uses a different local origin from OpenRide: Godot bounds begin at approximately (-6854.8, 0, -5270.0). No coordinate alignment has been inferred from visual resemblance. OSM member ways can extend outside our raster bounds; the converter extent is larger than the terrain subset. A future integration needs an explicit geographic transform and coverage clipping.

## Failures found

1. **Windows encoding:** the initial run generated a GLB with invalid UTF-8 in its JSON chunk. Repeating with `-Dfile.encoding=UTF-8` produced parseable JSON and a Godot-loadable model. The wrapper always sets this flag.
2. **Roof generation:** the converter exits 0 but logs three `ERROR:` entries involving `HeightfieldRoof` null height values and suppressed attachment-surface errors. These do not establish three distinct failed buildings. Missing/partial objects have not been localized yet. The wrapper records `converted_with_errors` and exits 1 so this cannot be mistaken for a clean conversion.
3. **Ground context:** the rendered result shows streets and many buildings, with sparse ground coverage in this run. It supplies useful object geometry, but not our terrain or a corrected riding surface.

## Recommendation

Keep the corrected route profile and terrain under OpenRide's control. Those are authoritative inputs to rendering, simulation and trainer grade, and the current custom preparation already preserves their provenance and exposes discrepancies.

Retain OSM2World as a candidate **offline scenery converter** for buildings, street details and materials. Do not adopt its complete output as the riding surface yet. Integration would require explicit coordinate conversion, clipping/chunking, terrain placement that preserves building shape, road replacement or exclusion, error localization and asset-license inventory. Thousands of mesh instances also need a measured batching/streaming strategy; no runtime performance conclusion follows from successful import alone.

The next implementation priority is source-bound correction records and a shared corrected road profile, with explicit road choices where footways and carriageways compete. Keep raw measurements visible. A ~16 m constant elevation shift may be a useful authoring experiment, but is not validated by the current DEM comparison and must not be silently applied or sent to the trainer.

## Reproduce

Use a new output directory:

```powershell
python tools/compare-osm2world.py local/osm2world-tools local/rpv-sources NEW_OUTPUT_DIRECTORY
./tools/preview-osm2world.ps1 -Model NEW_OUTPUT_DIRECTORY/rpv.glb
```

The wrapper verifies the OSM source checksum, records the JAR/config hashes, uses the converter's own working directory, retains stdout/stderr and compressed converter logs, imposes a five-minute timeout, checks GLB header/UTF-8 JSON, and measures model counts. It returns nonzero for conversion/log errors, preserving `comparison.json` and any generated artifact for diagnosis. It does not modify the downloaded map or the existing OpenRide preparation.
