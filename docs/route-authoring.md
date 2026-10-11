# Route authoring in the desktop editor

Open **Create** from the main navigation. A project is a local directory containing
`project.json`, downloaded sources, review documents, build outputs and a job log.
Use **Open project** to resume it. Projects and riding packages are separate:
keep the project for editing; distribute the `.orpack` directory for offline riding.

Authoring and 3D review now use the main application window. **Back / Escape**
returns to the preceding screen; reopening an unchanged review preserves its draft,
point selection and camera. The last project is remembered. Background jobs continue
when navigating away. Switching projects prompts before discarding unsaved edits.
Controller navigation uses the D-pad/left stick, south-button confirm and east-button
Back. Shoulders switch inner tabs when present, otherwise main sections. In the
editor, D-pad moves focus, left stick orbits, right stick pans, triggers zoom, left-stick click focuses the
point, right-stick click toggles tools, west button removes a point and north button
undoes a removal. File-dialog shoulders cycle through fields and Open / Cancel;
D-pad browses files. Text entry/file paths currently require desktop input.

Reopening restores the imported filename/path and section interval. New imports
retain both the original input file and the normalized route inside the project,
so moving the external source does not lose the import. Older projects retain
their existing normalized source and filename where available.

Each project creates and maintains **corrections.json** and **repairs.json**.
Opening either review tool loads the saved edits automatically. Save buttons
commit to the project without a file picker; new removal spans are added to the
existing set and validated together. Failed saves leave accepted documents and
project state intact. Internal stage snapshots retain prior versions. Existing
projects migrate on open; differing legacy root documents are backed up before
the accepted state is published at those fixed paths.

Tabs mark available/completed stages, and each stage shows its saved status.
Repair reports saved removal spans and points that keep their original positions;
default points are valid and do not require road selections. Changes that invalidate
a preview return its status to “needs rebuild.”

## Workflow

1. **Import:** choose an empty project folder and a GPX track, TCX activity, KML
   LineString, or `.orroute`. Select a distance interval up to 5 km. For precise
   sample selection, use Route studio's existing import/export tools first.
2. **Map sources:** acquire USGS 3DEP terrain, then OSM streets, buildings and land
   cover. These are explicit network actions. Streets and buildings share one OSM
   snapshot so their provenance and geometry remain consistent.
3. **Imagery:** choose 512, 1024 or 2048 pixels and acquire aerial colour. The larger
   images are mosaics of 512-pixel requests, not enlarged versions of the original
   image. A failed request retains completed tiles. Retry the same quality to
   resume. Imagery is optional; OSM ground materials remain available.
4. **Repair:** open **Edit route / road matches / remove points**. Choose road
   matches, adjust heights and remove points in the same draft. **Save route edits**
   saves both road choices and removals together. Road edits update the gold repair
   preview immediately; if a connector becomes invalid, its removal intent remains
   visible and the editor reports the conflict. No correction/repair file picker is
   needed. Reopening older projects refreshes road alternatives from cached OSM
   without a new download, retaining accepted choices and removals.
5. **Build and preview:** choose connected road geometry or corrected GPS points,
   and corrected recorded heights or explicit terrain heights. Build the preview,
   inspect it, then confirm and add it to Offline routes. Installation creates a
   new package; it does not overwrite an older version.

When a step fails, read its message and the project's `job.log`. Successful stage
paths are saved only after completion. The next attempt reuses verified source
files or imagery tiles. Bounds, origin and source hashes bind caches to their
project. A different route/interval requires a new project. Repeating a completed
map acquisition preserves repairs when its snapshot is unchanged. Road review
reopens the latest saved choices and removals. Reapplying corrections carries
accepted removal spans forward through the full repair validator and invalidates
the build. If a span now conflicts, the previous project state is retained and
the conflict is reported. Old outputs remain on disk. Closing the authoring window hides it; a running job continues. Reopen
the project to observe it after restarting the application.

In the repair viewer, **Shift + left drag** or **middle drag** pans the camera's
target. **F / Focus point** centers the selected point and brings it closer; the
wheel zooms to a minimum 2 m orbit radius. Normal left drag still orbits. Under
**Show tools**, **Next original-position point** cycles through retained points with
no road choice, skipping removed points. These defaults are valid: they retain
their recorded positions and reviewed heights. Road mode follows connected OSM
geometry through consecutive selected points and connects to default GPS points.
An isolated road selection retains its snapped position. Street propagation is
optional. Invalid selected road sections still report errors; source sample
indices are zero-based, while the viewer's point counter is one-based.

In source-point mode, **Delete / Remove current** previews removal of the slider's
selected point. **Ctrl+Z / Undo removal** reverses draft point/range removals in
the current editing session, including repeated removals. The buttons are under
**Show tools → Repairs**. Adjacent deletions merge into a connector between
surviving neighbors. Removing the first or last point trims the route; at least
two points must remain. Manual interior removal allows default surviving neighbors;
OSM way IDs may differ across intersections. It requires
a 1–100 m connector and at most 25 m deviation.
Removed points need no road choice. Automated drift repair retains its stricter
10 m/all-points-matched checks. Text fields keep their normal keyboard behavior. **Save route edits**
commits the draft to the project; closing without saving discards it. This undo
history covers draft removals, not road choices or previously committed sessions.

### Stop and jitter suggestions

In **Show tools ? Repairs**, click **Find stops / GPS jitter**. This saves the
current draft, reads timestamps from the imported recording and opens suggestions
in the same panel. Select a candidate, preview it alongside existing removals,
then save selected repairs. Detection never applies removals automatically.
Changing road choices or manual removals invalidates suggestions; scan again.
No candidates means manual cleanup may still be appropriate. Recordings without
timestamps cannot establish a stationary interval. Older projects need access to
the matching original activity; normalized route geometry alone is insufficient.

Road dropdowns include all OSM highway ways within 30 m, including alternatives
beyond the nearest three. Ways can be split at intersections, and separate paths
may be closer to historical GPS than the street. Review the visible alignment.
Explicitly selected paths with outdoor `access=no/private` or `bicycle=no/private`
tags are allowed for indoor simulation. These tags remain in generated profile
metadata and the build log; they do not block scenery generation. Connectivity
and elevation validation still apply.

If selected roads do not share OSM nodes, Build preview lists the affected point
pairs and gap lengths. **Connect short gaps between selected roads** in step 4
explicitly permits straight joins up to 100 m between those sections. It is off
by default and remembered per project. Connected sections still follow selected
road geometry; the builder does not choose unselected streets. The build log and
generated profile list the joins for preview review. Recording breaks, excessive
gaps and invalid grades still require repair.

**Remove slider point N (Del)** affects only the point currently selected on the
bottom slider. **Preview selected range** uses the inclusive **From / Through**
fields; those fields do not change the Delete key's target.

With the gold repair shown, the slider skips removed samples in both directions
and advances to a surviving neighbor after deletion. The original cyan route is
hidden; the gold route and small retained-point markers show the current repair.
A message beside the slider reports the removed point and retained count, or the
reason a deletion was rejected. Turn off the repair overlay to inspect the
original samples for comparison. Old generated-route playback is disabled after
editing until the route is rebuilt.

The 5 km limit is an initial authoring budget: one geographic rectangle and a
fixed 256 x 256 elevation raster per project, bounded OSM data and geometry
budgets. Runtime chunk streaming does not remove those import constraints.
Longer routes require tiled terrain/OSM acquisition and segmented preparation
while preserving continuous route stationing and review decisions.

## Existing RPV route

`local/rpv-route-project/project.json` is a working copy of the existing RPV
preparations, saved sources, corrections, repaired profile and real 512-pixel
imagery. It was rebuilt through the exported app. The original files are retained.

Open this project, select **3 Imagery**, acquire 2048 x 2048, then select **4 Repair →
Build preview** with road geometry and corrected recorded heights. No new road
selection or elevation offset is needed. Inspect the result in **5 Preview** and
add it to Offline routes if satisfactory. The download is the remaining live
service check: this agent environment returned WinError 10051 before reaching
USGS. The existing 512-pixel image remains selected until acquisition succeeds.

## Scope and limitations

- Windows authoring currently requires Python 3.10+ installed locally. Its path
  can be changed in Settings. The exported `authoring` folder includes the Python
  tools and native `content-builder.exe`. Riding does not invoke Python.
- Terrain and imagery providers are US services with location-dependent coverage;
  OSM coverage varies. A higher resolution request cannot invent source detail.
  Photography dates remain unresolved across the mosaic. Each tile retains
  download provenance, hashes, bounds and attribution.
- KML import supports LineStrings and segment boundaries. Only absolute KML
  altitudes become recorded heights; clamped/relative values remain missing.
  Missing heights can be displayed draped over terrain for review without changing
  the source, and explicitly projected during building. KMZ, NetworkLinks,
  gx:Track, binary FIT import and TCX Courses are not yet supported.
- Recorded-height mode includes any reviewed offset. Terrain-height mode replaces
  heights with sampled ground elevations; it does not add the recorded offset.
  Road mode interpolates those anchor heights along the connected road. This is
  not surveyed road elevation and is unsuitable for bridges/tunnels; detected
  nearby structures or missing terrain samples prevent projection.
- Corrected GPS mode retains the reviewed point alignment, including any explicit
  per-point road snaps. Road mode reconstructs connected OSM geometry for selected
  runs and retains GPS positions elsewhere. Transitions are straight connections,
  so inspect them in the preview. Invalid selected road sections do not silently
  fall back to GPS. Gaps and missing heights block builds; steep grades remain
  validation findings. Dense continuous road routes are the initial scope.
- Preview confirmation is a user's review, not a claim of surveyed accuracy.
  Packages remain marked unvalidated and the existing trainer limits apply.

## Verification

Tests cover tile orientation, interrupted-download resumption, cache identity,
separate terrain/OSM retries, stage failure persistence, terrain-height stationing,
and installation review/versioning. KML import is tested in Rust and through the
native builder. `--verify-authoring --authoring-project=...` exercises the exported
app's background build and embedded review viewer against the saved RPV project.
