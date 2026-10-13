//! Reviewed, geographically stationed profile. Native f64 index, bounded local views.
use crate::{
    simulation::{Config, Simulation},
    source_review::Review,
    Result,
};
use serde::Serialize;

#[derive(Clone, Debug)]
pub struct Point {
    pub sample: usize,
    pub distance: f64,
    pub lat: f64,
    pub lon: f64,
    pub height: f64,
    pub break_before: bool,
}
pub struct Profile {
    pub points: Vec<Point>,
    pub bridged_pauses: usize,
    pub unresolved_breaks: usize,
    pub issues: Vec<String>,
    pub revision: String,
}
#[derive(Serialize)]
pub struct LocalPoint {
    pub position: [f64; 3],
    pub break_before: bool,
    pub sample: usize,
}
#[derive(Serialize)]
pub struct Window {
    pub distance_m: f64,
    pub latitude: f64,
    pub longitude: f64,
    pub elevation_m: f64,
    pub grade_percent: Option<f64>,
    pub at_break: bool,
    pub points: Vec<LocalPoint>,
}
impl Profile {
    pub fn from_review(review: &Review) -> Result<Self> {
        crate::route_repairs::validate(review)?;
        let selected = review
            .document
            .geometry_source
            .as_ref()
            .ok_or("Select and save a geometry source first")?;
        let elevation = review
            .document
            .elevation_source
            .as_ref()
            .ok_or("Select and save an elevation source first")?;
        let mut source = review
            .activities
            .iter()
            .find(|a| &a.source_sha256 == selected)
            .ok_or("Geometry source missing")?
            .clone();
        if selected != elevation {
            return Err("This preview currently needs the same geometry and elevation source. Cross-source alignment requires a separate review.".into());
        }
        let mut connectors = std::collections::BTreeSet::new();
        for repair in review
            .document
            .repairs
            .values()
            .filter(|r| r.status == "accepted")
        {
            for (sample, height) in &repair.proposal.elevations {
                source.samples[*sample].elevation_m = Some(*height);
            }
            if let Some(sample) = repair.proposal.connector {
                connectors.insert(sample);
            }
        }
        let mut points: Vec<Point> = vec![];
        let mut bridged_pauses = 0;
        let mut unresolved_breaks = 0;
        let mut issues = vec![];
        let mut pending_break = false;
        for (i, s) in source.samples.iter().enumerate() {
            let (Some(lat), Some(lon), Some(height)) = (s.latitude, s.longitude, s.elevation_m)
            else {
                pending_break = true;
                issues.push(format!(
                    "Sample {i}: missing position or elevation; continuity remains unresolved"
                ));
                continue;
            };
            let mut p = Point {
                sample: i,
                distance: 0.,
                lat,
                lon,
                height,
                break_before: false,
            };
            if let Some(previous) = points.last() {
                let step = crate::haversine(previous.lat, previous.lon, lat, lon);
                p.break_before = s.break_before || pending_break;
                let pause = review
                    .document
                    .decisions
                    .get(&format!("{selected}:{i}"))
                    .is_some_and(|d| d.classification == "stationary_pause");
                if p.break_before
                    && pause
                    && !pending_break
                    && (step <= 5. || connectors.contains(&i) && step <= 10.)
                    && (height - previous.height).abs() <= 1.
                    && (step >= 0.01 && (height - previous.height).abs() <= step
                        || step < 0.01 && (height - previous.height).abs() < 0.01)
                {
                    p.break_before = false;
                    bridged_pauses += 1;
                } else if p.break_before {
                    issues.push(format!(
                        "Sample {i}: break retained ({step:.2} m separation, {:.2} m height change){}",
                        height-previous.height,
                        if pause {
                            " (pause exceeds 5 m or has uncertain elevation/position)"
                        } else {
                            " (not classified as a recording pause)"
                        }
                    ));
                }
                if !p.break_before && step < 0.01 {
                    // Co-located samples add no horizontal route distance. A vertical jump is a break.
                    if (height - previous.height).abs() < 0.01 {
                        pending_break = false;
                        continue;
                    }
                    p.break_before = true;
                    issues.push(format!(
                        "Sample {i}: elevation changes without horizontal travel"
                    ));
                }
                if p.break_before {
                    unresolved_breaks += 1;
                }
                p.distance = previous.distance + if p.break_before { 0. } else { step };
            }
            pending_break = false;
            points.push(p);
        }
        if points.len() < 2 || points.last().unwrap().distance <= 0. {
            return Err("Not enough located, elevated travel for a profile".into());
        }
        Ok(Self {
            points,
            bridged_pauses,
            unresolved_breaks,
            issues,
            revision: review.revision.clone(),
        })
    }
    pub fn length_m(&self) -> f64 {
        self.points.last().unwrap().distance
    }
    fn index(&self, d: f64) -> usize {
        self.points
            .partition_point(|p| p.distance <= d)
            .saturating_sub(1)
    }
    fn position(&self, d: f64) -> Point {
        let i = self.index(d);
        let a = &self.points[i];
        let Some(b) = self.points.get(i + 1) else {
            return a.clone();
        };
        if b.break_before || b.distance <= a.distance {
            return a.clone();
        }
        let t = ((d - a.distance) / (b.distance - a.distance)).clamp(0., 1.);
        Point {
            sample: a.sample,
            distance: d,
            lat: a.lat + (b.lat - a.lat) * t,
            lon: a.lon + (b.lon - a.lon) * t,
            height: a.height + (b.height - a.height) * t,
            break_before: a.break_before && t == 0.,
        }
    }
    pub fn window(&self, d: f64) -> Result<Window> {
        if !d.is_finite() {
            return Err("Distance must be finite".into());
        }
        let d = d.clamp(0., self.length_m());
        let center = self.position(d);
        let low = (d - 500.).max(0.);
        let high = (d + 500.).min(self.length_m());
        let start = self.index(low);
        let end = (self.index(high) + 2).min(self.points.len());
        // Bound transfer/render work even in very dense recordings. Never skip a break.
        let start = start.max(self.index(d).saturating_sub(511));
        let end = end.min(start + 1024);
        let mut points = vec![];
        let mut disconnected = false;
        for p in &self.points[start..end] {
            let mut p = p.clone();
            if p.distance < low {
                p = self.position(low);
            }
            if p.distance > high {
                p = self.position(high);
            }
            let x = (p.lon - center.lon).to_radians() * center.lat.to_radians().cos() * 6_371_000.;
            let z = -(p.lat - center.lat).to_radians() * 6_371_000.;
            if x.abs() >= 2000. || z.abs() >= 2000. || (p.height - center.height).abs() >= 2000. {
                disconnected = true;
                continue;
            }
            points.push(LocalPoint {
                position: [x, p.height - center.height, z],
                break_before: disconnected || p.break_before,
                sample: p.sample,
            });
            disconnected = false;
        }
        // Match Simulation's incoming-segment convention at exact sample stations.
        let i = self
            .points
            .partition_point(|p| p.distance < d)
            .clamp(1, self.points.len() - 1)
            - 1;
        let a = &self.points[i];
        let b = &self.points[i + 1];
        let grade = (!center.break_before && !b.break_before && b.distance > a.distance)
            .then(|| 100. * (b.height - a.height) / (b.distance - a.distance));
        Ok(Window {
            distance_m: d,
            latitude: center.lat,
            longitude: center.lon,
            elevation_m: center.height,
            grade_percent: grade,
            at_break: center.break_before,
            points,
        })
    }
    /// Preview traversal stops before unresolved discontinuities. Explicit seeks may inspect either side.
    pub fn advance(&self, from: f64, travel: f64) -> Result<(f64, bool)> {
        if !from.is_finite() || !travel.is_finite() || travel < 0. {
            return Err("Invalid traversal distance".into());
        }
        let target = (from + travel).min(self.length_m());
        if let Some(p) = self
            .points
            .iter()
            .skip(self.index(from) + 1)
            .take_while(|p| p.distance <= target)
            .find(|p| p.break_before)
        {
            return Ok(((p.distance - 0.001).max(from), true));
        }
        Ok((target, target >= self.length_m()))
    }
    pub fn simulation(&self) -> Result<Simulation> {
        if self.unresolved_breaks > 0 || !self.issues.is_empty() {
            return Err("Profile has unresolved breaks or missing source data".into());
        }
        Simulation::new(
            self.points.iter().map(|p| (p.distance, p.height)).collect(),
            Config::default(),
        )
    }
}
