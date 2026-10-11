//! Offline exports of OpenRide's acknowledged-controller journal. No hardware I/O.
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::Path,
};
mod fit;
mod xml;

pub const MAX_JOURNAL_BYTES: u64 = 64 * 1024 * 1024;
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Measurements {
    pub power_w: Option<i16>,
    pub cadence_rpm: Option<f64>,
    pub heart_rate_bpm: Option<u8>,
    pub speed_kmh: Option<f64>,
    pub distance_m: Option<u32>,
    pub elapsed_s: Option<u16>,
    pub power_received_ms: Option<u64>,
    pub cadence_received_ms: Option<u64>,
    pub hr_received_ms: Option<u64>,
    #[serde(default)]
    pub selected: bool,
}
#[derive(Debug, Clone)]
pub struct Sample {
    pub ms: u64,
    pub data: Measurements,
    pub virtual_distance_m: Option<f64>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Interval {
    pub start_ms: u64,
    pub end_ms: u64,
}
#[derive(Debug, Deserialize)]
pub struct VirtualState {
    #[serde(rename = "elapsed_ms")]
    pub ms: u64,
    pub distance_m: f64,
    pub speed_kph: f64,
    pub course_grade_percent: f64,
    pub acknowledged_grade_percent: f64,
    pub simulation_state: String,
    pub power_w: Option<f64>,
    pub route_sha256: String,
}
#[derive(Debug)]
pub struct Activity {
    pub selected_sources: Option<Value>,
    pub hr_config: Option<Value>,
    pub hr_samples: Vec<(u64, Option<u8>)>,
    pub route_config: Option<Value>,
    pub virtual_states: Vec<VirtualState>,
    pub utc_unix_ms: u64,
    pub samples: Vec<Sample>,
    pub intervals: Vec<Interval>,
    pub start_ms: u64,
    pub end_ms: u64,
    pub grades: Vec<(u64, f64)>,
    pub complete: bool,
    pub recovered_running: bool,
    pub discarded_partial_line: bool,
    pub warnings: Vec<String>,
}
#[derive(Debug, Serialize)]
pub struct ExportReport {
    pub selected_sources: Option<Value>,
    pub hr_config: Option<Value>,
    pub hr_samples: usize,
    pub schema: u32,
    pub source_sha256: String,
    pub output_directory: String,
    pub completed_session: bool,
    pub discarded_partial_line: bool,
    pub start_utc: String,
    pub end_utc: String,
    pub elapsed_ms: u64,
    pub timer_ms: u64,
    pub csv_samples: usize,
    pub virtual_samples: usize,
    pub virtual_distance_m: Option<f64>,
    pub route_config: Option<Value>,
    pub fit_records: usize,
    pub tcx_records: usize,
    pub gpx_records: usize,
    pub geographic_source: Option<String>,
    pub running_intervals: Vec<Interval>,
    pub warnings: Vec<String>,
}
fn number(row: &Value, name: &str) -> Result<u64, String> {
    row[name]
        .as_u64()
        .ok_or_else(|| format!("Missing/invalid {name}"))
}
impl Activity {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() as u64 > MAX_JOURNAL_BYTES {
            return Err("Journal exceeds 64 MiB limit".into());
        }
        let end = bytes.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
        let mut a = Self {
            selected_sources: None,
            hr_config: None,
            hr_samples: vec![],
            route_config: None,
            virtual_states: vec![],
            utc_unix_ms: 0,
            samples: vec![],
            intervals: vec![],
            start_ms: 0,
            end_ms: 0,
            grades: vec![],
            complete: false,
            recovered_running: false,
            discarded_partial_line: end < bytes.len(),
            warnings: vec![],
        };
        let mut previous = 0;
        let mut running = None;
        let mut first_start = None;
        let mut last_stop = None;
        let mut last_received = 0;
        for (count, line) in bytes[..end]
            .split(|b| *b == b'\n')
            .filter(|l| !l.is_empty())
            .enumerate()
        {
            let row: Value = serde_json::from_slice(line)
                .map_err(|e| format!("Journal row {}: {e}", count + 1))?;
            let ms = number(&row, "elapsed_ms")?;
            if ms >= u64::from(u32::MAX) {
                return Err("Journal duration exceeds export range".into());
            }
            if number(&row, "sequence")? != count as u64 || ms < previous {
                return Err("Invalid journal sequence/time order".into());
            }
            let event = row["event"].as_str().ok_or("Missing event")?;
            if count == 0 {
                if event != "session_check_created" || row["schema"] != 1 || ms != 0 {
                    return Err("Unsupported journal header/schema".into());
                }
                a.utc_unix_ms = number(&row, "utc_unix_ms")?;
            } else if event == "session_check_created" {
                return Err("Duplicate journal header".into());
            }
            match event {
                "selected_sources" => {
                    if a.selected_sources.is_some() || first_start.is_some() || row["schema"] != 1 {
                        return Err("Invalid selected source configuration".into());
                    }
                    let pairing = trainer_core::pairing::Pairing::parse(&row["roles"].to_string())?;
                    if pairing.heart_rate.is_some() && a.hr_config.is_some() {
                        return Err("Conflicting Bluetooth and ANT HR selections".into());
                    }
                    if !row["measurement_schema"].is_null() && row["measurement_schema"] != 1 {
                        return Err("Unsupported selected measurement schema".into());
                    }
                    a.selected_sources = Some(row.clone());
                }
                "ant_hr_config" => {
                    if a.selected_sources
                        .as_ref()
                        .is_some_and(|s| !s["roles"]["heart_rate"].is_null())
                    {
                        return Err("Conflicting Bluetooth and ANT HR selections".into());
                    }
                    if a.hr_config.is_some()
                        || first_start.is_some()
                        || row["schema"] != 1
                        || row["device_type"] != 120
                        || row["freshness_ms"] != 3000
                        || !row["device_number"]
                            .as_u64()
                            .is_some_and(|n| (1..=65535).contains(&n))
                        || !row["transmission_type"]
                            .as_u64()
                            .is_some_and(|n| (1..=255).contains(&n))
                    {
                        return Err("Invalid HR source configuration".into());
                    }
                    a.hr_config = Some(row.clone());
                }
                "ant_hr_data" => {
                    let config = a.hr_config.as_ref().ok_or("HR data without selection")?;
                    for field in ["device_number", "device_type", "transmission_type"] {
                        if config[field] != row[field] {
                            return Err("HR source identity mismatch".into());
                        }
                    }
                    let received = number(&row, "received_ms")?;
                    if received > ms || a.hr_samples.last().is_some_and(|(t, _)| *t > received) {
                        return Err("Invalid HR receipt time".into());
                    }
                    let bpm = if row["heart_rate_bpm"].is_null() {
                        None
                    } else {
                        Some(
                            row["heart_rate_bpm"]
                                .as_u64()
                                .filter(|n| (1..=254).contains(n))
                                .ok_or("Invalid HR value")? as u8,
                        )
                    };
                    a.hr_samples.push((received, bpm));
                }
                "ant_hr_status" if row["status"] != "receiving" => {
                    if a.hr_config.is_some() {
                        let received = number(&row, "received_ms")?;
                        if received > ms || a.hr_samples.last().is_some_and(|(t, _)| *t > received)
                        {
                            return Err("Invalid HR status time".into());
                        }
                        a.hr_samples.push((received, None));
                    }
                }
                "route_session_config" => {
                    if a.route_config.is_some() || first_start.is_some() || row["schema"] != 1 {
                        return Err("Invalid virtual route configuration".into());
                    }
                    for field in ["route_sha256", "package_manifest_sha256"] {
                        let hash = row[field]
                            .as_str()
                            .ok_or("Missing virtual route identity")?;
                        if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                            return Err("Invalid route identity".into());
                        }
                    }
                    a.route_config = Some(row.clone());
                }
                "virtual_state" => {
                    let config = a
                        .route_config
                        .as_ref()
                        .ok_or("Virtual state without route configuration")?;
                    let v: VirtualState =
                        serde_json::from_value(row.clone()).map_err(|e| e.to_string())?;
                    if ![
                        "running",
                        "paused",
                        "finished",
                        "power_missing",
                        "timing_gap",
                    ]
                    .contains(&v.simulation_state.as_str())
                        || config["route_sha256"] != v.route_sha256
                        || !v.distance_m.is_finite()
                        || !(0. ..=42949672.94).contains(&v.distance_m)
                        || !v.speed_kph.is_finite()
                        || !(0. ..=180.).contains(&v.speed_kph)
                        || !v.course_grade_percent.is_finite()
                        || !v.acknowledged_grade_percent.is_finite()
                        || v.power_w
                            .is_some_and(|p| !p.is_finite() || !(0. ..=3000.).contains(&p))
                        || a.virtual_states
                            .last()
                            .is_some_and(|p| p.distance_m > v.distance_m)
                    {
                        return Err("Invalid virtual state/identity/progress".into());
                    }
                    a.virtual_states.push(v);
                }

                "command_acknowledged" => match row["opcode"].as_u64() {
                    Some(7) if running.is_none() => {
                        a.complete = false;
                        first_start.get_or_insert(ms);
                        running = Some(ms);
                    }
                    Some(8) => {
                        if let Some(start) = running.take() {
                            a.intervals.push(Interval {
                                start_ms: start,
                                end_ms: ms,
                            });
                        }
                        last_stop = Some(ms);
                    }
                    Some(17) => {
                        let wire: Vec<u8> = serde_json::from_value(row["bytes"].clone())
                            .map_err(|_| "Invalid acknowledged simulation bytes")?;
                        if wire.len() != 7 || wire[0] != 17 {
                            return Err("Invalid acknowledged simulation bytes".into());
                        }
                        a.grades.push((
                            ms,
                            f64::from(i16::from_le_bytes([wire[3], wire[4]])) / 100.0,
                        ));
                    }
                    _ => (),
                },
                "indoor_bike_data" | "selected_measurements" => {
                    let selected_mode = a
                        .selected_sources
                        .as_ref()
                        .is_some_and(|s| s["measurement_schema"] == 1);
                    if event == "indoor_bike_data" && selected_mode {
                        previous = ms;
                        continue;
                    }
                    if event == "selected_measurements" && !selected_mode {
                        return Err("Selected measurements without source configuration".into());
                    }
                    let received = number(&row, "received_ms")?;
                    if received > ms || received < last_received {
                        return Err("Invalid telemetry receipt time".into());
                    }
                    let mut data: Measurements =
                        serde_json::from_value(row["decoded"]["Ok"].clone())
                            .map_err(|e| format!("Invalid decoded telemetry: {e}"))?;
                    data.selected = selected_mode;
                    if selected_mode {
                        for (present, timestamp) in [
                            (data.power_w.is_some(), data.power_received_ms),
                            (data.cadence_rpm.is_some(), data.cadence_received_ms),
                            (data.heart_rate_bpm.is_some(), data.hr_received_ms),
                        ] {
                            if timestamp.is_some_and(|t| t > received)
                                || (present && timestamp.is_none_or(|t| received - t >= 3000))
                            {
                                return Err("Invalid or stale selected sensor timestamp".into());
                            }
                        }
                        if data.heart_rate_bpm == Some(255) {
                            return Err("Unsupported HR value".into());
                        }
                    }
                    if data.cadence_rpm.is_some_and(|v| !v.is_finite() || v < 0.0)
                        || data.speed_kmh.is_some_and(|v| !v.is_finite() || v < 0.0)
                    {
                        return Err("Invalid cadence/speed".into());
                    }
                    a.samples.push(Sample {
                        ms: received,
                        data,
                        virtual_distance_m: None,
                    });
                    last_received = received;
                }
                "session_check_complete" => {
                    a.complete = row["pause_acknowledged"] == true
                        && running.is_none()
                        && last_stop.is_some();
                }
                "session_check_failed" => a.complete = false,
                _ => (),
            }
            previous = ms;
        }
        a.start_ms = first_start.ok_or("No acknowledged Start; not an activity")?;
        if a.discarded_partial_line {
            a.complete = false;
            a.warnings
                .push("Discarded an incomplete final journal line.".into());
        }
        if a.complete {
            a.end_ms = last_stop.ok_or("Missing acknowledged Pause")?;
        } else {
            // A recovery boundary is an export convention, not proof that hardware stopped.
            a.end_ms = if a.route_config.is_some() {
                a.virtual_states.last().map_or(previous, |v| v.ms)
            } else {
                a.samples.last().map_or(previous, |s| s.ms)
            }
            .max(last_stop.unwrap_or(0))
            .max(a.start_ms);
            if let Some(start) = running {
                a.recovered_running = true;
                a.intervals.push(Interval {
                    start_ms: start,
                    end_ms: a.end_ms.max(start),
                });
            }
            a.warnings.push("Incomplete session: export ends at the last observed sample/acknowledged stop; trainer state is not established.".into());
        }
        if a.end_ms < a.start_ms || a.intervals.is_empty() {
            return Err("No recorded running interval".into());
        }
        a.timestamp(a.end_ms)?;
        a.warnings.push("FIT records use whole-second timestamps and omit paused/pre-start/post-finish samples. CSV retains every telemetry notification and millisecond timestamps.".into());
        if a.route_config.is_some() {
            if a.virtual_states.is_empty() {
                return Err("Virtual activity has no recorded progress".into());
            }
            a.warnings.push("Virtual activity: FIT speed/distance come from recorded simulation snapshots (horizontal route station distance); trainer measurements remain in activity.csv. virtual.csv preserves every snapshot, including paused states whose stored speed is held for resume. FIT uses running snapshots only, with latest sensor readings younger than three seconds. Timer boundaries remain acknowledged Start/Pause; no geographic positions or altitude are invented.".into());
        } else {
            a.warnings.push("Trainer speed is reported speed, not virtual route speed. FIT omits distance, position, altitude, and synthetic target grade; raw trainer counters and acknowledged grade remain in CSV.".into());
        }
        Ok(a)
    }
    pub fn active(&self, ms: u64) -> bool {
        self.intervals
            .iter()
            .any(|i| ms >= i.start_ms && ms < i.end_ms)
            || (self.recovered_running && ms == self.end_ms)
    }
    pub fn timer_ms(&self) -> u64 {
        self.intervals.iter().map(|i| i.end_ms - i.start_ms).sum()
    }
    pub fn unix_ms(&self, ms: u64) -> Result<u64, String> {
        self.utc_unix_ms
            .checked_add(ms)
            .ok_or("UTC timestamp overflow".into())
    }
    pub fn timestamp(&self, ms: u64) -> Result<String, String> {
        let value = i64::try_from(self.unix_ms(ms)?).map_err(|_| "UTC timestamp out of range")?;
        DateTime::<Utc>::from_timestamp_millis(value)
            .map(|d| d.to_rfc3339_opts(SecondsFormat::Millis, true))
            .ok_or("UTC timestamp out of range".into())
    }
    pub fn csv(&self) -> Result<String, String> {
        fn cell<T: ToString>(v: Option<T>) -> String {
            v.map_or(String::new(), |v| v.to_string())
        }
        let mut out = String::from(
            "timestamp_utc,journal_elapsed_ms,activity_elapsed_ms,timer_active,power_w,cadence_rpm,heart_rate_bpm,trainer_speed_kmh,trainer_distance_m,trainer_elapsed_s,acknowledged_grade_percent\n",
        );
        for s in &self.samples {
            let d = &s.data;
            let grade = self
                .grades
                .iter()
                .rev()
                .find(|(ms, _)| *ms <= s.ms)
                .map(|(_, g)| *g);
            let relative = i128::from(s.ms) - i128::from(self.start_ms);
            out.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{},{}\n",
                self.timestamp(s.ms)?,
                s.ms,
                relative,
                self.active(s.ms),
                cell(d.power_w),
                cell(d.cadence_rpm),
                cell(d.heart_rate_bpm),
                cell(d.speed_kmh),
                cell(d.distance_m),
                cell(d.elapsed_s),
                cell(grade)
            ));
        }
        Ok(out)
    }
    pub fn virtual_distance(&self) -> Option<f64> {
        self.virtual_states
            .iter()
            .rev()
            .find(|v| v.ms <= self.end_ms)
            .map(|v| v.distance_m)
    }
    fn virtual_sample(&self, v: &VirtualState) -> Sample {
        let i = self.samples.partition_point(|s| s.ms <= v.ms);
        let mut data = if i > 0 && v.ms - self.samples[i - 1].ms < 3000 {
            self.samples[i - 1].data.clone()
        } else {
            Measurements::default()
        };
        if data.selected {
            if data
                .cadence_received_ms
                .is_none_or(|t| v.ms.saturating_sub(t) >= 3000)
            {
                data.cadence_rpm = None;
            }
            if data
                .hr_received_ms
                .is_none_or(|t| v.ms.saturating_sub(t) >= 3000)
            {
                data.heart_rate_bpm = None;
            }
        }
        if self.hr_config.is_some() {
            data.heart_rate_bpm = self.selected_hr(v.ms);
        }
        data.power_w = v.power_w.map(|p| p.round() as i16);
        data.speed_kmh = Some(v.speed_kph);
        data.distance_m = None;
        data.elapsed_s = None;
        Sample {
            ms: v.ms,
            data,
            virtual_distance_m: Some(v.distance_m),
        }
    }
    fn selected_hr(&self, ms: u64) -> Option<u8> {
        let i = self.hr_samples.partition_point(|(t, _)| *t <= ms);
        if i > 0 && ms - self.hr_samples[i - 1].0 < 3000 {
            self.hr_samples[i - 1].1
        } else {
            None
        }
    }
    pub fn hr_csv(&self) -> Result<String, String> {
        let mut out = String::from("timestamp_utc,journal_elapsed_ms,heart_rate_bpm\n");
        for (ms, bpm) in &self.hr_samples {
            out.push_str(&format!(
                "{},{},{}\n",
                self.timestamp(*ms)?,
                ms,
                bpm.map_or(String::new(), |b| b.to_string())
            ));
        }
        Ok(out)
    }
    pub fn fit_samples(&self) -> Vec<Sample> {
        if self.route_config.is_none() {
            return self
                .samples
                .iter()
                .cloned()
                .map(|mut s| {
                    if self.hr_config.is_some() {
                        s.data.heart_rate_bpm = self.selected_hr(s.ms);
                    }
                    s
                })
                .collect();
        }
        self.virtual_states
            .iter()
            .filter(|v| v.simulation_state == "running")
            .map(|v| self.virtual_sample(v))
            .collect()
    }
    pub fn virtual_csv(&self) -> Result<String, String> {
        let mut out = String::from(
            "timestamp_utc,journal_elapsed_ms,timer_active,simulation_state,virtual_distance_m,virtual_speed_kmh,course_grade_percent,acknowledged_grade_percent,power_w,cadence_rpm,heart_rate_bpm\n",
        );
        for v in &self.virtual_states {
            let s = self.virtual_sample(v);
            out.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{},{}\n",
                self.timestamp(v.ms)?,
                v.ms,
                self.active(v.ms),
                v.simulation_state,
                v.distance_m,
                v.speed_kph,
                v.course_grade_percent,
                v.acknowledged_grade_percent,
                s.data.power_w.map_or(String::new(), |p| p.to_string()),
                s.data.cadence_rpm.map_or(String::new(), |p| p.to_string()),
                s.data
                    .heart_rate_bpm
                    .map_or(String::new(), |p| p.to_string())
            ));
        }
        Ok(out)
    }
    pub fn fit(&self) -> Result<(Vec<u8>, usize, Vec<String>), String> {
        fit::encode(self)
    }
}

/// Build all outputs first, then exclusively create a new export directory.
/// report.json is written last; its presence marks a completed export bundle.
pub fn export(source: &Path, destination: &Path) -> Result<ExportReport, String> {
    export_with_manifest(source, destination, None)
}

/// An optional original package manifest supports recordings made before manifests were saved.
pub fn export_with_manifest(
    source: &Path,
    destination: &Path,
    manifest: Option<&Path>,
) -> Result<ExportReport, String> {
    let file = std::fs::File::open(source).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(MAX_JOURNAL_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let activity = Activity::parse(&bytes)?;
    let csv = activity.csv()?;
    let warnings = activity.warnings.clone();
    let mut report = ExportReport {
        selected_sources: activity.selected_sources.clone(),
        schema: 4,
        hr_config: activity.hr_config.clone(),
        hr_samples: activity.hr_samples.len(),
        tcx_records: 0,
        gpx_records: 0,
        geographic_source: None,
        source_sha256: format!("{:x}", Sha256::digest(&bytes)),
        output_directory: destination.display().to_string(),
        completed_session: activity.complete,
        discarded_partial_line: activity.discarded_partial_line,
        start_utc: activity.timestamp(activity.start_ms)?,
        end_utc: activity.timestamp(activity.end_ms)?,
        elapsed_ms: activity.end_ms - activity.start_ms,
        timer_ms: activity.timer_ms(),
        csv_samples: activity.samples.len(),
        virtual_samples: activity.virtual_states.len(),
        virtual_distance_m: activity.virtual_distance(),
        route_config: activity.route_config.clone(),
        fit_records: 0,
        running_intervals: activity.intervals.clone(),
        warnings,
    };
    let mut profile = Vec::new();
    if let Some(config) = &activity.route_config {
        std::fs::File::open(source.with_file_name("route-profile.json"))
            .map_err(|e| format!("Missing saved route profile: {e}"))?
            .take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut profile)
            .map_err(|e| e.to_string())?;
        if profile.len() > 2 * 1024 * 1024
            || config["route_sha256"] != format!("{:x}", Sha256::digest(&profile))
        {
            return Err("Saved route profile hash mismatch".into());
        }
    }
    let saved_manifest = source.with_file_name("package-manifest.json");
    let manifest_path =
        manifest.or_else(|| saved_manifest.exists().then_some(saved_manifest.as_path()));
    let mut manifest_bytes = Vec::new();
    let geography = if let (Some(config), Some(path)) = (&activity.route_config, manifest_path) {
        std::fs::File::open(path)
            .map_err(|e| e.to_string())?
            .take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut manifest_bytes)
            .map_err(|e| e.to_string())?;
        if manifest_bytes.len() > 8 * 1024 * 1024
            || config["package_manifest_sha256"] != format!("{:x}", Sha256::digest(&manifest_bytes))
        {
            return Err("Package manifest hash mismatch or size limit exceeded".into());
        }
        let g = xml::Geography::parse(&profile, &manifest_bytes)?;
        g.at(activity.virtual_distance().unwrap_or(0.))?;
        report.geographic_source =
            Some("Derived virtual positions: hash-verified saved route and package origin".into());
        Some(g)
    } else {
        None
    };
    let (fit, records, fit_warnings) = fit::encode_with_geography(&activity, geography.as_ref())?;
    report.fit_records = records;
    report.warnings.extend(fit_warnings);
    if geography.is_some() {
        report.warnings.push("FIT coordinates and altitude are derived virtual route positions from verified geography; virtual cycling classification is retained.".into());
    }
    let xml = xml::encode(&activity, geography.as_ref())?;
    report.tcx_records = xml.count;
    report.gpx_records = if xml.gpx.is_some() { xml.count } else { 0 };
    if xml.gpx.is_none() {
        report.warnings.push("GPX omitted: no verified geographic origin. TCX contains no positions. For older virtual sessions, select the original package manifest.".into());
    }
    report.warnings.push("TCX/GPX retain active samples at millisecond precision and split tracks at acknowledged pauses. Coordinates/elevation are derived virtual positions, not outdoor GPS. TCX calories are unknown (required placeholder 0); trainer-only distance is unknown (required placeholder 0). GPX custom power/speed/distance extensions may be ignored by other tools; prefer FIT or TCX for analysis.".into());
    let virtual_csv = activity.virtual_csv()?;
    let hr_csv = activity.hr_csv()?;
    let summary = serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?;
    std::fs::create_dir(destination).map_err(|e| {
        format!("Create a new export folder (existing folders are never overwritten): {e}")
    })?;
    let mut outputs = vec![
        ("activity.fit", fit.as_slice()),
        ("activity.csv", csv.as_bytes()),
        ("activity.tcx", xml.tcx.as_bytes()),
    ];
    if activity.hr_config.is_some() {
        outputs.push(("heart-rate.csv", hr_csv.as_bytes()));
    }
    if activity.route_config.is_some() {
        outputs.push(("virtual.csv", virtual_csv.as_bytes()));
        outputs.push(("route-profile.json", profile.as_slice()));
    }
    if let Some(gpx) = &xml.gpx {
        outputs.push(("activity.gpx", gpx.as_bytes()));
    }
    if !manifest_bytes.is_empty() {
        outputs.push(("package-manifest.json", manifest_bytes.as_slice()));
    }
    outputs.push(("report.json", summary.as_slice()));
    for (name, data) in outputs {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination.join(name))
            .map_err(|e| format!("Partial export in {}: {e}", destination.display()))?;
        file.write_all(data)
            .and_then(|_| file.sync_all())
            .map_err(|e| format!("Partial export in {}: {e}", destination.display()))?;
    }
    Ok(report)
}
