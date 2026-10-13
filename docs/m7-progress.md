# M7 progress

2026-10-12: initial [resumable corridor jobs](m7-corridor-preparation.md) now plan
the complete accepted OC profile into 20 regions and independently resume terrain,
OSM acquisition and regional baking. The first two regions have verified terrain
and separate packages. Seven additional >100% raw-grade segments block four
regions; the accepted continuity repairs remain unchanged. Whole-route package
assembly, regional terrain seam reconciliation, OSM scenery and S04 acceptance
remain open. Eight new synthetic tests passed (81 Python tests total), along with
locked/offline Rust workspace tests. See the linked document for precise limits.

2026-10-11: S01 review, initial S02 full-route sparse preview and a bounded S03
multi-tile section builder are available. M7 remains open. The [roadmap](m7-roadmap.md)
defines the remaining slices. The newly supplied review under `local/oc_to_solana`
is confirmed to contain the GPX for both source roles and all 13 pause decisions.
The user has now accepted all four local repair proposals, and native profile
export confirms zero remaining breaks or profile issues. Other recording reviews and continuity/elevation
acceptance are still pending, so S01 is not marked accepted.

## Full-ride intake and saved review

Open **Create → Full ride review / compare TCX and GPX**. Import one or more
variants of the same ride, inspect the full map and elevation profile, and choose
geometry and elevation sources independently. The comparison distinguishes
recorded distance from geometry distance excluding source gaps. Sample inspection
shows segment identity, UTC time, elevation and importer diagnostics. Selecting a
gap shows elapsed time, endpoint separation and its diagnostic reasons.

Decisions can be unresolved, keep break, recording pause or missing travel, with
an optional note. Saving them does not modify snapshots. Source choices default
to unselected. The S02 preview can derive short pause connectors as described below.

Each private review directory stores original-byte TCX/GPX snapshots and a
versioned `review.json`. Source hashes and sample indices bind gap decisions;
reopening validates snapshots before applying them. Save checks the opened
revision, holds an exclusive lock and replaces the manifest through a temporary
file. Conflicts and invalid decisions preserve the accepted manifest. Unsaved
edits block opening another review and trigger an exit warning.

Keep these folders private, preferably under ignored `local/`. They contain
personal geography and all original recording fields. Do not commit them. A
failed initial write may leave an incomplete new folder. An interrupted save may
leave `review.lock` or `review.json.tmp`; after confirming all writers have exited,
back up the folder before removing those remnants and reopening the accepted
`review.json`. The UI does not automatically recover interrupted saves.

## Verification

- Three synthetic Rust persistence tests cover independent source/gap identity,
  save/reopen after original removal, stale revisions, invalid decisions, lock
  conflicts, snapshot tampering, duplicate inputs and path traversal rejection.
- Godot integration verifies full-source plots beyond 5 km, explicit source
  choices, saved gap classification, navigation, reopening through a fresh native
  worker, and unchanged originals. Failures exit nonzero in release exports.
- All three private TCX/GPX pairs (Death Ride, L'Étape du California, OC to Solana
  Beach) import and reopen with complete recordings over 100 km; original hashes
  remain unchanged. No real source or gap decisions were selected automatically.
- Locked offline Rust workspace tests and all 68 Python tooling tests passed.
  Windows export, exported synthetic and three-pair review checks, and the existing
  short-project/editor/simulated-ride regression passed (`SOURCE_REVIEW_OK`,
  `SOURCE_REVIEW_PRIVATE_OK`, `UX_OK`, `UX_RIDE_OK`). The first UX run used an
  outdated fake trainer and failed its shift check; rebuilding the fixture from
  current source resolved it. Private logs and screenshots remain under `local/`.

To reproduce the review integration check, run the exported executable with
`--headless -- --verify-source-review --review-root=ABS_NEW_DIRECTORY` using isolated
`APPDATA` under `local/godot-appdata`, hidden execution, redirected logs and a
60-second timeout. The test creates synthetic recordings and requires a new
directory. Optional `--review-inputs=ABS_JSON` takes an array of TCX/GPX path pairs
for private validation; keep that JSON and outputs in `local/`. For a rendered
synthetic screenshot, omit `--headless` and add `--review-snapshot=ABS_PNG`.

The current intake bounds are 1–8 source variants, 64 MiB per source / 128 MiB
combined and 2–100,000 samples per source. Import/save runs in a native worker;
this first overview still retains whole activity arrays in the UI. These are
intake guards, not validated runtime memory/performance budgets. The map is a
schematic local track view, not the canonical long-route projection contract.

## S02: full-route sparse preview

From a saved review, choose **Open full-route sparse preview (no 5 km limit)**.
Geometry/elevation selections must currently use the same source. The native
profile keeps geographic coordinates and cumulative horizontal distance in f64,
uses binary distance lookup, and returns at most 1,024 points around a selected
station (up to ±500 m). Coordinates are rebased before conversion to Godot vectors;
remote discontinuities cannot create global-sized mesh coordinates. Dense sources
may narrow the displayed interval to respect the point bound. A decimated overview
contains at most 513 points and preserves breaks across omitted samples; it is for
navigation, not detailed elevation validation. The full native point index remains
resident; disk-backed profile blocks and large-source performance acceptance remain
follow-ups.

The preview applies only saved recording-pause decisions, with a 5 m endpoint
separation limit, a 1 m height-difference limit and a maximum absolute raw connector
grade of 100%. Co-located samples add no distance and require heights matching
within 1 cm; vertical changes without horizontal travel remain findings. No height
smoothing or flattening occurs. These initial connector limits are conservative
inspection policy, not claims about road quality or trainer safety. All other
breaks remain explicit and add no artificial connector distance.

The slider, elevation overview and ±10 km actions support distant seeks; 30 km/h
playback stops before unresolved breaks and stops when leaving the screen. The
native profile can feed the existing Rust simulation when continuous and within
its existing 10,000-point/grade limits. Missing data prevents that conversion.
This screen does not connect hardware, record activities, install packages or
replace the existing scenery pipeline. It regenerates from the saved review and
rejects a revision conflict rather than previewing stale decisions.

Four Rust contract tests cover >20 km pause bridging, preserved source breaks,
unresolved-gap stops, clipped/bounded coordinates, unsafe pause heights and missing
tail data. The Godot check verifies saving, full-route seeking, playback, navigation,
reopening and unchanged inputs. The 22.13 km synthetic fixture bridges one pause.
A disposable review built from the user's OC GPX reaches 153.119 km with 3,718
retained points, 11 bridged pauses and four retained discontinuities. Test-only
pause decisions were applied to that copy, not the user's saved review. Findings
include a 1 m height change across 0.82 m, a 5.93 m separation, and stationary
height changes. Geographic details and captures remain private under `local/`.

Final verification: locked/offline Rust workspace tests, all 68 Python tests,
Windows export, exported synthetic/OC `FULL_PREVIEW_OK`, `SOURCE_REVIEW_OK`,
`UX_OK` and `UX_RIDE_OK` passed. Rendered synthetic and OC screenshots were
inspected. No hardware test or performance acceptance is implied.

Run `--verify-full-preview --preview-root=ABS_NEW_DIRECTORY` after `--` in the
source or exported app, with the same isolated/hidden/bounded test conventions as
S01. Optional `--preview-source=ABS_GPX` creates a disposable review and explicitly
classifies its breaks for testing; it does not edit the source or original review.
Optional `--preview-snapshot=ABS_PNG` requires rendered execution, not `--headless`.

## Explicit local repair review

The sparse preview now offers source-bound proposals for short pause connectors,
local elevation blends and stationary elevation consolidation. Select a proposal
to inspect its location, per-sample elevation changes, before/after stations and
grades, route length and retained-break count. **Accept repair**, **Reject repair**
and **Undo decision** save through the existing revision/lock/atomic replacement
path. Undo returns the proposal to an inactive state and restores the original
derived profile; snapshots remain unchanged. Other accepted repairs remain applied
in each comparison. A missing grade at a break is shown as unavailable.

The default 5 m pause cutoff is unchanged. Only an accepted source-bound connector
may extend it to 10 m; the existing height/grade checks still apply. Blends use
nearby endpoint heights and horizontal distance over at most 100 m / 32 samples,
changing each height by at most 2 m. Stationary clusters retain their first recorded
height. These are review suggestions, not verified road elevations; steep grades
remain visible. Missing travel, missing samples and unclassified source breaks
are not repaired automatically. At most 128 non-overlapping proposals are offered.

Decisions include exact proposed values in the optional `repairs` map in schema-1
reviews. Existing reviews without that map remain readable. Older clients reject
reviews containing repairs rather than silently ignoring them. Accepted repairs
block conflicting source/gap changes until undone. Inactive decisions can remain
as records after changing source choices. Preview and builder consume the same
corrected native profile. Existing packages are not modified; replan/rebuild from
the saved revision to include accepted repairs.

`content-builder --review-repairs REVIEW NEW_JSON` exports a private comparison
without changing the review. Add `--verify-route-repairs` to the existing
`--verify-full-preview` test to exercise accept/reject/undo in a new disposable
review; optional `--preview-source` still creates a copy, never editing a saved
review. The authoritative OC review now contains all four accepted proposals.
Full-corridor preparation, incremental rebuilds, mountain acceptance, scenery and
performance/endurance gates remain open; this does not close S05 or M7.

Validation on 2026-10-11: locked/offline Rust workspace tests and 73 Python tests
passed. The release Windows build/export and exported synthetic/OC repair lifecycle
checks passed (`ROUTE_REPAIRS_OK`), as did ordinary source review and sparse preview
regressions. In a disposable OC review, accepting all four proposals yielded
153.126048 km, 3,716 retained points, 13 bridged pauses and zero retained breaks;
the native builder exported that same repaired profile. Hashes confirmed the
authoritative review and its GPX snapshot were unchanged. The rendered synthetic
repair screen was inspected; controls fit the default window and the content
scrolls at smaller sizes. This verifies software behavior, not elevation acceptance
or whole-route scenery/trainer performance.

## Next

The user accepted all four OC repairs on 2026-10-11. Native reopen and profile
export verified 153.126048 km, 3,716 points, 13 bridged pauses and zero retained
breaks or profile issues in the authoritative review. Full-corridor preparation
is next; this does not establish a completed full-route scenery package.

Before repair, the saved OC review had four retained findings. Its first 8 km is
continuous and needs no steep-grade override. A real terrain/road package for that
section is now under `local/oc_to_solana/content`; see the [tiled section builder](m7-tiled-sections.md).
It uses 16 USGS source tiles, a shared 10 m terrain lattice and 85 streamed chunks.
The exported app traversed the whole section with at most nine live / three cached
chunks. A synthetic 7 km mountain package preserved ±20% grades across 18 source
tiles and passed exported traversal. Source acquisition recovered from an HTTP
502 by reusing the verified cache. Provenance corruption is rejected before viewing.

Locked/offline Rust tests, 73 Python tests, Windows export, real/synthetic streamed
traversal, library discovery, software-only power-demo and existing UX regressions
passed. Those earlier checks left the saved review unchanged; the user's subsequent
repair acceptance is recorded above. These bounded proofs do not close S03:
Death Ride mountain/user acceptance is still pending. Implement S04 full-corridor preparation
next. Imagery/distant terrain remain S06, and the 30-minute supervised ride and
measured performance acceptance remain S07. No hardware was exercised.
