# Continuous corridor packages

`assemble-corridor.py` builds one streamed `.orpack` from verified regional terrain
acquisition. It rebuilds the road and terrain in one common frame; it never joins
already-baked regional meshes. Existing regional packages remain unchanged.

```powershell
. ./build/Use-Environment.ps1
python tools/assemble-corridor.py local/my-corridor local/my-content/full-route.orpack --builder target/release/content-builder.exe
```

All planned regions need verified terrain receipts. OSM acquisition is not required
by this terrain/recorded-road assembler and is not presented as integrated scenery.
If the retained profile has >15% grades, review them first; `--accept-steep` explicitly
keeps those elevations in the preview package. It does not repair or flatten them,
or establish hardware acceptance. `content-builder --review-audit` helps inspect
their context and exercise the profile with software physics.

For incremental verification, `--regions 3` assembles a continuous prefix ending
at the third region boundary. The package name and provenance identify its actual
coverage; a prefix is not labeled as a full route. Choose a new destination for
each assembly. Source review changes require a new corridor plan.

If differing DEM sampling grids produce a terrain-overlap rejection, inspect the
reported disagreement and local source-cell ranges. `--reconcile-overlaps` enables
edge-weighted feathering of the derived display terrain. Disagreement may then
exceed 2 m only within the observed local cell-height variation, with an absolute
10 m cap. Flat unexplained offsets still fail. Provenance records the policy,
maximum spread, largest change from the first source, and reconciliation count.
This never edits the saved road elevations or original DEM files.

## Continuity and bounds

- A spherical orthographic frame centered on the recording keeps all regions in
  one coordinate system. Canonical distances remain geodesic; the runtime verifies
  each geographic position against the declared projection and each station step
  against its geographic distance. The legacy local-coordinate validator remains
  in use for older packages.
- The saved route profile stays sparse and unchanged. Road geometry alone is
  subdivided to at most 10 m between vertices. One ribbon spans regional boundaries.
  Recorded reversals use coincident end caps instead of an undefined miter, keeping
  route occurrences, stationing and elevation intact.
- Terrain is resampled onto one 8 m lattice aligned to 256 m runtime cells. A stable
  source order gives each vertex one height. Overlap disagreements above 2 m fail
  by default; explicit bounded reconciliation uses the policy above. Every grid square belongs to one spatial chunk; shared edge
  heights and supplied normals are identical. Road triangles use centroid ownership.
- Presentation-only terrain conformance uses the whole route, including segments
  on both sides of regional boundaries. Bridge/tunnel/stacked-road ground handling
  remains unresolved. Recorded road heights are never changed by terrain sampling.
- Assembly supports routes up to 200 km, 10,000 canonical points, a 99 km projection
  radius, and 32,768 road mesh stations. Existing runtime bounds remain: 4,096 chunks,
  2 MiB per payload, nine live chunks and three cached chunks. Full route metadata
  remains resident; this is not unbounded profile streaming or a minimum-hardware
  performance acceptance result.

The assembler retains at most eight decoded DEM grids and 16,384 sampled heights
in memory, writes each chunk separately, and stages output before publication.
It revalidates the pinned review, source checksums, terrain receipts and tool hashes
before publishing. A failed run leaves its private `.openride-assembly/` workspace for
diagnosis, never a partially published package. Acquisition is resumable; assembly
currently restarts in a new staging workspace rather than adopting partial chunks.
The route library excludes that reserved staging directory.

Temporary acquisition HTTP 408/429/5xx failures and interrupted responses retry the
same request up to three times with bounded backoff. Permanent HTTP errors and
oversized responses fail immediately. Previously verified terrain is reused.

## Verification

Synthetic tests cover long-route stationing, projection round trips, reversal caps,
shared edge heights/normals, triangle ownership, missing terrain, stale-input
publication rejection, and download retry limits. `verify_corridor.gd` rejects
mutated route positions, geography, stationing and projection metadata. The normal
exported `--verify-stream --riding-view` traversal checks the assembled package's
loading and unloading bounds. Inspect rendered boundaries as well as numeric tests.

The manifest stays an unvalidated preview. This feature does not close M7's OSM,
imagery, mountain-route, performance or supervised trainer/endurance gates.
