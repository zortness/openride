# M7 tiled section builder

This first S03 implementation builds a continuous section up to 12 km from a
saved full-ride review, using several terrain source tiles and one shared route
profile. It produces the existing streamed `.orpack` format. The whole-recording
builder, OSM corridor processing, imagery and distant terrain remain later work.
The original Create workflow retains its 5 km restriction. The newer
[corridor job runner](m7-corridor-preparation.md) orchestrates resumable regional
preparation across a complete recording; unified full-route assembly remains open.

Accepted local repairs from the sparse preview feed this same native profile.
Inspect and save each repair explicitly, then create a new plan to bind the new
review revision. Previewing a proposal alone does not change builder output.

## Use the prepared OC section

In the updated Windows app, open **Ride → Choose folder**, select
`local/oc_to_solana/content`, then select the **0.0–8.0 km tiled section** and
choose **Preview selected route**. It contains real USGS terrain and the recorded
route ribbon. It has no imagery, buildings or OSM road alignment. This is a
section proof, not the entire OC recording or a new hardware acceptance result.

## Prepare another section

First select geometry/elevation sources and save decisions in Full ride review.
The current profile requires both roles to use the same recording. Preserve that
review directory; all outputs below go into new, private directories.

```powershell
python tools/tiled-scenery.py plan local/my-review local/my-tiled-section --builder target/release/content-builder.exe --start 0 --end 8000
python tools/tiled-scenery.py acquire local/my-tiled-section
python tools/tiled-scenery.py build local/my-tiled-section local/my-content/section.orpack --builder target/release/content-builder.exe
```

Create the package's parent content folder first. Discover Python on PATH and
build the native tool using `tools/build.ps1 -Release -Offline`. The export also
includes the scripts and builder under `dist/authoring`. No terrain is downloaded
during planning or package playback. Acquisition explicitly contacts USGS.

Planning uses `content-builder --review-profile REVIEW NEW_JSON` to reopen and
hash-check the saved review. It reports remaining full-recording findings and
rejects unresolved breaks inside the selected interval. The first OC 8 km is
continuous even though findings remain later in the ride. Planning interpolates
the section endpoints, retains original vertices and adds points at most 10 m
apart. Source samples, original route stations and heights remain attributable.
The current runtime projection tolerance and 10,000-point limit remain enforced.

Inspect `plan.json` before accepting its `steep_segments`. Grades above 15% are
reported, not flattened. Building such a section requires `--accept-steep`, which
records the explicit decision. Grades above 100% still require source repair.
The synthetic mountain proof exercises this option; the OC first 8 km needed no
steep-grade override.

## Terrain and publication rules

- Source tiles have 1 km cores, 40 m overlap and saved USGS F32 raster metadata.
  The terrain mesh uses a shared 10 m lattice within roughly 150 m of the route.
  Raster resolution does not establish source accuracy.
- Shared vertices are sampled and stored once. Deterministic source ownership
  avoids separate filtering on opposite sides of a boundary. Overlap disagreement
  above 2 m fails the build; missing DEM data also fails rather than filling holes.
- The route ribbon is generated once for the section. Tile reuse does not merge
  repeated route occurrences or heights on stacked roads.
- The existing presentation-only terrain corridor adjustment is applied after
  mosaicking. Recorded road elevations are unchanged. This is not vertical-datum,
  bridge/tunnel or stacked-road terrain reconciliation; visual review is required.
- The bounded proof allows at most 40 source tiles / 100,000 terrain vertices.
  Existing preparation, package, chunk and profile limits are retained. Source
  grids and the assembled section mesh remain resident during building, so this
  is not the final full-corridor memory architecture.
- Acquisition validates and reuses cached files. Retry the same acquisition
  command after a service failure. Full build-stage resume/cancellation is S04.
- Builds use a new private staging directory. Payloads and checksummed
  `terrain-provenance.json` are completed before renaming to a new package path;
  an existing package is never replaced. Failed build directories remain in the
  private project for diagnosis. Publishing across filesystems may require choosing
  a destination on the same volume.

The runtime checks the provenance payload's path, size, checksum and schema.
Copy the complete `.orpack` directory when installing it. Terrain source/license
records and the saved review revision travel with the package; original recording
files stay in the private authoring project. Packages themselves still contain
personal route geography.

## Verification and remaining acceptance

The synthetic fixture is reproducible without network access:

```powershell
python tools/fixtures/tiled-terrain.py local/summit-proof target/release/content-builder.exe
python tools/tiled-scenery.py build local/summit-proof/project local/summit-proof/summit.orpack --builder target/release/content-builder.exe --accept-steep
```

Synthetic tests cover a 7 km climb/summit/descent with ±20% grades, planar seam
equivalence, unique shared vertices, hairpins/repeated XY positions with distinct
heights, missing DEM, conflicting overlap and unresolved source breaks. The
packaged summit traversed across 58 chunks in the exported app.

The OC 8 km proof uses 16 source tiles, 85 chunks and 884 route points. Package
size is 2,112,877 bytes. Shared-source differences peaked at about 0.036 m; the
rendered traversal kept at most nine live chunks and three cached chunks. These
are observations from this bounded check, not full-route FPS/RAM/VRAM acceptance.

Use isolated APPDATA, hidden bounded processes and redirected logs for automated
Godot checks. `--stream-package=ABS_PACKAGE --verify-stream --riding-view` after
`--` traverses the package without trainer I/O. Rust workspace tests, 73 Python
tests, exported library/power-demo checks, short-route UX regression and rejection
of corrupted provenance passed. The OC rendering was visually inspected.

S03 remains open for a reviewed Death Ride mountain section and user elevation/
repair acceptance. The four OC continuity repairs are now accepted. Corridor-wide
planning found seven additional raw-grade blockers; resolving those and completing
unified package assembly precede a complete 153 km package.
