//! Private full-recording review. Classifications never modify source geometry.
use crate::{import_xml, Activity, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const SOURCE_LIMIT: u64 = 64 * 1024 * 1024;
const DOCUMENT_LIMIT: u64 = 1024 * 1024;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub sha256: String,
    pub name: String,
    pub file: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Decision {
    pub classification: String,
    pub note: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub kind: String,
    pub schema_version: u32,
    pub sources: Vec<Source>,
    pub geometry_source: Option<String>,
    pub elevation_source: Option<String>,
    pub decisions: BTreeMap<String, Decision>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub repairs: BTreeMap<String, crate::route_repairs::Repair>,
}
#[derive(Debug, Serialize)]
pub struct Gap {
    pub id: String,
    pub source_sha256: String,
    pub before_sample: usize,
    pub after_sample: usize,
    pub segment: usize,
    pub distance_m: f64,
    pub elapsed_ms: Option<i64>,
    pub separation_m: Option<f64>,
    pub reasons: Vec<String>,
}
#[derive(Debug, Serialize)]
pub struct Review {
    pub document: Document,
    pub revision: String,
    pub activities: Vec<Activity>,
    pub gaps: Vec<Gap>,
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn read(path: &Path, limit: u64) -> Result<Vec<u8>> {
    if !fs::symlink_metadata(path)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_file()
    {
        return Err("Review inputs must be regular files".into());
    }
    let mut data = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(limit + 1)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() as u64 > limit {
        return Err("Review input exceeds size budget".into());
    }
    Ok(data)
}
fn activity(bytes: &[u8], name: &str) -> Result<Activity> {
    let a = import_xml(std::str::from_utf8(bytes).map_err(|e| e.to_string())?, name)?;
    if !matches!(a.format.as_str(), "GPX" | "TCX") {
        return Err("Full ride review supports GPX and TCX recordings".into());
    }
    if a.samples.len() < 2 || a.samples.len() > 100_000 {
        return Err("Review needs 2–100,000 source samples".into());
    }
    Ok(a)
}
fn gaps(activities: &[Activity]) -> Vec<Gap> {
    let mut result = vec![];
    for a in activities {
        for (i, s) in a
            .samples
            .iter()
            .enumerate()
            .skip(1)
            .filter(|(_, s)| s.break_before)
        {
            let p = &a.samples[i - 1];
            let mut reasons: Vec<_> = a
                .diagnostics
                .iter()
                .filter(|d| d.sample == Some(i))
                .map(|d| d.code.clone())
                .collect();
            if p.latitude.is_none() || s.latitude.is_none() {
                reasons.push("missing_position_boundary".into());
            }
            if reasons.is_empty() {
                reasons.push("source_break".into());
            }
            result.push(Gap {
                id: format!("{}:{i}", a.source_sha256),
                source_sha256: a.source_sha256.clone(),
                before_sample: i - 1,
                after_sample: i,
                segment: s.segment,
                distance_m: s.route_distance_m,
                elapsed_ms: p.time_ms.zip(s.time_ms).map(|(x, y)| y - x),
                separation_m: p
                    .latitude
                    .zip(p.longitude)
                    .zip(s.latitude.zip(s.longitude))
                    .map(|((a, b), (c, d))| crate::haversine(a, b, c, d)),
                reasons,
            });
        }
    }
    result
}
fn validate(document: &Document, gaps: &[Gap]) -> Result<()> {
    for selected in [&document.geometry_source, &document.elevation_source]
        .into_iter()
        .flatten()
    {
        if !document.sources.iter().any(|s| &s.sha256 == selected) {
            return Err("Selected source is not in this review".into());
        }
    }
    for (id, decision) in &document.decisions {
        if !gaps.iter().any(|g| &g.id == id) {
            return Err("Gap decision does not match a source boundary".into());
        }
        if !matches!(
            decision.classification.as_str(),
            "unresolved" | "keep_break" | "stationary_pause" | "missing_travel"
        ) || decision.note.len() > 2000
        {
            return Err("Invalid gap classification or note exceeds 2,000 bytes".into());
        }
    }
    Ok(())
}
pub fn create(directory: &Path, paths: &[PathBuf]) -> Result<Review> {
    if paths.is_empty() || paths.len() > 8 {
        return Err("Select 1–8 source variants for one ride".into());
    }
    let mut sources = vec![];
    let mut activities = vec![];
    let mut originals = vec![];
    let mut total = 0;
    for (i, path) in paths.iter().enumerate() {
        let bytes = read(path, SOURCE_LIMIT)?;
        total += bytes.len();
        if total > 128 * 1024 * 1024 {
            return Err("Combined review sources exceed 128 MiB".into());
        }
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("Invalid source filename")?;
        let a = activity(&bytes, name)?;
        if activities
            .iter()
            .any(|v: &Activity| v.source_sha256 == a.source_sha256)
        {
            return Err("Duplicate source selected".into());
        }
        if activities
            .first()
            .is_some_and(|first| !crate::same_ride(first, &a))
        {
            return Err("Select variants of one recording; these sources do not match".into());
        }
        sources.push(Source {
            sha256: a.source_sha256.clone(),
            name: name.into(),
            file: format!("source-{i}.{}", a.format.to_lowercase()),
        });
        activities.push(a);
        originals.push(bytes);
    }
    let document = Document {
        kind: "openride.source-review".into(),
        schema_version: 1,
        sources,
        geometry_source: None,
        elevation_source: None,
        decisions: BTreeMap::new(),
        repairs: BTreeMap::new(),
    };
    let bytes = serde_json::to_vec_pretty(&document).map_err(|e| e.to_string())?;
    // Validate everything before creating an exclusive new project directory.
    fs::create_dir(directory).map_err(|e| format!("Choose a new review folder: {e}"))?;
    for (s, data) in document.sources.iter().zip(originals) {
        write_new(&directory.join(&s.file), &data)?;
    }
    write_new(&directory.join("review.json"), &bytes)?;
    Ok(Review {
        gaps: gaps(&activities),
        activities,
        document,
        revision: hash(&bytes),
    })
}
pub fn open(directory: &Path) -> Result<Review> {
    let bytes = read(&directory.join("review.json"), DOCUMENT_LIMIT)?;
    let document: Document = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if document.kind != "openride.source-review"
        || document.schema_version != 1
        || document.sources.is_empty()
        || document.sources.len() > 8
    {
        return Err("Unsupported source review".into());
    }
    let mut activities = vec![];
    let mut total = 0;
    for (i, s) in document.sources.iter().enumerate() {
        let project_path = s.sha256.len() == 64
            && s.sha256.bytes().all(|c| c.is_ascii_hexdigit())
            && [
                format!("sources/{}.gpx", s.sha256),
                format!("sources/{}.tcx", s.sha256),
            ]
            .contains(&s.file);
        if !project_path
            && ![format!("source-{i}.gpx"), format!("source-{i}.tcx")].contains(&s.file)
        {
            return Err("Invalid source snapshot path".into());
        }
        let raw = read(&directory.join(&s.file), SOURCE_LIMIT)?;
        total += raw.len();
        if total > 128 * 1024 * 1024 {
            return Err("Combined review sources exceed 128 MiB".into());
        }
        if hash(&raw) != s.sha256 {
            return Err(format!(
                "Source snapshot changed: {}. Review cannot be applied.",
                s.name
            ));
        }
        let a = activity(&raw, &s.name)?;
        if activities
            .iter()
            .any(|v: &Activity| v.source_sha256 == a.source_sha256)
        {
            return Err("Duplicate source snapshot".into());
        }
        if activities
            .first()
            .is_some_and(|first| !crate::same_ride(first, &a))
        {
            return Err("Source snapshots are not variants of one recording".into());
        }
        activities.push(a);
    }
    let gaps = gaps(&activities);
    validate(&document, &gaps)?;
    let review = Review {
        document,
        revision: hash(&bytes),
        activities,
        gaps,
    };
    crate::route_repairs::validate(&review)?;
    Ok(review)
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    f.write_all(bytes)
        .and_then(|_| f.sync_all())
        .map_err(|e| e.to_string())
}
struct Lock(PathBuf);
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
pub fn save(directory: &Path, revision: &str, document: Document) -> Result<Review> {
    let lock = directory.join("review.lock");
    write_new(&lock, b"Review save in progress")
        .map_err(|e| format!("Review is locked; another save may be active: {e}"))?;
    let _lock = Lock(lock);
    let current = open(directory)?;
    if current.revision != revision {
        return Err("Review changed since opening; reopen before saving".into());
    }
    if document.sources != current.document.sources
        || document.kind != current.document.kind
        || document.schema_version != current.document.schema_version
    {
        return Err(
            "Source identity and review schema cannot be changed by a decision save".into(),
        );
    }
    validate(&document, &current.gaps)?;
    let mut checked = Review {
        document,
        revision: current.revision.clone(),
        activities: current.activities.clone(),
        gaps: gaps(&current.activities),
    };
    crate::route_repairs::validate(&checked)?;
    crate::route_repairs::apply_automatic(&mut checked)?;
    crate::route_repairs::validate(&checked)?;
    let document = checked.document;
    let raw = serde_json::to_vec_pretty(&document).map_err(|e| e.to_string())?;
    if raw.len() as u64 > DOCUMENT_LIMIT {
        return Err("Review decisions exceed 1 MiB".into());
    }
    let temporary = directory.join("review.json.tmp");
    write_new(&temporary, &raw)?;
    if let Err(e) = fs::rename(&temporary, directory.join("review.json")) {
        let _ = fs::remove_file(&temporary);
        return Err(e.to_string());
    }
    Ok(Review {
        document,
        revision: hash(&raw),
        ..current
    })
}
