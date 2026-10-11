# Activity exports

Use **Activity exports** in the Windows app. Choose a recorded `session.jsonl`, choose an output parent folder, and export. Processing runs on a Rust worker. Each export creates a new folder containing:

- `activity.fit`: indoor or virtual cycling activity, timer events, one lap, one session, and sensor records.
- `activity.tcx`: activity timing, virtual distance, power/speed/cadence/heart rate where available, and derived positions when a verified geographic origin is available.
- `activity.gpx`: virtual track, when a verified origin is available.
- `activity.csv`: all decoded telemetry notifications, including pre-start, paused, and post-finish observations.
- `report.json`: source SHA-256, completion/recovery status, exact durations, sample counts, running intervals, and conversion notes. Written last as the export completion marker.

Exports never overwrite a source or an existing output directory. A write failure can leave a partial new directory; the error identifies it. No export uploads data or connects to equipment. Private exports belong in the ignored `export/` directory or a private folder outside the project.

## Virtual route rides

Virtual route journals are now supported. FIT records use recorded virtual distance and speed, with cycling/virtual-activity classification. Raw trainer speed and counters remain in `activity.csv`. Extra files are:

- `virtual.csv`: every virtual snapshot, including course/acknowledged trainer grade and simulation state, with sensor values joined by timestamp.
- `route-profile.json`: the exact saved profile, checked against the recorded route SHA-256 before export.
- `report.json` schema 4 also includes final virtual distance, virtual snapshot count and the original route/package identities and simulation settings.

FIT chooses the last **running simulation snapshot** per UTC second within acknowledged Start/Pause intervals. Virtual power comes from the snapshot; cadence/heart rate use the latest recorded notification no more than three seconds old (strictly less than three). Values are not interpolated. Paused snapshots retain the simulation's stored speed for resumption; their `simulation_state` identifies the hold and they are excluded from FIT records. FIT lap/session total distance uses the last recorded snapshot at or before activity end, so it can exceed the last whole-second record slightly. Recovery uses the last virtual snapshot or acknowledged stop without extrapolating progress. Timer duration continues to mean acknowledged controller running time, which can differ slightly from the simulation's moving time.

Distance is horizontal route station distance; speed is the simulation's road speed. FIT, TCX and GPX now include derived geographic positions and elevation when verified geographic context is available. FIT uses signed semicircle latitude/longitude and standard/enhanced altitude (0.2 m resolution). The virtual classification and distance fields follow Garmin's [published FIT profile](https://github.com/garmin/fit-python-sdk/blob/main/garmin_fit_sdk/profile.py); no SDK implementation is bundled.

The successful live session `route-1791669428-795-28244936` was exported through the Windows app to `export/rpv-live-ride`: 45 FIT records, 64 raw CSV notifications, 296 virtual snapshots, 44.601 seconds timer/elapsed and 119.426 m virtual distance (119.43 m in FIT). Structural decoding and CRC checks passed. On 2026-10-10, the user confirmed successful GoldenCheetah import of this virtual FIT, with visible power and speed graphs. This confirms import and graph display; distance totals, timer interpretation and pause handling remain unconfirmed.

## TCX and GPX

The export window writes TCX alongside FIT/CSV. GPX is added when the original geographic origin can be verified. Future live rides save `package-manifest.json` with the session. Older rides can use **Older ride: choose original manifest (optional)** and select the original `.orpack/manifest.json`. The CLI accepts that path as a third argument; the UI verification accepts `--manifest=ABSOLUTE_PATH`.

The manifest must match the package SHA-256 recorded in the journal, and the profile must match its recorded route SHA-256. Both are copied into geographic export bundles. Missing geographic context produces positionless TCX and no GPX, with an explanation in the window/report. A supplied or saved manifest with the wrong hash fails before creating an output folder. Originals are never modified.

Coordinates are **derived virtual positions**, not outdoor GPS measurements. The exporter interpolates saved local route geometry at recorded virtual station distance and reverses the content builder's local projection using the saved origin. Elevation comes from the accepted route profile, including its corrections. Out-of-range progress, invalid origins, unordered stations and disconnected profiles are rejected rather than extrapolated or bridged.

TCX/GPX use all running simulation snapshots within acknowledged running intervals (raw notifications for trainer-only TCX), retaining millisecond timestamps. Each interval becomes a separate track/segment. They do not fabricate start/end samples; consequently point timestamps or GPX geometry may cover slightly less than the acknowledged duration/final distance. TCX lap totals preserve acknowledged timer time and final virtual distance. GPX has no standard activity timer/distance summary; consumers may calculate their own totals, including pauses differently.

TCX uses Training Center Database v2 and ActivityExtension v2 for speed (m/s) and watts. Its required calories field is 0 as an explicitly documented **unknown placeholder**, not an energy estimate. Trainer-only TCX also uses required lap distance 0 as unknown, with no per-point distance. Cadence is rounded to whole rpm; values outside 0-254 rpm and negative watts are omitted from TCX, while CSV retains them. Missing/zero heart rate is omitted. FIT remains preferable when fractional cadence matters.

GPX 1.1 uses Garmin TrackPointExtension v1 for heart rate/cadence. The OpenRide namespace `https://openride.local/xmlschemas/activity/v1` is an identifier, not a service: `sample` contains optional `power_w`, `speed_mps`, and `distance_m`. Other applications may ignore these extensions; use FIT/TCX for training analysis. No upload is performed.

Validated sample: `export/rpv-live-xml`, from the same 44.601 s / 119.426 m RPV ride, contains 208 TCX points and 208 GPX points. The exported Windows app produced the bundle; Python's independent XML parser checked namespaces, counts, timestamps, sensor values, coordinates and summaries against CSV and saved provenance. Run:

```powershell
& "C:/Users/zortn/AppData/Local/Python/bin/python.exe" tools/verify-activity-xml.py export/rpv-live-xml
```

The automated check is structural/semantic validation, not XSD validation. Direct schema download was blocked in the agent environment. The user subsequently confirmed both GPX and TCX files verified in GoldenCheetah on 2026-10-10, establishing external application acceptance of the short-ride exports alongside the previously accepted FIT. This sample does not exercise mid-ride pause/resume or full-route completion; those remain integrated ride checks.

References: [TCX v2 schema](https://www8.garmin.com/xmlschemas/TrainingCenterDatabasev2.xsd), [ActivityExtension v2 schema](https://www8.garmin.com/xmlschemas/ActivityExtensionv2.xsd), [GPX 1.1 documentation](https://www.topografix.com/gpx/1/1/).

## Time and measurements

UTC comes from the journal's UTC creation timestamp plus monotonic elapsed time. Telemetry uses callback receipt time rather than later log-write time. Running intervals begin at acknowledged Start and end at acknowledged Pause. Elapsed duration includes pauses but excludes the initial connection setup and final five-second observation; timer duration excludes pauses. Device-reported elapsed/distance counters are preserved as separate CSV columns, not used as activity duration or virtual distance.

CSV columns are `timestamp_utc`, `journal_elapsed_ms`, `activity_elapsed_ms`, `timer_active`, `power_w`, `cadence_rpm`, `heart_rate_bpm`, `trainer_speed_kmh`, `trainer_distance_m`, `trainer_elapsed_s`, and `acknowledged_grade_percent`. Empty cells mean absent measurements; zero remains zero. Relative activity time is negative before Start. Cadence fractions, signed power, millisecond timestamps, and raw trainer counters are retained. Grade is the last acknowledged synthetic target at the measurement receipt time, not a measured road slope.

FIT contains only samples from running intervals. Where multiple notifications occupy the same UTC second, the last one is selected without interpolation or averaging. Missing fields use FIT invalid values. Negative power and out-of-range values that FIT cannot represent are omitted with a report warning; CSV retains them. Cadence includes the standard fractional field, and speed is converted from km/h to m/s. FIT timer events have whole-second timestamps; summary durations retain millisecond precision. Tools that reconstruct durations solely from event timestamps can differ by rounding.

FIT has no invented outdoor GPS measurements, local timezone, energy, or summary power statistics. Its geographic fields, when available, describe derived virtual route positions and elevation, with virtual cycling classification retained. For M2 journals without virtual state, FIT uses trainer speed and omits distance. For virtual journals, the rules above apply. Synthetic target grade remains in CSV. The file identifies its producer with the development manufacturer ID; no Garmin/Elite manufacturer identity is claimed for OpenRide's file.

## Recovery

Only an incomplete final JSONL line may be discarded. Corrupt complete records, reversed receipt/log times, bad sequence numbers, unsupported headers, excessive duration, and input larger than 64 MiB are rejected. A recording without an acknowledged Start is not an activity.

For an incomplete activity, the recovery boundary is the last observed sample or acknowledged stop. An open running interval receives an export-only ending there, including its last sample. The report marks the activity incomplete. This is not evidence that the trainer physically stopped. A live journal exported while recording is a snapshot and will likewise be marked incomplete.

## CLI and tests

```powershell
cargo run -p activity-core --locked --offline -- "PATH_TO_SESSION_JSONL" "NEW_EXPORT_DIRECTORY"
cargo test -p activity-core --locked --offline
cargo clippy -p activity-core -p openride-godot --all-targets --locked --offline -- -D warnings
```

The native export test runs against an external journal and a new output folder:

```powershell
./dist/OpenRide.exe --headless -- --verify-activity-export --journal=ABSOLUTE_SESSION_JSONL --output=ABSOLUTE_NEW_DIRECTORY
```

Quote any argument containing spaces. For automation, use `Start-Process -Wait -PassThru` and inspect its exit code. Success prints `ACTIVITY_EXPORT_OK`. `--export-snapshot=ABSOLUTE_PNG` also renders the export result when run without `--headless`.

Ten Rust tests cover CRC check vectors/full-file residue, schema structure and sensor units, zero versus missing values, fractional cadence, pause duration, complete versus recovered intervals, malformed/torn input, second-resolution conversion, and exclusive output creation. The native window was rendered and inspected using a real session export. These checks are not a substitute for external application interoperability.

### Independent decoder and GoldenCheetah

The independent `fitdecode` package is a development-only validation tool, not an application dependency. Once available in a normal network-enabled shell:

```powershell
$env:UV_CACHE_DIR = Join-Path (Get-Location) 'local/uv-cache'
uv pip install --python "C:/Users/zortn/AppData/Local/Python/bin/python.exe" --target local/fit-validation/vendor fitdecode
& "C:/Users/zortn/AppData/Local/Python/bin/python.exe" tools/verify-activity-fit.py export/activities/session-1791600730
```

Substitute another installed Python if needed. The validator uses strict CRC/parse checks, compares decoded summaries and sensor values against CSV/report data, and prints `FITDECODE_OK` only after success. It neither downloads nor uploads anything itself.

On 2026-10-09, the user confirmed successful import into their analysis application (called "opencheetah" in the report, following our GoldenCheetah discussion), with a visible power graph over approximately 42 seconds. This establishes user-observed import and power display. The first and last exported running samples occupy UTC seconds 02:52:25 and 02:53:07, a 42-second span consistent with that graph. The acknowledged session boundaries still give 43.410 s elapsed and 37.206 s timer time; application summary and pause handling have not yet been checked.

Independent decoder execution remains pending: installation failed because the agent's network access is restricted and the package was not cached. The earlier M2 export is `export/activities/session-1791600730/activity.fit`, with 37 FIT records and 50 CSV notifications. The newer virtual ride export is `export/rpv-live-ride/activity.fit`, as described above.

## Protocol references

The encoder is original MIT code with no bundled Garmin SDK implementation or generated profile. Its narrow field set follows Garmin's [activity-file requirements](https://developer.garmin.com/fit/articles/file-types/activity.html), [FIT protocol](https://developer.garmin.com/fit/protocol/), and published [message fields](https://github.com/garmin/fit-cpp-sdk/blob/main/src/fit_record_mesg.hpp). It emits protocol 2.0 headers and profile 21.218 field definitions. Independent validation uses the MIT [fitdecode reader](https://github.com/polyvertex/fitdecode).

## Full-route export follow-up (2026-10-10)

The user completed session `route-1791677726-822-147140954`, but reported needing both TCX and FIT imports for all data and no map placement in GoldenCheetah. Inspection confirmed valid coordinates in the original TCX/GPX, while FIT omitted coordinates and elevation. FIT now includes those fields from the same hash-verified geography; missing geographic context remains missing, never zero coordinates. Tests cover negative longitude, latitude, altitude encoding, sensor preservation and absent-geography sentinels.

The rebuilt app exported `export/rpv_session_export/with-fit-geography/activity.fit`: 338 records. A separate structural decoder checked CRC, signed coordinates, altitude, power and speed against TCX. XML semantic validation passed for 1,613 points; original journal hash was unchanged. GoldenCheetah acceptance of this single corrected FIT remains pending. Which additional fields required the two-format import has been requested from the user; no specific TCX importer defect has been established.

User clarification: TCX/GPX imported but did not display power; only FIT showed it. TCX contains the expected Watts values, so full TCX sensor interoperability remains open. A namespace-equivalent diagnostic TCX is available in `export/rpv_session_export/tcx-compatibility-check`; no production encoding change has been made based on this unconfirmed hypothesis.

## Accepted single-file formats (2026-10-10)

The user confirmed the compatible TCX shows power and map data in GoldenCheetah 3.8, and the corrected FIT works with all data. The production exporter now emits the accepted TCX form: `<TPX xmlns="http://www.garmin.com/xmlschemas/ActivityExtension/v2">` with unprefixed `<Speed>` and `<Watts>` children. This supersedes the unresolved-import notes above. The precise importer failure mechanism was not established.

The rebuilt application exported `export/rpv_session_export/accepted-formats`. Its FIT and TCX are byte-identical to the user-accepted files; ten Rust export tests and real-session XML checks passed. Either FIT or TCX can now serve as the single-file analysis export for this tested setup. FIT preserves whole-second records and fractional cadence; TCX retains active snapshot timestamps. GPX remains suitable for track interchange; its custom power extension is not confirmed compatible with GoldenCheetah.

## Selected ANT heart rate (M5)

Optional ANT HR is recorded in the same controller journal on its monotonic clock. `ant_hr_config` identifies the selected device number, profile (120), transmission type and 3000 ms freshness limit. `ant_hr_data` retains measurement receipt time, decoded BPM and received payload; `ant_hr_status` records search/availability separately from trainer state. The network key and its path are not included in the journal configuration.

Exports validate the selected identity and receipt ordering. With ANT HR selected, FIT/TCX/GPX and `virtual.csv` use only that source: latest measurement at or before the exported sample, strictly younger than three seconds. Invalid/missing values and unavailable/searching status clear HR; there is no fallback to a different ANT source or Bluetooth HR. Existing sessions without an ANT selection retain their previous behavior.

`activity.csv` remains the unmodified Bluetooth measurement stream. An additional `heart-rate.csv` preserves ANT HR measurements and explicit missing/status boundaries, including observations outside active timer intervals. Its empty BPM cells mean missing, not zero. Report schema 4 adds `hr_config` and `hr_samples` (CSV rows, including missing/status boundaries); source provenance is in `hr_config`. FIT still selects the last whole sample per UTC second, so some subsecond HR transitions appear only in TCX/GPX/CSV.
