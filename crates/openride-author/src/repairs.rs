//! Reviewable geometry-only repairs. Original samples and elevations are immutable.
use crate::{atomic_file, cancelled, checked, error, lock, open, Result, Snapshot};
use ride_core::{haversine, source_review::Review, Activity, Sample};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::Path,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    pub source: String,
    pub first: usize,
    pub last: usize,
    pub kind: String,
    pub status: String,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Book {
    pub schema_version: u32,
    pub decisions: BTreeMap<String, Decision>,
    pub history: Vec<Decision>,
}
#[derive(Debug, Serialize)]
pub struct Candidate {
    pub id: String,
    pub decision: Decision,
    pub allowed: bool,
    pub reason: String,
    pub before: Vec<Sample>,
    pub after: Vec<Sample>,
    pub distance_change_m: f64,
}
#[derive(Debug, Serialize)]
pub struct Preview {
    pub candidates: Vec<Candidate>,
    pub samples: Vec<Sample>,
    pub source_samples: Vec<usize>,
    pub before_length_m: f64,
    pub length_m: f64,
    pub unresolved_breaks: usize,
}
fn distance(a: &Sample, b: &Sample) -> Option<f64> {
    Some(haversine(
        a.latitude?,
        a.longitude?,
        b.latitude?,
        b.longitude?,
    ))
}
fn candidate(
    a: &Activity,
    first: usize,
    last: usize,
    kind: &str,
    allowed: bool,
    reason: &str,
) -> Candidate {
    let before = a.samples[first..=last].to_vec();
    let mut after = vec![a.samples[first].clone(), a.samples[last].clone()];
    if allowed {
        after[1].break_before = false;
    }
    let old: f64 = before
        .windows(2)
        .filter(|w| !w[1].break_before)
        .filter_map(|w| distance(&w[0], &w[1]))
        .sum();
    let change = distance(&after[0], &after[1]).unwrap_or(0.) - old;
    Candidate {
        id: format!("{}:{kind}:{first}:{last}:v1", a.source_sha256),
        decision: Decision {
            source: a.source_sha256.clone(),
            first,
            last,
            kind: kind.into(),
            status: "pending".into(),
        },
        allowed,
        reason: reason.into(),
        before,
        after,
        distance_change_m: change,
    }
}
pub fn candidates(a: &Activity) -> Vec<Candidate> {
    let mut result = Vec::new();
    for (i, s) in a.samples.iter().enumerate().skip(1) {
        if !s.break_before {
            continue;
        }
        let p = &a.samples[i - 1];
        let d = distance(p, s);
        let h = p.elevation_m.zip(s.elevation_m).map(|(x, y)| (y - x).abs());
        let allowed = d
            .zip(h)
            .is_some_and(|(d, h)| d <= 10. && h <= 1. && h <= d.max(0.04) * 0.25);
        result.push(candidate(a,i-1,i,"gap",allowed,if allowed {"Short recording-pause candidate. Confirm that no travel is missing; heights stay unchanged."} else {"Missing travel or uncertain position/elevation. Keep unresolved for course editing; straight gap closure is not safe within the 10 m / 1 m height limits."}));
    }
    // Fixed lookahead bounds work; no nearest-point deletion across later road visits.
    let mut i = 0;
    while i + 3 < a.samples.len() {
        let p = &a.samples[i];
        let mut arc = 0.;
        let mut area = 0.;
        let mut found = None;
        for j in i + 1..(i + 17).min(a.samples.len()) {
            let q = &a.samples[j];
            let prev = &a.samples[j - 1];
            if q.break_before {
                break;
            }
            let (Some(step), Some(net), Some(t0), Some(t1)) =
                (distance(prev, q), distance(p, q), p.time_ms, q.time_ms)
            else {
                break;
            };
            if prev.time_ms.is_none_or(|t| t1 <= t)
                || t1 <= t0
                || t1 - t0 > 30_000
                || net > 10.
                || p.speed_mps.is_some_and(|v| v > 1.5)
                || q.speed_mps.is_some_and(|v| v > 1.5)
            {
                break;
            }
            arc += step;
            if arc > 30. {
                break;
            }
            let lat = p.latitude.unwrap().to_radians();
            let xy = |s: &Sample| {
                (
                    (s.longitude.unwrap() - p.longitude.unwrap()) * 111195. * lat.cos(),
                    (s.latitude.unwrap() - p.latitude.unwrap()) * 111195.,
                )
            };
            let (x, y) = xy(prev);
            let (u, v) = xy(q);
            area += x * v - u * y;
            let height_ok = p
                .elevation_m
                .zip(q.elevation_m)
                .is_some_and(|(a, b)| (a - b).abs() <= net.max(0.04) * 0.25);
            if j >= i + 3 && arc >= 5. && net <= 1. && area.abs() >= 2. && height_ok {
                found = Some(j);
                break;
            }
        }
        if let Some(j) = found {
            result.push(candidate(a,i,j,"tiny_loop",true,"Tiny enclosed GPS loop candidate, not proof of an error. Keep deliberate turns/laps; accepting removes only interior occurrences and preserves endpoint heights."));
            i = j + 1;
        } else {
            i += 1;
        }
    }
    result
}
pub fn load(root: &Path, review: &Review) -> Result<(Book, Option<Vec<u8>>)> {
    let path = root.join("repairs.json");
    let mut book = Book {
        schema_version: 1,
        ..Book::default()
    };
    let raw = if path.exists() {
        let mut bytes = Vec::new();
        fs::File::open(path)?
            .take(1_048_577)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 1_048_576 {
            return Err(error("invalid_repairs", "Repair history exceeds 1 MiB"));
        }
        book =
            serde_json::from_slice(&bytes).map_err(|e| error("invalid_repairs", e.to_string()))?;
        Some(bytes)
    } else {
        None
    };
    if book.schema_version != 1 || book.history.len() > 10000 {
        return Err(error("invalid_repairs", "Unsupported repair history"));
    }
    for a in &review.activities {
        let proposals = candidates(a);
        for (id, d) in book
            .decisions
            .iter()
            .filter(|(_, d)| d.source == a.source_sha256)
        {
            let c = proposals.iter().find(|c| c.id == *id).ok_or_else(|| {
                error(
                    "repair_conflict",
                    "Saved repair no longer matches source samples",
                )
            })?;
            if d.first != c.decision.first
                || d.last != c.decision.last
                || d.kind != c.decision.kind
                || !["pending", "accepted", "rejected"].contains(&d.status.as_str())
                || d.status == "accepted" && !c.allowed
            {
                return Err(error("invalid_repairs", "Invalid or unsafe saved repair"));
            }
        }
    }
    if book.decisions.values().any(|d| {
        !review
            .activities
            .iter()
            .any(|a| a.source_sha256 == d.source)
    }) {
        return Err(error("repair_conflict", "Repair source missing"));
    }
    Ok((book, raw))
}
pub fn preview(review: &Review, book: &Book) -> Option<Preview> {
    let selected = review.document.geometry_source.as_ref()?;
    let a = review
        .activities
        .iter()
        .find(|a| &a.source_sha256 == selected)?;
    let mut candidates = candidates(a);
    let mut removed = BTreeSet::new();
    let mut connected = BTreeSet::new();
    for c in &mut candidates {
        if let Some(d) = book.decisions.get(&c.id) {
            c.decision.status = d.status.clone();
        }
        if c.decision.status == "accepted" {
            if c.decision.kind == "tiny_loop" {
                removed.extend(c.decision.first + 1..c.decision.last);
            }
            connected.insert(c.decision.last);
        }
    }
    let mut samples: Vec<Sample> = Vec::new();
    let mut source_samples = Vec::new();
    let mut station = 0.;
    let mut breaks = 0;
    for (i, raw) in a.samples.iter().enumerate() {
        if removed.contains(&i) {
            continue;
        }
        let mut s = raw.clone();
        if connected.contains(&i) {
            s.break_before = false;
        }
        if let Some(p) = samples.last() {
            if let Some(step) = distance(p, &s) {
                if !s.break_before {
                    station += step;
                }
            } else {
                s.break_before = true;
            }
            if s.break_before {
                breaks += 1;
            }
        }
        s.route_distance_m = station;
        samples.push(s);
        source_samples.push(i);
    }
    Some(Preview {
        candidates,
        samples,
        source_samples,
        before_length_m: a.route_distance_m,
        length_m: station,
        unresolved_breaks: breaks,
    })
}
pub fn decide(
    root: &Path,
    revision: &str,
    id: &str,
    status: &str,
    cancel: Option<&Path>,
) -> Result<Snapshot> {
    cancelled(cancel)?;
    let _guard = lock(root)?;
    let current = checked(root, revision)?;
    let review = current
        .review
        .as_ref()
        .ok_or_else(|| error("not_imported", "Import sources first"))?;
    let (mut book, _) = load(root, review)?;
    let report = preview(review, &book)
        .ok_or_else(|| error("source_required", "Save a geometry source first"))?;
    let proposal = report
        .candidates
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| error("unknown_repair", "Choose a current repair proposal"))?;
    if !["accepted", "rejected", "pending"].contains(&status)
        || status == "accepted" && !proposal.allowed
    {
        return Err(error("unsafe_repair", proposal.reason));
    }
    let mut decision = proposal.decision;
    decision.status = status.into();
    book.history.push(decision.clone());
    book.decisions.insert(id.into(), decision);
    let raw =
        serde_json::to_vec_pretty(&book).map_err(|e| error("invalid_repairs", e.to_string()))?;
    if raw.len() > 1_048_576 || book.history.len() > 10000 {
        return Err(error("invalid_repairs", "Repair history budget reached"));
    }
    cancelled(cancel)?;
    atomic_file(&root.join("repairs.json"), &raw)?;
    open(root)
}
