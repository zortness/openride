# M3 riding presentation

The streamed RPV section now has an optional rider-height presentation. Launch from the repository root:

```powershell
./tools/preview-riding.ps1
```

The default local package is now `local/content/rpv-buildings-v2.orpack`; see [coastal presentation](m3-coastline.md) for water, the terrain backdrop, and look-around controls. Play advances at the selected simulated speed; the slider seeks along the route. The camera sits 1.65 m above the route and follows a direction sampled from 3 m behind to 12 m ahead. The compact HUD shows simulated speed, distance, grade, and unavailable power. No trainer is connected by this preview.

The original inspector and orbit streaming preview remain available. This presentation uses the same bounded loader: nine live chunks, three cached payloads, one worker read, and one mesh construction per frame.

## Road and terrain

Build a new package with:

```powershell
python tools/chunk-scenery.py export/rpv-m3-road-route/preparation.json NEW_DIRECTORY --riding-surface
./tools/preview-riding.ps1 -Package NEW_DIRECTORY
```

The option creates a connected six-metre asphalt ribbon with shared vertices at bends. The road uses normal depth testing. Terrain vertices within 18 m of the route are placed 0.25 m below the nearest route segment, blending back to source terrain by 30 m. This broad corridor accommodates the coarse terrain triangulation. Manifest presentation metadata records this adjustment.

This is illustrative earthwork, not surveyed terrain or a route-elevation correction. Original inputs and every route point remain unchanged. Bridge/tunnel handling, intersections, sharp hairpins and more accurate shoulders require further work; this treatment is demonstrated on the selected RPV road section. The riding launcher rejects packages without the corridor flag.

Procedural sky, lighting and atmospheric haze supply basic context. Distant terrain, buildings, vegetation, road markings and surface textures are still pending. The exported rider executable has not been changed.

## Verification (2026-10-10)

- 32 Python tests passed, including shared road joins and corridor interpolation/source preservation.
- Rendered full-section traversal and rapid seeks exited 0: 48 loads, 39 unloads, peak nine live chunks and three cached payloads. Camera height and finite transforms checked every 25 m.
- Maximum observed per-chunk mesh construction: 3.399 ms; neighborhood update: 0.512 ms. These are component timings, not M3 performance acceptance.
- Mid-route screenshot inspected: normal-depth road visible, no diagnostic lines or marker, rider camera and compact HUD.
- All 417 exported route points equal the source corrected road profile. Terrain clearance at 862 road-edge vertices (including chunk duplicates) was positive, minimum 0.120 m. This sample check is not a general triangle-intersection proof.
- Package: 90 chunks, 6,658,541 bytes; 130,050 terrain triangles and 832 road triangles.

Next: review the riding perspective, add distant terrain/basic scenery, integrate streamed package discovery into the exported client, and measure complete frame times and actual memory usage.
