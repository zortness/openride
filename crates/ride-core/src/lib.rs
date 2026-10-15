//! Engine-independent activity import and route inspection. No engine or network access.
pub mod full_route;
pub mod route_audit;
pub mod route_repairs;
pub mod source_review;
use chrono::DateTime;
use quick_xml::{events::Event, Reader};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};

pub type Result<T> = std::result::Result<T, String>;
/// Canonical runtime route JSON only; scenery payloads retain their own budgets.
pub const MAX_ROUTE_PROFILE_BYTES: usize = 8 * 1024 * 1024;
const MAX_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Sample {
    pub time_ms: Option<i64>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub elevation_m: Option<f64>,
    pub heart_rate_bpm: Option<f64>,
    pub cadence_rpm: Option<f64>,
    pub power_w: Option<f64>,
    pub speed_mps: Option<f64>,
    pub temperature_c: Option<f64>,
    pub recorded_distance_m: Option<f64>,
    pub lap: usize,
    pub segment: usize,
    pub route_distance_m: f64,
    pub break_before: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Lap {
    pub start_time: Option<String>,
    pub distance_m: Option<f64>,
    pub timer_s: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: String,
    pub sample: Option<usize>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Activity {
    pub name: String,
    pub source_name: String,
    pub source_sha256: String,
    pub format: String,
    pub samples: Vec<Sample>,
    pub laps: Vec<Lap>,
    pub recorded_distance_m: Option<f64>,
    pub timer_s: Option<f64>,
    pub elapsed_s: Option<f64>,
    pub route_distance_m: f64,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Default)]
struct Element {
    name: String,
    text: String,
    attributes: BTreeMap<String, String>,
    children: Vec<Element>,
}
impl Element {
    fn child(&self, name: &str) -> Option<&Self> {
        self.children.iter().find(|n| n.name == name)
    }
    fn text(&self, name: &str) -> Option<&str> {
        self.child(name).map(|n| n.text.trim())
    }
    fn descendants<'a>(&'a self, name: &str, out: &mut Vec<&'a Self>) {
        for n in &self.children {
            if n.name == name {
                out.push(n);
            }
            n.descendants(name, out);
        }
    }
    fn all(&self, name: &str) -> Vec<&Self> {
        let mut out = vec![];
        self.descendants(name, &mut out);
        out
    }
}

fn parse_xml(xml: &str) -> Result<Element> {
    if xml.len() as u64 > MAX_BYTES {
        return Err("File exceeds the 64 MiB import limit".into());
    }
    let mut reader = Reader::from_str(xml);
    reader.config_mut().expand_empty_elements = true;
    let mut stack = vec![Element::default()];
    let mut count = 0;
    loop {
        match reader
            .read_event()
            .map_err(|e| format!("XML at byte {}: {e}", reader.error_position()))?
        {
            Event::Start(e) => {
                count += 1;
                if count > 1_000_000 || stack.len() > 64 {
                    return Err("XML complexity limit exceeded".into());
                }
                let mut n = Element {
                    name: String::from_utf8_lossy(e.local_name().as_ref()).into(),
                    ..Default::default()
                };
                for attr in e.attributes() {
                    let a = attr.map_err(|e| e.to_string())?;
                    n.attributes.insert(
                        String::from_utf8_lossy(a.key.local_name().as_ref()).into(),
                        a.unescape_value().map_err(|e| e.to_string())?.into_owned(),
                    );
                }
                stack.push(n);
            }
            Event::End(_) => {
                if stack.len() < 2 {
                    return Err("Unexpected XML closing element".into());
                }
                let n = stack.pop().unwrap();
                stack.last_mut().unwrap().children.push(n);
            }
            Event::Text(e) => stack
                .last_mut()
                .unwrap()
                .text
                .push_str(&e.decode().map_err(|e| e.to_string())?),
            Event::CData(e) => stack
                .last_mut()
                .unwrap()
                .text
                .push_str(&e.decode().map_err(|e| e.to_string())?),
            Event::GeneralRef(e) => {
                let reference = e.decode().map_err(|e| e.to_string())?;
                let escaped = format!("&{reference};");
                stack
                    .last_mut()
                    .unwrap()
                    .text
                    .push_str(&quick_xml::escape::unescape(&escaped).map_err(|e| e.to_string())?);
            }
            Event::DocType(_) => return Err("DTD declarations are not supported".into()),
            Event::Eof => break,
            _ => {}
        }
    }
    if stack.len() != 1 {
        return Err("Incomplete XML document".into());
    }
    let mut root = stack.pop().unwrap();
    if root.children.len() != 1 {
        return Err("Expected one XML root element".into());
    }
    Ok(root.children.remove(0))
}

fn number(
    value: Option<&str>,
    field: &str,
    index: Option<usize>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<f64> {
    let value = value?;
    match value.parse::<f64>() {
        Ok(x) if x.is_finite() => Some(x),
        _ => {
            diagnostics.push(Diagnostic {
                code: "invalid_number".into(),
                sample: index,
                message: format!("Invalid {field}: {value}"),
            });
            None
        }
    }
}

fn sample(
    node: &Element,
    format: &str,
    lap: usize,
    segment: usize,
    index: usize,
    diagnostics: &mut Vec<Diagnostic>,
) -> Sample {
    let gpx = format == "GPX";
    let mut s = Sample {
        lap,
        segment,
        ..Default::default()
    };
    let timestamp = node.text(if gpx { "time" } else { "Time" });
    s.time_ms = timestamp.and_then(|t| match DateTime::parse_from_rfc3339(t) {
        Ok(t) => Some(t.timestamp_millis()),
        Err(_) => {
            diagnostics.push(Diagnostic {
                code: "invalid_time".into(),
                sample: Some(index),
                message: format!("Invalid timestamp: {t}"),
            });
            None
        }
    });
    let position = node.child("Position");
    let lat = if gpx {
        node.attributes.get("lat").map(String::as_str)
    } else {
        position.and_then(|n| n.text("LatitudeDegrees"))
    };
    let lon = if gpx {
        node.attributes.get("lon").map(String::as_str)
    } else {
        position.and_then(|n| n.text("LongitudeDegrees"))
    };
    s.latitude = number(lat, "latitude", Some(index), diagnostics);
    s.longitude = number(lon, "longitude", Some(index), diagnostics);
    if !matches!((s.latitude,s.longitude),(Some(a),Some(b)) if (-90.0..=90.0).contains(&a) && (-180.0..=180.0).contains(&b))
    {
        s.latitude = None;
        s.longitude = None;
    }
    s.elevation_m = number(
        node.text(if gpx { "ele" } else { "AltitudeMeters" }),
        "elevation",
        Some(index),
        diagnostics,
    );
    s.recorded_distance_m = number(
        node.text("DistanceMeters"),
        "distance",
        Some(index),
        diagnostics,
    );
    s.heart_rate_bpm = number(
        node.child("HeartRateBpm").and_then(|n| n.text("Value")),
        "heart rate",
        Some(index),
        diagnostics,
    );
    s.cadence_rpm = number(node.text("Cadence"), "cadence", Some(index), diagnostics);
    if let Some(ext) = node.child(if gpx { "extensions" } else { "Extensions" }) {
        for (tag, target) in [
            ("hr", &mut s.heart_rate_bpm),
            ("cad", &mut s.cadence_rpm),
            ("Watts", &mut s.power_w),
        ] {
            if let Some(n) = ext.all(tag).first() {
                *target = number(Some(n.text.trim()), tag, Some(index), diagnostics);
            }
        }
        for (tag, target) in [("Speed", &mut s.speed_mps), ("atemp", &mut s.temperature_c)] {
            if let Some(n) = ext.all(tag).first() {
                *target = number(Some(n.text.trim()), tag, Some(index), diagnostics);
            }
        }
        if let Some(n) = ext.all("power").first() {
            s.power_w = number(Some(n.text.trim()), "power", Some(index), diagnostics);
        }
    }
    s
}

pub fn import_xml(xml: &str, source_name: &str) -> Result<Activity> {
    let root = parse_xml(xml)?;
    let mut a = Activity {
        name: source_name.into(),
        source_name: source_name.into(),
        source_sha256: format!("{:x}", Sha256::digest(xml.as_bytes())),
        format: String::new(),
        samples: vec![],
        laps: vec![],
        recorded_distance_m: None,
        timer_s: None,
        elapsed_s: None,
        route_distance_m: 0.0,
        diagnostics: vec![],
    };
    match root.name.as_str() {
        "gpx" => {
            a.format = "GPX".into();
            let tracks = root.all("trk");
            if let Some(name) = tracks.first().and_then(|n| n.text("name")) {
                a.name = name.into();
            }
            for (segment, seg) in root.all("trkseg").iter().enumerate() {
                for p in seg.children.iter().filter(|n| n.name == "trkpt") {
                    a.samples.push(sample(
                        p,
                        "GPX",
                        0,
                        segment,
                        a.samples.len(),
                        &mut a.diagnostics,
                    ));
                }
            }
        }
        "kml" => {
            a.format = "KML".into();
            if let Some(name) = root.all("Placemark").first().and_then(|n| n.text("name")) {
                a.name = name.into();
            }
            for (segment, line) in root.all("LineString").iter().enumerate() {
                let coordinates = line
                    .text("coordinates")
                    .ok_or("KML LineString has no coordinates")?;
                for coordinate in coordinates.split_whitespace() {
                    let parts: Vec<_> = coordinate.split(',').collect();
                    if !(2..=3).contains(&parts.len()) {
                        return Err("Invalid KML coordinate tuple".into());
                    }
                    let values: Vec<f64> = parts
                        .iter()
                        .map(|v| {
                            v.parse::<f64>()
                                .map_err(|_| "Invalid KML number".to_string())
                        })
                        .collect::<Result<_>>()?;
                    if values.iter().any(|v| !v.is_finite())
                        || values[0].abs() > 180.
                        || values[1].abs() > 90.
                    {
                        return Err("Invalid KML geographic coordinates".into());
                    }
                    a.samples.push(Sample {
                        longitude: Some(values[0]),
                        latitude: Some(values[1]),
                        elevation_m: if line.text("altitudeMode") == Some("absolute") {
                            values.get(2).copied()
                        } else {
                            None
                        },
                        segment,
                        ..Default::default()
                    });
                }
            }
            if a.samples.is_empty() {
                return Err("KML requires LineString geometry; points, polygons, NetworkLinks and gx:Track are not supported".into());
            }
        }
        "TrainingCenterDatabase" => {
            a.format = "TCX".into();
            let activities = root.all("Activity");
            if activities.len() != 1 {
                return Err(format!("Expected one TCX Activity, found {}. Courses/multiple activities are not supported in M1.",activities.len()));
            }
            let mut segment = 0;
            for (lap, node) in activities[0].all("Lap").iter().enumerate() {
                a.laps.push(Lap {
                    start_time: node.attributes.get("StartTime").cloned(),
                    distance_m: number(
                        node.text("DistanceMeters"),
                        "lap distance",
                        None,
                        &mut a.diagnostics,
                    ),
                    timer_s: number(
                        node.text("TotalTimeSeconds"),
                        "lap timer",
                        None,
                        &mut a.diagnostics,
                    ),
                });
                for track in node.all("Track") {
                    for p in track.children.iter().filter(|n| n.name == "Trackpoint") {
                        a.samples.push(sample(
                            p,
                            "TCX",
                            lap,
                            segment,
                            a.samples.len(),
                            &mut a.diagnostics,
                        ));
                    }
                    segment += 1;
                }
            }
            if !a.laps.is_empty() {
                a.recorded_distance_m = a.laps.iter().map(|l| l.distance_m).sum();
                a.timer_s = a.laps.iter().map(|l| l.timer_s).sum();
            }
        }
        _ => {
            return Err(format!(
                "Unsupported root element: {}. Expected GPX track, TCX activity or KML LineString.",
                root.name
            ))
        }
    }
    if a.samples.is_empty() {
        return Err("No activity track samples found".into());
    }
    analyze(&mut a);
    Ok(a)
}

fn analyze(a: &mut Activity) {
    let mut distance = 0.0;
    for i in 0..a.samples.len() {
        let s = &a.samples[i];
        let mut broken = i == 0;
        for (missing, code, message) in [
            (
                s.latitude.is_none(),
                "missing_position",
                "Sample has no valid GPS position",
            ),
            (
                s.time_ms.is_none(),
                "missing_time",
                "Sample has no valid timestamp",
            ),
            (
                s.elevation_m.is_none(),
                "missing_elevation",
                "Sample has no elevation",
            ),
        ] {
            if missing {
                a.diagnostics.push(Diagnostic {
                    code: code.into(),
                    sample: Some(i),
                    message: message.into(),
                });
            }
        }
        if i > 0 {
            let p = &a.samples[i - 1];
            broken = s.segment != p.segment || s.latitude.is_none() || p.latitude.is_none();
            if s.segment != p.segment {
                a.diagnostics.push(Diagnostic {
                    code: "segment_boundary".into(),
                    sample: Some(i),
                    message: "Source track/segment boundary; retained in the route".into(),
                });
            }
            if let (Some(t), Some(prev)) = (s.time_ms, p.time_ms) {
                if t <= prev {
                    a.diagnostics.push(Diagnostic {
                        code: "nonincreasing_time".into(),
                        sample: Some(i),
                        message: "Repeated or decreasing timestamp; sample retained".into(),
                    });
                    broken = true;
                }
                if t - prev > 60_000 {
                    a.diagnostics.push(Diagnostic {
                        code: "time_gap".into(),
                        sample: Some(i),
                        message: format!(
                            "{:.1} second gap; no route distance bridged",
                            (t - prev) as f64 / 1000.0
                        ),
                    });
                    broken = true;
                }
            } else if s.time_ms.is_some() != p.time_ms.is_some() {
                broken = true;
            }
            if !broken {
                distance += haversine(
                    p.latitude.unwrap(),
                    p.longitude.unwrap(),
                    s.latitude.unwrap(),
                    s.longitude.unwrap(),
                );
            }
        }
        a.samples[i].break_before = broken;
        a.samples[i].route_distance_m = distance;
    }
    a.route_distance_m = distance;
    a.elapsed_s = a
        .samples
        .first()
        .and_then(|p| p.time_ms)
        .zip(a.samples.last().and_then(|p| p.time_ms))
        .filter(|(s, e)| e >= s)
        .map(|(s, e)| (e - s) as f64 / 1000.0);
    for (count, name) in [
        (
            a.samples
                .iter()
                .filter(|s| s.heart_rate_bpm.is_none())
                .count(),
            "heart rate",
        ),
        (
            a.samples.iter().filter(|s| s.cadence_rpm.is_none()).count(),
            "cadence",
        ),
        (
            a.samples.iter().filter(|s| s.power_w.is_none()).count(),
            "power",
        ),
    ] {
        if count > 0 {
            a.diagnostics.push(Diagnostic {
                code: "sensor_coverage".into(),
                sample: None,
                message: format!(
                    "{name}: {count}/{} samples absent (not zero)",
                    a.samples.len()
                ),
            });
        }
    }
}

pub fn haversine(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let p = lat1.to_radians();
    let q = lat2.to_radians();
    let h = ((q - p) / 2.0).sin().powi(2)
        + p.cos() * q.cos() * ((lon2 - lon1).to_radians() / 2.0).sin().powi(2);
    12_742_000.0 * h.clamp(0.0, 1.0).sqrt().asin()
}

pub fn import_file(path: impl AsRef<Path>) -> Result<Activity> {
    let path = path.as_ref();
    if std::fs::metadata(path).map_err(|e| e.to_string())?.len() > MAX_BYTES {
        return Err("File exceeds the 64 MiB import limit".into());
    }
    let xml = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    import_xml(
        &xml,
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .as_ref(),
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityGroup {
    pub sources: Vec<Activity>,
    pub preferred_source: usize,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Library {
    pub groups: Vec<ActivityGroup>,
    pub errors: Vec<String>,
}

fn track_signature(a: &Activity) -> BTreeMap<i64, (f64, f64)> {
    a.samples
        .iter()
        .filter_map(|s| Some((s.time_ms?, (s.latitude?, s.longitude?))))
        .collect()
}

fn same_ride(a: &Activity, b: &Activity) -> bool {
    let x = track_signature(a);
    let y = track_signature(b);
    if x.len() < 3 || y.len() < 3 {
        return false;
    }
    let (Some((xs, _)), Some((xe, _)), Some((ys, _)), Some((ye, _))) = (
        x.first_key_value(),
        x.last_key_value(),
        y.first_key_value(),
        y.last_key_value(),
    ) else {
        return false;
    };
    if (xs - ys).abs() > 10_000 || (xe - ye).abs() > 10_000 {
        return false;
    }
    let matched = x
        .iter()
        .filter(|(t, (lat, lon))| {
            y.get(t).is_some_and(|(other_lat, other_lon)| {
                haversine(*lat, *lon, *other_lat, *other_lon) < 3.0
            })
        })
        .count();
    matched >= 3 && matched as f64 / x.len().max(y.len()) as f64 >= 0.90
}

impl Library {
    pub fn insert(&mut self, activity: Activity) -> usize {
        for (i, group) in self.groups.iter_mut().enumerate() {
            if group
                .sources
                .iter()
                .any(|a| a.source_sha256 == activity.source_sha256)
            {
                return i;
            }
            if group.sources.iter().any(|a| same_ride(a, &activity)) {
                // Preserve both exports and their differing values rather than inventing a merged sample.
                if activity.format == "TCX" && group.sources[group.preferred_source].format != "TCX"
                {
                    group.preferred_source = group.sources.len();
                }
                group.sources.push(activity);
                return i;
            }
        }
        self.groups.push(ActivityGroup {
            sources: vec![activity],
            preferred_source: 0,
        });
        self.groups.len() - 1
    }

    pub fn import_paths(paths: &[String]) -> Self {
        let mut library = Self::default();
        for path in paths {
            match import_file(path) {
                Ok(a) => {
                    library.insert(a);
                }
                Err(e) => library.errors.push(format!(
                    "{}: {e}",
                    Path::new(path)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                )),
            }
        }
        library
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutePoint {
    pub latitude: f64,
    pub longitude: f64,
    pub elevation_m: Option<f64>,
    pub distance_m: f64,
    pub source_sample: usize,
    pub break_before: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteDocument {
    pub schema_version: u32,
    pub kind: String,
    pub name: String,
    pub source_sha256: String,
    pub source_format: String,
    pub source_range: [usize; 2],
    pub elevation_provenance: String,
    pub points: Vec<RoutePoint>,
}

pub fn extract_route(a: &Activity, start: usize, end: usize) -> Result<RouteDocument> {
    if start >= end || end >= a.samples.len() {
        return Err("Choose an inclusive sample range with start < end within the activity".into());
    }
    let mut points: Vec<RoutePoint> = vec![];
    let mut broken = true;
    let mut base = 0.0;
    for i in start..=end {
        let s = &a.samples[i];
        broken |= s.break_before;
        if let (Some(latitude), Some(longitude)) = (s.latitude, s.longitude) {
            if points.is_empty() {
                base = s.route_distance_m;
            }
            points.push(RoutePoint {
                latitude,
                longitude,
                elevation_m: s.elevation_m,
                distance_m: s.route_distance_m - base,
                source_sample: i,
                break_before: broken,
            });
            broken = false;
        } else {
            broken = true;
        }
    }
    if points.len() < 2 {
        return Err("Selected range needs at least two valid positions".into());
    }
    Ok(RouteDocument {
        schema_version: 1,
        kind: "openride.route".into(),
        name: a.name.clone(),
        source_sha256: a.source_sha256.clone(),
        source_format: a.format.clone(),
        source_range: [start, end],
        elevation_provenance:
            "Uncorrected source activity elevation; not validated for trainer control".into(),
        points,
    })
}

pub fn read_route(json: &str) -> Result<RouteDocument> {
    if json.len() as u64 > MAX_BYTES {
        return Err("Route exceeds the 64 MiB limit".into());
    }
    let route: RouteDocument =
        serde_json::from_str(json).map_err(|e| format!("Invalid route manifest: {e}"))?;
    if route.schema_version != 1 || route.kind != "openride.route" {
        return Err("Unsupported route kind or schema version".into());
    }
    if route.points.len() < 2 || route.source_range[0] >= route.source_range[1] {
        return Err("Invalid route range or point count".into());
    }
    let mut previous_distance = 0.0;
    let mut previous_index = None;
    for (i, p) in route.points.iter().enumerate() {
        if !p.latitude.is_finite()
            || !p.longitude.is_finite()
            || !(-90.0..=90.0).contains(&p.latitude)
            || !(-180.0..=180.0).contains(&p.longitude)
            || !p.distance_m.is_finite()
            || p.distance_m < previous_distance
            || p.elevation_m.is_some_and(|e| !e.is_finite())
            || p.source_sample < route.source_range[0]
            || p.source_sample > route.source_range[1]
            || previous_index.is_some_and(|prev| p.source_sample <= prev)
        {
            return Err(format!("Invalid route point {i}"));
        }
        if i == 0 && (p.distance_m != 0.0 || !p.break_before) {
            return Err("First point must start a segment at zero distance".into());
        }
        previous_distance = p.distance_m;
        previous_index = Some(p.source_sample);
    }
    Ok(route)
}

pub fn route_gpx(route: &RouteDocument) -> String {
    let mut out=format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<gpx version=\"1.1\" creator=\"OpenRide\" xmlns=\"http://www.topografix.com/GPX/1/1\"><trk><name>{}</name>",quick_xml::escape::escape(&route.name));
    for (i, p) in route.points.iter().enumerate() {
        if i == 0 || p.break_before {
            if i > 0 {
                out.push_str("</trkseg>");
            }
            out.push_str("<trkseg>");
        }
        out.push_str(&format!(
            "<trkpt lat=\"{}\" lon=\"{}\">",
            p.latitude, p.longitude
        ));
        if let Some(e) = p.elevation_m {
            out.push_str(&format!("<ele>{e}</ele>"));
        }
        out.push_str("</trkpt>");
    }
    out.push_str("</trkseg></trk></gpx>\n");
    out
}

/// Writes a new file only, so an export cannot overwrite a source activity.
pub fn save_new(path: impl AsRef<Path>, contents: &str) -> Result<()> {
    use std::io::Write;
    let path = path.as_ref();
    let name = path
        .file_name()
        .ok_or("Missing output filename")?
        .to_string_lossy();
    let temp = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|e| e.to_string())?;
    let result = (|| -> Result<()> {
        f.write_all(contents.as_bytes())
            .map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
        // Hard link is atomic and fails if destination exists, including on Unix.
        std::fs::hard_link(&temp, path)
            .map_err(|e| format!("Cannot create export (choose a new filename): {e}"))?;
        Ok(())
    })();
    drop(f);
    let _ = std::fs::remove_file(temp);
    result
}

pub mod replay;
pub mod simulation;

pub mod live;
