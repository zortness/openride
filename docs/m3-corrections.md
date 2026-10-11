# M3 source-bound corrections

## Manual point removal

Open `export/rpv-m3-corrected/preparation.json`, then **Show tools → Repairs → Remove source points and rejoin**. Set the inclusive **From / Through** source-point numbers shown by the bottom slider. **Preview removal** joins the immediately preceding and following points; gold shows the proposal. **Save manual repair** writes a source-bound repair file for `repair-profile.py`, just like detected repairs. Original measurements are not deleted. Range changes require another preview before saving. This first tool authors one range per proposal, on an unchanged corrected preparation; reopen that preparation to replace a previously loaded repair.

For RPV, removing points 5–31 joins 4 to 32 and leaves one steep-grade finding. Removing 5–32 joins 4 to 33 and reproduces the earlier 53-point manual repair with zero current findings. Endpoint elevations retain +16 m. The source slider still includes original samples, now explicitly labelling omitted points while the gold proposal is shown and hiding their marker. Jitter candidates also use these same local source-point numbers rather than original activity sample IDs.

Stop/jitter detection and review are described in [M3 jitter detection](m3-jitter.md).

Open the refreshed RPV preparation (older OSM preparations lack projected candidate coordinates):

```powershell
./tools/preview-scenery.ps1 -Preparation export/rpv-m3-authoring/preparation.json
```

Move the slider and use the dropdown to select a road **for that point only**. Select **Keep source position** to remove the choice. No other samples snap automatically. Adjust the global elevation offset to experiment with vertical alignment; it defaults to zero. Cyan is the original profile, pink is the draft, and the marker follows the draft. The overlay toggle controls terrain occlusion for both paths.

**Extend road selection both ways** fills consecutive points around the current selection. It accepts the same OSM way, or a unique candidate with the same nonempty street name and highway type across split ways. It stops at source breaks, source/projected gaps over 100 m, missing or ambiguous candidates, and conflicting existing choices. Existing matching choices are retained. **Undo extension** reverses the most recent extension; changing a point or loading another file clears that undo. This is a name-based authoring convenience, not a connected-path solver. Review the result and save it explicitly.

**Save corrections** writes a separate JSON file, for example `export/rpv-corrections.json`. Use a new filename for each revision; existing files are preserved. **Load corrections** restores choices and offset, rejecting mismatched sources or missing candidates without replacing current edits. Edits are not autosaved; opening another preparation discards the draft.

The document stores schema version, kind `openride.scenery-corrections`, exact preparation SHA-256, elevation offset, and source-sample/OSM-way/node-pair choices. The complete preparation hash binds terrain, route, candidate geometry and source snapshot. Even formatting-only changes require review and a new correction document. There is no automatic rebase.

## Offline regeneration

Use Python 3; on this development PC:

```powershell
python tools/correct-scenery.py export/rpv-m3-authoring/preparation.json export/rpv-corrections.json export/rpv-m3-corrected
./tools/preview-scenery.ps1 -Preparation export/rpv-m3-corrected/preparation.json
```

Output must be a new directory. Source conflicts and asset checksum failures are reported before output creation. Original points, geometry, terrain and assets are preserved; the builder copies the corrections and adds `corrected_profile` with points, six-metre road mesh, grades and findings. The original `road-preview.obj` remains an original-profile asset; corrected geometry is in `corrected_profile.mesh`. Apply subsequent correction revisions to the original authoring preparation, not an already-corrected output.

Coordinates come from explicitly selected candidate projections. Heights retain the activity profile plus the offset, without terrain sampling. Grade and road mesh use the same corrected points and horizontal lengths. Breaks, missing elevations, segments under one metre and jumps over 100 metres receive neither geometry nor grade. Distance sums valid segments only. Grades above 15% are flagged. The viewer calculates temporary drafts; the offline builder creates the stored profile, with automated grade parity checking on the RPV fixture.

## Limits and next work

This is an authoring experiment. Choices do not establish connected OSM topology, direction, bicycle access, or bridge/tunnel height. A global offset does not resolve a vertical datum. Sharp transitions between selected and unselected points require inspection. `trainer_ready` remains false; there is no trainer connection.

Next: inspect and save actual RPV corrections; address continuity and vertical-profile review, then packaging, discovery, chunk loading and performance gates. OSM2World remains an optional scenery source per [the converter comparison](m3-converter-comparison.md). The offset and road choice in `local/corrections-test.json` are verification inputs, not accepted RPV corrections.

## Saved RPV correction review

The user's `export/rpv_corrections.json` matches the authoring preparation checksum. All 81 samples select **Palos Verdes Drive West** across seven OSM ways; all corrected heights are exactly original height +16 m. Regenerated output is `export/rpv-m3-corrected/preparation.json`. Original source data and the user's correction file are preserved.

The generated road has 116 triangles. Nineteen sub-metre segments and three original breaks have no geometry/grade under the existing rules. No corrected adjacent distance exceeds 100 m (maximum 76.794 m). Samples 998 and 1008 retain grades of +19.852% and -16.903%. Selection and offset checks therefore pass, but continuity and physical grade validation remain open. Projecting samples onto a road does not prove every straight connector follows the road's full curvature.

Verification: 9 propagation rule checks; Godot real-RPV propagation/undo check; generated-profile grade parity for all 81 points. The rendered corrected profile was also inspected. `trainer_ready=false` remains in force.

## Short-span continuity repair proposal

The user visually accepted the road selection and +16 m offset. Further inspection places both steep grades and all three breaks in a small backtracking cluster, consistent with a stop/GPS drift. This is an inference from positions, not a verified explanation of the original recording.

`export/rpv_profile_repairs.json` proposes replacing the interior of samples 981–1010 with a straight connector. It binds the exact corrected-preparation hash and records the method and reason. The 28 interior samples remain in the original and corrected profiles; only the separate `reviewed_profile` omits them. Endpoint heights stay at original +16 m. The 14.862 m connector stays within 0.483 m of the removed projected positions; it approximates the road curvature rather than reconstructing all OSM vertices.

```powershell
./tools/preview-scenery.ps1 -Preparation export/rpv-m3-repaired/preparation.json
```

Use **Inspect repair** to zoom to the affected area and **Show repair proposal (gold)** to compare against the pink unrepaired profile. The point slider still indexes original samples. Changing road/elevation corrections invalidates and hides the proposal until regeneration.

Regeneration into a new directory:

```powershell
python tools/repair-profile.py export/rpv-m3-corrected/preparation.json export/rpv_profile_repairs.json export/rpv-m3-repaired
```

The tool requires explicit choices on one OSM way, finite existing elevations, a 1–100 m connector, no overlapping spans, and at most 10 m deviation. Those are conservative authoring limits, not proof of road connectivity or elevation accuracy. Unrelated breaks remain untouched. The same retained geometry determines road mesh, cumulative distance, and grade.

RPV result: 53 retained points, 52 connected segments, 1,970.652 m horizontal length, grades -5.484% to +4.485%, and zero findings under the current gap/grade rules. All retained elevations remain original +16 m. This is a review proposal with `validated=false` and `trainer_ready=false`, not a calibration or hardware test. Verification includes 19 Python tests, the actual RPV geometry/grade/height checks, and Godot loading/rendering the proposal. Packaging and streaming remain next after review.
