# Automatic elevation repair during import

The full-recording import workflow automatically repairs bounded elevation noise
when you save matching geometry and elevation source selections or gap decisions.
It records each change as an accepted `automatic_grade_blend` in the saved review.
The save result reports the applied count. Open **Full-route preview** to inspect
before/after elevations and grades, reject a repair, or undo its decision. Later
saves preserve rejection and undo; they do not reapply that candidate.

Original activity files and snapshots remain unchanged. Repairs affect the native
profile shared by preview and content building. Simply opening a saved review or
exporting comparisons never applies new changes. Source selection remains explicit;
different geometry/elevation variants require alignment before this repair runs.

## Conservative limits

- Candidates start at continuous segments with absolute grade above 100%, horizontal
  movement of 0.01–1 m, and height change no greater than 2 m.
- Nearby candidates within 20 m are grouped. Unchanged anchors must extend at least
  5 m beyond both ends of the group.
- The full blend spans at most 100 m and 32 source-sample intervals. Missing samples,
  unresolved boundaries, and overlap with continuity repairs exclude a candidate.
- Elevation is interpolated by horizontal route distance. No sample changes by more
  than 2 m, and the resulting anchor-to-anchor grade must be at most 15% in magnitude.
- Geometry, stations, and source files are preserved. Larger errors and genuine
  steep climbs remain for review; this is not general smoothing or terrain matching.

Rules are versioned in the proposal identity (`grade-v1`) and bound to source hashes.
Accepted proposals must match a fresh native calculation. Saving uses the existing
revision check, exclusive lock, and atomic replacement. Up to 128 automatic groups
are generated per review; remaining findings still block preparation as appropriate.

## Command-line use

After building the native tool, inspect candidates without changing the review:

```powershell
target/release/content-builder.exe --review-repairs local/my-review local/comparisons.json
```

To run automatic repair on an existing review, pass the exact revision reported in
that comparison export (or the SHA-256 of `review.json`):

```powershell
target/release/content-builder.exe --repair-review local/my-review REVIEW_REVISION
```

A stale revision fails without modifying the saved review. Export the repaired
profile or create a new corridor project afterward. Existing packages and corridor
jobs remain pinned to their old review revision and are not overwritten.
