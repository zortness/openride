# Coastal riding preview

Run `./tools/preview-riding.ps1`. The coastal-only package is `local/content/rpv-coastal-v3.orpack`; the default launcher now uses the [building preview](m3-buildings.md), preserving this coast.

- Hold the right mouse button and drag horizontally to look toward the ocean.
- Press **F** to face forward again.
- Play and distance scrubbing still use simulated speed; no trainer connection is made.

## Content preparation

```powershell
python tools/chunk-scenery.py export/rpv-m3-road-route/preparation.json NEW_DIRECTORY --riding-surface --coastline-osm local/rpv-sources/map.osm
./tools/preview-riding.ps1 -Package NEW_DIRECTORY
```

The coastline input must match the preparation's OSM checksum. The builder joins directed shoreline ways and clips the resulting chain to the DEM footprint. It supports one open, unbranched shoreline crossing that footprint. Missing nodes, disconnected chains, islands, multiple crossings, interior endpoints, or a water mask covering route points require further authoring work and are rejected.

Water-side orientation follows the [OSM coastline convention](https://wiki.openstreetmap.org/wiki/Tag%3Anatural%3Dcoastline): land left and water right along the directed way, accounting for our inverted north/Z axis. The shoreline represents a mapped high-water boundary; this preview does not simulate tides.

The water polygon follows the supplied shoreline within the DEM extent. Outside that extent, its endpoints extend perpendicular to the footprint boundary and close on a rectangle expanded by 12 km. Those extensions are visual approximations, not additional mapped coastline. Ocean-side DEM vertices are lowered beneath the water so source raster values do not render as green ocean. Shoreline contact is still limited by the approximately 8–10 m terrain sampling; it is not exact shoreline triangulation.

Nominal sea elevation is 0 m in the source elevation frame (`local Y = -origin elevation`, approximately -81.6 m for RPV). The vertical datum remains unvalidated. The road's accepted +16 m correction is not applied to water, and all 417 route-profile points remain unchanged.

## Distant terrain and runtime bounds

A 4,225-vertex / 8,192-triangle backdrop samples every fourth source-grid vertex across the existing DEM footprint. It sits 4 m below the nearby mesh; a shader suppresses its interior beneath the loaded neighborhood, retaining overlap around the boundary. This extends visible terrain beyond nearby chunks, but does not acquire terrain beyond the original roughly 2 × 2.5 km coverage. Larger-area DEM acquisition and a more general LOD transition remain future work.

The package adds one fixed-path, checksummed `coast.json` payload (367,073 bytes). It contains the backdrop and water polygon, source identity, and approximation metadata. Runtime retains two additional meshes: backdrop terrain and 94 water triangles. These are outside the nine streamed-chunk budget, explicitly bounded to 5,000 terrain vertices and 5,010 polygon points; the payload limit is 2 MiB. No network access or extra runtime dependency is needed.

## Verification (2026-10-10)

- 36 Python tests passed, including coastline clipping, water-side reversal, missing/multiple crossings and interior-endpoint rejection.
- Rendered traversal and rapid seeks exited 0: nine live chunks, three cached payloads, 48 loads and 39 unloads. Mid-route ocean-facing screenshots inspected.
- All 92 payload lengths/checksums verified; all route points equal the original generated road profile.
- 19 source ways join into a shoreline with 93 points after clipping. The mask adjusts 18,445 terrain vertices.
- Package total: 7,020,944 bytes. Component timings remain diagnostic, not full frame-time or memory acceptance.

Remaining: broader terrain coverage, shoreline/LOD seam refinement, sea-level validation, surface detail, exported-client integration, and M3 performance acceptance.
