//! TCX activities and GPX tracks. Coordinates describe simulated route progress.
use crate::{Activity, Sample};
use serde::Deserialize;

#[derive(Deserialize)]
struct Point {
    position: [f64; 3],
    elevation_m: f64,
    distance_m: f64,
    #[serde(default)]
    break_before: bool,
}
pub(crate) struct Geography {
    origin: [f64; 3],
    points: Vec<Point>,
}
impl Geography {
    pub fn parse(profile: &[u8], manifest: &[u8]) -> Result<Self, String> {
        #[derive(Deserialize)]
        struct Profile {
            points: Vec<Point>,
        }
        #[derive(Deserialize)]
        struct Manifest {
            origin_lat_lon_elevation: [f64; 3],
        }
        let p: Profile = serde_json::from_slice(profile).map_err(|e| e.to_string())?;
        let m: Manifest = serde_json::from_slice(manifest).map_err(|e| e.to_string())?;
        let origin = m.origin_lat_lon_elevation;
        if !origin.iter().all(|v| v.is_finite())
            || origin[0].abs() >= 80.
            || origin[1].abs() > 180.
            || p.points.len() < 2
            || p.points[0].distance_m != 0.
            || p.points.iter().any(|p| {
                !p.position.iter().all(|v| v.is_finite())
                    || !p.elevation_m.is_finite()
                    || !p.distance_m.is_finite()
            })
            || p.points
                .windows(2)
                .any(|p| p[1].distance_m <= p[0].distance_m || p[1].break_before)
        {
            return Err("Invalid geographic route/origin or disconnected route".into());
        }
        Ok(Self {
            origin,
            points: p.points,
        })
    }
    pub fn at(&self, distance: f64) -> Result<[f64; 3], String> {
        if !distance.is_finite()
            || distance < 0.
            || distance > self.points.last().unwrap().distance_m
        {
            return Err("Virtual progress outside saved route".into());
        }
        let i = self
            .points
            .partition_point(|p| p.distance_m < distance)
            .clamp(1, self.points.len() - 1);
        let (a, b) = (&self.points[i - 1], &self.points[i]);
        let t = (distance - a.distance_m) / (b.distance_m - a.distance_m);
        let x = a.position[0] + t * (b.position[0] - a.position[0]);
        let z = a.position[2] + t * (b.position[2] - a.position[2]);
        let lat = self.origin[0] - (z / 6_371_000.).to_degrees();
        let lon =
            self.origin[1] + (x / (6_371_000. * self.origin[0].to_radians().cos())).to_degrees();
        if lat.abs() > 90. || lon.abs() > 180. {
            return Err("Derived position outside geographic bounds".into());
        }
        Ok([
            lat,
            lon,
            a.elevation_m + t * (b.elevation_m - a.elevation_m),
        ])
    }
}
const NOTES: &str = "OpenRide indoor activity. Positions and elevation, when present, are derived virtual positions along the saved route, not recorded outdoor GPS. Calories are unknown (required TCX placeholder 0). Distance is unknown for trainer-only sessions (TCX placeholder 0). Tracks split at acknowledged pauses; samples retain millisecond timestamps.";
pub(crate) struct Outputs {
    pub tcx: String,
    pub gpx: Option<String>,
    pub count: usize,
}
pub(crate) fn encode(a: &Activity, geo: Option<&Geography>) -> Result<Outputs, String> {
    let start = a.timestamp(a.start_ms)?;
    let mut tcx=format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><TrainingCenterDatabase xmlns=\"http://www.garmin.com/xmlschemas/TrainingCenterDatabase/v2\" xmlns:ae=\"http://www.garmin.com/xmlschemas/ActivityExtension/v2\"><Activities><Activity Sport=\"Biking\"><Id>{start}</Id><Lap StartTime=\"{start}\"><TotalTimeSeconds>{:.3}</TotalTimeSeconds><DistanceMeters>{:.6}</DistanceMeters><Calories>0</Calories><Intensity>Active</Intensity><TriggerMethod>Manual</TriggerMethod>",a.timer_ms() as f64/1000.,a.virtual_distance().unwrap_or(0.));
    let mut gpx=geo.map(|_|format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><gpx version=\"1.1\" creator=\"OpenRide\" xmlns=\"http://www.topografix.com/GPX/1/1\" xmlns:gpxtpx=\"http://www.garmin.com/xmlschemas/TrackPointExtension/v1\" xmlns:or=\"https://openride.local/xmlschemas/activity/v1\"><metadata><name>OpenRide virtual ride</name><desc>{NOTES}</desc><time>{start}</time></metadata><trk><name>OpenRide virtual ride</name>"));
    let samples = a.fit_samples();
    let mut count = 0;
    for (idx, interval) in a.intervals.iter().enumerate() {
        let segment: Vec<&Sample> = samples
            .iter()
            .filter(|s| {
                s.ms >= interval.start_ms
                    && (s.ms < interval.end_ms
                        || (a.recovered_running
                            && idx + 1 == a.intervals.len()
                            && s.ms == interval.end_ms))
            })
            .collect();
        if segment.is_empty() {
            continue;
        }
        tcx.push_str("<Track>");
        if let Some(g) = &mut gpx {
            g.push_str("<trkseg>");
        }
        for s in segment {
            count += 1;
            let time = a.timestamp(s.ms)?;
            let position = match (geo, s.virtual_distance_m) {
                (Some(g), Some(d)) => Some(g.at(d)?),
                _ => None,
            };
            tcx.push_str(&format!("<Trackpoint><Time>{time}</Time>"));
            if let Some([lat, lon, ele]) = position {
                tcx.push_str(&format!("<Position><LatitudeDegrees>{lat:.9}</LatitudeDegrees><LongitudeDegrees>{lon:.9}</LongitudeDegrees></Position><AltitudeMeters>{ele:.6}</AltitudeMeters>"));
            }
            if let Some(d) = s.virtual_distance_m {
                tcx.push_str(&format!("<DistanceMeters>{d:.6}</DistanceMeters>"));
            }
            if let Some(hr) = s.data.heart_rate_bpm.filter(|v| *v > 0) {
                tcx.push_str(&format!("<HeartRateBpm><Value>{hr}</Value></HeartRateBpm>"));
            }
            let cadence = s
                .data
                .cadence_rpm
                .filter(|v| (0. ..=254.).contains(v))
                .map(|v| v.round() as u8);
            if let Some(c) = cadence {
                tcx.push_str(&format!("<Cadence>{c}</Cadence>"));
            }
            // GoldenCheetah 3.8 user-verified form: local default namespace,
            // unprefixed extension names. Preserve this spelling for interoperability.
            tcx.push_str(
                "<Extensions><TPX xmlns=\"http://www.garmin.com/xmlschemas/ActivityExtension/v2\">",
            );
            if let Some(v) = s.data.speed_kmh {
                tcx.push_str(&format!("<Speed>{:.6}</Speed>", v / 3.6));
            }
            if let Some(w) = s.data.power_w.filter(|v| *v >= 0) {
                tcx.push_str(&format!("<Watts>{w}</Watts>"));
            }
            tcx.push_str("</TPX></Extensions></Trackpoint>");
            if let (Some(g), Some([lat, lon, ele])) = (&mut gpx, position) {
                g.push_str(&format!("<trkpt lat=\"{lat:.9}\" lon=\"{lon:.9}\"><ele>{ele:.6}</ele><time>{time}</time><extensions><gpxtpx:TrackPointExtension>"));
                if let Some(hr) = s.data.heart_rate_bpm.filter(|v| *v > 0) {
                    g.push_str(&format!("<gpxtpx:hr>{hr}</gpxtpx:hr>"));
                }
                if let Some(c) = cadence {
                    g.push_str(&format!("<gpxtpx:cad>{c}</gpxtpx:cad>"));
                }
                g.push_str("</gpxtpx:TrackPointExtension><or:sample>");
                if let Some(w) = s.data.power_w {
                    g.push_str(&format!("<or:power_w>{w}</or:power_w>"));
                }
                if let Some(v) = s.data.speed_kmh {
                    g.push_str(&format!("<or:speed_mps>{:.6}</or:speed_mps>", v / 3.6));
                }
                if let Some(d) = s.virtual_distance_m {
                    g.push_str(&format!("<or:distance_m>{d:.6}</or:distance_m>"));
                }
                g.push_str("</or:sample></extensions></trkpt>");
            }
        }
        tcx.push_str("</Track>");
        if let Some(g) = &mut gpx {
            g.push_str("</trkseg>");
        }
    }
    tcx.push_str(&format!(
        "</Lap><Notes>{NOTES}</Notes></Activity></Activities></TrainingCenterDatabase>"
    ));
    if let Some(g) = &mut gpx {
        g.push_str("</trk></gpx>");
    }
    Ok(Outputs { tcx, gpx, count })
}
