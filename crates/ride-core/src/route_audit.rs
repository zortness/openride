//! Read-only software rehearsal using the same fixed-tick physics as riding.
use crate::{full_route::Profile, Result};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Rehearsal {
    pub power_w: f64,
    pub status: &'static str,
    pub simulated_seconds: f64,
    pub distance_m: f64,
    pub route_length_m: f64,
    pub source_sample: usize,
    pub grade_percent_at_stop: f64,
    pub peak_speed_mps: f64,
}

/// A bounded, constant-power rehearsal; neither scenery nor device I/O is involved.
/// Less than one metre of progress over a minute is reported as a stall, not success.
pub fn rehearse(profile: &Profile, power_w: f64, max_seconds: u64) -> Result<Rehearsal> {
    if !power_w.is_finite()
        || !(1. ..=3000.).contains(&power_w)
        || !(60..=86400).contains(&max_seconds)
    {
        return Err("Rehearsal requires 1–3,000 W and 60–86,400 seconds".into());
    }
    let mut simulation = profile.simulation()?;
    simulation.resume();
    let mut peak_speed: f64 = 0.;
    let mut checkpoint = 0.;
    let mut status = "time_limit";
    let mut ticks = 0;
    for tick in 1..=max_seconds * 10 {
        simulation.advance(100, Some(power_w))?;
        ticks = tick;
        peak_speed = peak_speed.max(simulation.speed_mps());
        if simulation.status() == "finished" {
            status = "finished";
            break;
        }
        if tick % 600 == 0 {
            if simulation.distance_m() - checkpoint < 1. {
                status = "stalled_at_test_power";
                break;
            }
            checkpoint = simulation.distance_m();
        }
    }
    let i = profile
        .points
        .partition_point(|p| p.distance < simulation.distance_m())
        .clamp(1, profile.points.len() - 1);
    Ok(Rehearsal {
        power_w,
        status,
        simulated_seconds: ticks as f64 / 10.,
        distance_m: simulation.distance_m(),
        route_length_m: profile.length_m(),
        source_sample: profile.points[i].sample,
        grade_percent_at_stop: simulation.grade_percent(),
        peak_speed_mps: peak_speed,
    })
}

#[derive(Serialize)]
pub struct GradeFinding {
    pub source_sample: usize,
    pub station_m: f64,
    pub segment_m: f64,
    pub rise_m: f64,
    pub raw_grade_percent: f64,
    pub context_20m_grade_percent: Option<f64>,
}

pub fn steep_findings(profile: &Profile) -> Vec<GradeFinding> {
    let height_at = |d: f64| -> Option<f64> {
        let i = profile
            .points
            .partition_point(|p| p.distance < d)
            .clamp(1, profile.points.len() - 1);
        let (a, b) = (&profile.points[i - 1], &profile.points[i]);
        if b.break_before || b.distance <= a.distance {
            return None;
        }
        Some(a.height + (b.height - a.height) * (d - a.distance) / (b.distance - a.distance))
    };
    profile
        .points
        .windows(2)
        .filter_map(|p| {
            let step = p[1].distance - p[0].distance;
            let rise = p[1].height - p[0].height;
            if p[1].break_before || step <= 0. || (100. * rise / step).abs() <= 15. {
                return None;
            }
            let middle = (p[0].distance + p[1].distance) / 2.;
            let left = (middle - 10.).max(0.);
            let right = (middle + 10.).min(profile.length_m());
            let context = if profile
                .points
                .iter()
                .any(|p| p.break_before && p.distance >= left && p.distance <= right)
            {
                None
            } else {
                height_at(left)
                    .zip(height_at(right))
                    .map(|(a, b)| 100. * (b - a) / (right - left))
            };
            Some(GradeFinding {
                source_sample: p[1].sample,
                station_m: p[1].distance,
                segment_m: step,
                rise_m: rise,
                raw_grade_percent: 100. * rise / step,
                context_20m_grade_percent: context,
            })
        })
        .collect()
}
