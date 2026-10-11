# Streamed building preview

Run `./tools/preview-riding.ps1`. The default package is `local/content/rpv-buildings-v2.orpack`. Play/scrub as before; right-drag looks around and F faces forward.

## Build

```powershell
python tools/chunk-scenery.py export/rpv-m3-road-route/preparation.json NEW_DIRECTORY --riding-surface --coastline-osm local/rpv-sources/map.osm --buildings-osm local/rpv-sources/map.osm
```

The optional builder uses the exact OSM snapshot bound to the preparation. Closed simple building ways within 250 m of route samples become beige wall meshes and flat brown roofs. Concave footprints are triangulated; relation members are skipped to avoid filling multipolygon courtyards. Detailed roofs, facades, textures, building parts, and vegetation are not implemented.

Height precedence is a numeric metre height tag, then building levels multiplied by 3 m, then an explicit 6 m assumption. Each accepted way ID, height, and height source is retained in the manifest. Unsupported heights are skipped. Only heights from 2–40 m and footprints no larger than 100 m are accepted. Terrain is sampled at footprint vertices: foundations start 0.5 m below the lowest sample; roofs sit above the highest sample. Ground variation above 8 m is skipped. These are illustrative foundations, not reconstructed grading or retaining walls.

Buildings must clear sampled route points by 6 m. Unsupported/missing footprints, source extent limitations, height errors, and slope exclusions are counted in the manifest. This bounded prototype is not a general city converter.

Walls and roofs are partitioned into the existing 256 m chunks. Large triangles are subdivided to respect the 32 m primitive bound. Each added mesh has the existing 30,000-vertex / 90,000-index limit, and total chunk payloads remain limited to 2 MiB. Runtime uses at most two additional meshes per live chunk; buildings unload with terrain. Buildings have no separate distant LOD yet, so they may appear at the neighborhood boundary.

## RPV verification (2026-10-10)

- 388 buildings: 382 mapped heights and six assumed 6 m heights.
- Skipped: 1,156 outside the corridor, three unsupported footprints, three with steep ground.
- Added 9,924 wall triangles and 4,414 roof triangles. Package: 9,758,822 bytes across 90 chunks plus route/backdrop/manifest.
- 40 Python tests passed, including height provenance, concave roof area under both windings, ground sampling, road clearance and subdivision area/bounds.
- Rendered full traversal/rapid seek passed: nine live chunks, three cached payloads, 48 loads / 39 unloads. Screenshot inspected from the riding view.
- All chunk checksums passed; route and coastal payloads are byte-identical to the reviewed coastal package. All terrain, road and context-line geometry is unchanged.
- Maximum observed chunk mesh build was 9.996 ms and neighborhood update 0.697 ms on the development machine. These remain component measurements, not full frame-time acceptance; adding geometry increases upload work.

Next: modest vegetation/ground appearance, exported-client package loading, then complete frame-time and memory acceptance. Broader source coverage, building LOD and detailed architecture remain follow-up work.
