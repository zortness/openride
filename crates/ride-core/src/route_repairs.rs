//! Explicit source-bound local proposals. Pending/rejected repairs never alter a profile.
use crate::{full_route::Profile, source_review::Review, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub id: String,
    pub source_sha256: String,
    pub sample: usize,
    pub kind: String,
    pub elevations: BTreeMap<usize, f64>,
    pub connector: Option<usize>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Repair {
    pub proposal: Proposal,
    pub status: String,
}
#[derive(Serialize)]
pub struct Comparison {
    pub proposal: Proposal,
    pub status: String,
    pub before: Vec<Measurement>,
    pub after: Vec<Measurement>,
    pub before_length_m: f64,
    pub after_length_m: f64,
    pub before_breaks: usize,
    pub after_breaks: usize,
    pub elevation_changes: Vec<ElevationChange>,
}
#[derive(Serialize)]
pub struct ElevationChange {
    pub sample: usize,
    pub before_m: f64,
    pub after_m: f64,
}
#[derive(Serialize)]
pub struct Measurement {
    pub sample: usize,
    pub elevation_m: f64,
    pub distance_m: f64,
    pub grade_percent: Option<f64>,
    pub break_before: bool,
}

fn baseline(review: &Review) -> Review {
    let mut document = review.document.clone();
    document.repairs.clear();
    Review {
        document,
        revision: review.revision.clone(),
        activities: review.activities.clone(),
        gaps: vec![],
    }
}

/// Conservative candidates only: no missing travel, broad smoothing or global cutoff change.
pub fn proposals(review: &Review) -> Result<Vec<Proposal>> {
    let raw = baseline(review);
    let profile = Profile::from_review(&raw)?;
    let hash = raw.document.geometry_source.as_ref().unwrap();
    let source = raw
        .activities
        .iter()
        .find(|s| &s.source_sha256 == hash)
        .unwrap();
    let mut proposals = vec![];
    let mut edited_samples = BTreeSet::new();
    for (index, pair) in profile.points.windows(2).enumerate() {
        let a = &pair[0];
        let b = &pair[1];
        if !b.break_before {
            continue;
        }
        let step = crate::haversine(a.lat, a.lon, b.lat, b.lon);
        let pause = raw
            .document
            .decisions
            .get(&format!("{hash}:{}", b.sample))
            .is_some_and(|d| d.classification == "stationary_pause");
        // A derived stationary finding need not be a source gap. Missing samples cannot be bridged.
        if b.sample != a.sample + 1 || (source.samples[b.sample].break_before && !pause) {
            continue;
        }
        let mut p = Proposal {
            id: format!("{hash}:{}:local-v1", b.sample),
            source_sha256: hash.clone(),
            sample: b.sample,
            kind: String::new(),
            elevations: BTreeMap::new(),
            connector: None,
        };
        if step >= 0.01
            && step <= 10.
            && (b.height - a.height).abs() <= 1.
            && (b.height - a.height).abs() <= step
            && pause
        {
            p.kind = "short_connector".into();
            p.connector = Some(b.sample);
        } else if step < 0.01 && (b.height - a.height).abs() <= 2. {
            p.kind = "stationary_elevation".into();
            // Keep the first recorded height throughout this stationary cluster.
            for (i, s) in source.samples.iter().enumerate().skip(a.sample).take(32) {
                let (Some(lat), Some(lon), Some(h)) = (s.latitude, s.longitude, s.elevation_m)
                else {
                    break;
                };
                if crate::haversine(a.lat, a.lon, lat, lon) >= 0.01 || (h - a.height).abs() > 2. {
                    break;
                }
                if i > a.sample
                    && s.break_before
                    && !raw
                        .document
                        .decisions
                        .get(&format!("{hash}:{i}"))
                        .is_some_and(|d| d.classification == "stationary_pause")
                {
                    break;
                }
                p.elevations.insert(i, a.height);
            }
            if !p.elevations.contains_key(&b.sample) {
                continue;
            }
        } else if step <= 5. && pause && index > 0 && index + 2 < profile.points.len() {
            let left = &profile.points[index - 1];
            let right = &profile.points[index + 2];
            if a.break_before || right.break_before || right.sample - left.sample > 32 {
                continue;
            }
            let samples = &source.samples[left.sample..=right.sample];
            let mut stations = vec![0.];
            let mut valid = true;
            for s in samples.windows(2) {
                match (
                    s[0].latitude,
                    s[0].longitude,
                    s[1].latitude,
                    s[1].longitude,
                    s[1].elevation_m,
                ) {
                    (Some(x), Some(y), Some(z), Some(w), Some(_)) => {
                        stations.push(stations.last().unwrap() + crate::haversine(x, y, z, w))
                    }
                    _ => {
                        valid = false;
                        break;
                    }
                }
            }
            let length = *stations.last().unwrap();
            if !valid || length < 0.01 || length > 100. {
                continue;
            }
            for (offset, station) in stations.iter().enumerate().skip(1).take(samples.len() - 2) {
                let height = left.height + (right.height - left.height) * station / length;
                if (height - samples[offset].elevation_m.unwrap()).abs() > 2. {
                    valid = false;
                    break;
                }
                p.elevations.insert(left.sample + offset, height);
            }
            if !valid {
                continue;
            }
            p.kind = "local_elevation_blend".into();
        } else {
            continue;
        }
        if p.elevations.keys().any(|i| edited_samples.contains(i)) {
            continue;
        }
        edited_samples.extend(p.elevations.keys().copied());
        proposals.push(p);
        if proposals.len() == 128 {
            break;
        }
    }
    Ok(proposals)
}

pub fn validate(review: &Review) -> Result<()> {
    if review.document.repairs.is_empty() {
        return Ok(());
    }
    let candidates = if review
        .document
        .repairs
        .values()
        .any(|r| r.status == "accepted")
    {
        proposals(review)?
    } else {
        vec![]
    };
    let mut samples = BTreeSet::new();
    for (id, repair) in &review.document.repairs {
        if id != &repair.proposal.id
            || *id
                != format!(
                    "{}:{}:local-v1",
                    repair.proposal.source_sha256, repair.proposal.sample
                )
            || !matches!(repair.status.as_str(), "proposed" | "accepted" | "rejected")
        {
            return Err("Invalid repair identity or status".into());
        }
        let source = review
            .activities
            .iter()
            .find(|a| a.source_sha256 == repair.proposal.source_sha256)
            .ok_or("Repair source missing")?;
        if repair.proposal.sample >= source.samples.len()
            || repair
                .proposal
                .elevations
                .iter()
                .any(|(i, h)| *i >= source.samples.len() || !h.is_finite())
        {
            return Err("Invalid repair samples or heights".into());
        }
        if repair.status == "accepted" {
            if !candidates.contains(&repair.proposal) {
                return Err("Accepted repair conflicts with source selection or gap decisions; undo it before changing those decisions".into());
            }
            for sample in repair.proposal.elevations.keys() {
                if !samples.insert(*sample) {
                    return Err(
                        "Accepted elevation repairs overlap; undo one before accepting another"
                            .into(),
                    );
                }
            }
        }
    }
    Ok(())
}

pub fn comparisons(review: &Review) -> Result<Vec<Comparison>> {
    let mut result = vec![];
    for proposal in proposals(review)? {
        let source = review
            .activities
            .iter()
            .find(|a| a.source_sha256 == proposal.source_sha256)
            .unwrap();
        let elevation_changes = proposal
            .elevations
            .iter()
            .map(|(i, h)| ElevationChange {
                sample: *i,
                before_m: source.samples[*i].elevation_m.unwrap(),
                after_m: *h,
            })
            .collect();
        let mut before = baseline(review);
        before.document.repairs = review.document.repairs.clone();
        before.document.repairs.remove(&proposal.id);
        let a = Profile::from_review(&before)?;
        let mut after = before;
        after.document.repairs.insert(
            proposal.id.clone(),
            Repair {
                proposal: proposal.clone(),
                status: "accepted".into(),
            },
        );
        let b = Profile::from_review(&after)?;
        let start = proposal
            .elevations
            .keys()
            .next()
            .copied()
            .unwrap_or(proposal.sample)
            .saturating_sub(2);
        let end = proposal
            .elevations
            .keys()
            .next_back()
            .copied()
            .unwrap_or(proposal.sample)
            + 2;
        let measurements = |profile: &Profile| -> Vec<Measurement> {
            profile
                .points
                .iter()
                .enumerate()
                .filter(|(_, p)| p.sample >= start && p.sample <= end)
                .map(|(i, p)| {
                    let previous = i.checked_sub(1).map(|j| &profile.points[j]);
                    Measurement {
                        sample: p.sample,
                        elevation_m: p.height,
                        distance_m: p.distance,
                        break_before: p.break_before,
                        grade_percent: previous
                            .filter(|a| !p.break_before && p.distance > a.distance)
                            .map(|a| 100. * (p.height - a.height) / (p.distance - a.distance)),
                    }
                })
                .collect()
        };
        result.push(Comparison {
            status: review
                .document
                .repairs
                .get(&proposal.id)
                .map(|r| r.status.clone())
                .unwrap_or("proposed".into()),
            before: measurements(&a),
            after: measurements(&b),
            before_length_m: a.length_m(),
            after_length_m: b.length_m(),
            before_breaks: a.unresolved_breaks,
            after_breaks: b.unresolved_breaks,
            elevation_changes,
            proposal,
        });
    }
    Ok(result)
}

pub fn decide(
    directory: &std::path::Path,
    revision: &str,
    id: &str,
    status: &str,
) -> Result<Review> {
    let review = crate::source_review::open(directory)?;
    if review.revision != revision {
        return Err("Review changed; reopen before deciding a repair".into());
    }
    let proposal = proposals(&review)?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or("Repair proposal no longer available")?;
    let mut document = review.document.clone();
    document.repairs.insert(
        id.into(),
        Repair {
            proposal,
            status: status.into(),
        },
    );
    crate::source_review::save(directory, revision, document)
}
