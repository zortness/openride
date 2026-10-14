//! Explicit source-occurrence edits, bound to the source and acquired map revisions.
use crate::{cancelled, checked, course, error, lock, maps, open, Result, Snapshot};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Edit {
    pub source_sample: usize,
    pub mode: String,
    pub position: Option<[f64; 2]>,
    pub way: Option<i64>,
}
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Book {
    pub schema_version: u32,
    pub source_revision: String,
    pub map_key: String,
    pub map_sources: BTreeMap<String, String>,
    pub decisions: BTreeMap<usize, Edit>,
    pub history: Vec<Edit>,
}
pub fn hash(book: &Book) -> String {
    format!("{:x}", Sha256::digest(serde_json::to_vec(book).unwrap()))
}
pub fn load(root: &Path) -> Result<Book> {
    let path = root.join("corrections.json");
    if !path.exists() {
        return Ok(Book {
            schema_version: 1,
            ..Default::default()
        });
    }
    if fs::metadata(&path)?.len() > 4 * 1024 * 1024 {
        return Err(error("invalid_corrections", "Edits exceed 4 MiB"));
    }
    let book: Book = serde_json::from_slice(&fs::read(path)?)
        .map_err(|e| error("invalid_corrections", e.to_string()))?;
    if book.schema_version != 1 || book.history.len() > 10000 {
        return Err(error(
            "invalid_corrections",
            "Unsupported or excessive edit history",
        ));
    }
    for (&id, e) in &book.decisions {
        if id != e.source_sample
            || !["trace", "point", "way", "join"].contains(&e.mode.as_str())
            || e.position.is_some_and(|p| {
                !p[0].is_finite() || !p[1].is_finite() || p[0].abs() > 80. || p[1].abs() > 180.
            })
        {
            return Err(error("invalid_corrections", "Invalid saved edit"));
        }
    }
    Ok(book)
}
pub fn validate(
    book: &Book,
    source_revision: &str,
    plan: &maps::Plan,
    sources: &BTreeMap<String, String>,
) -> Result<()> {
    if !book.decisions.is_empty()
        && (book.source_revision != source_revision
            || book.map_key != plan.key
            || book.map_sources != *sources)
    {
        return Err(error("correction_conflict","Sources or street maps changed. Saved edits are retained; archive stale edits before rebuilding, then review their replacements."));
    }
    Ok(())
}
pub fn decide(root: &Path, revision: &str, edit: Edit, cancel: Option<&Path>) -> Result<Snapshot> {
    let _guard = lock(root)?;
    let current = checked(root, revision)?;
    let preview = current
        .repair_preview
        .as_ref()
        .ok_or_else(|| error("source_required", "Select source roles"))?;
    let index = preview
        .source_samples
        .iter()
        .position(|n| *n == edit.source_sample)
        .ok_or_else(|| {
            error(
                "invalid_edit",
                "Source occurrence is not in the repaired trace",
            )
        })?;
    if !["trace", "point", "way", "join", "undo"].contains(&edit.mode.as_str()) {
        return Err(error(
            "invalid_edit",
            "Choose trace, point, way, join or undo",
        ));
    }
    if edit.mode == "point" || edit.mode == "join" {
        let p = edit
            .position
            .ok_or_else(|| error("invalid_edit", "Edited point needs latitude and longitude"))?;
        let s = &preview.samples[index];
        let valid = s.latitude.zip(s.longitude).is_some_and(|(a, b)| {
            p.iter().all(|v| v.is_finite())
                && p[0].abs() < 80.
                && p[1].abs() <= 180.
                && ride_core::haversine(a, b, p[0], p[1]) <= 5000.
        });
        if !valid {
            return Err(error(
                "invalid_edit",
                "Edited point must be within 5 km of its source occurrence",
            ));
        }
    }
    if edit.mode == "way" && edit.way.is_none() {
        return Err(error("invalid_edit", "Choose a road/path ID"));
    }
    let plan = maps::plan(&current)?;
    let graph = maps::street_graph(root, &plan, cancel)?;
    if graph.sources.is_empty() {
        return Err(error(
            "maps_required",
            "Acquire street coverage before editing",
        ));
    }
    let mut book = load(root)?;
    validate(&book, &current.source_revision, &plan, &graph.sources)?;
    book.source_revision = current.source_revision.clone();
    book.map_key = plan.key.clone();
    book.map_sources = graph.sources.clone();
    if edit.mode == "undo" {
        book.decisions.remove(&edit.source_sample);
    } else {
        book.decisions.insert(edit.source_sample, edit.clone());
    }
    book.history.push(edit);
    if book.history.len() > 10000 {
        return Err(error("invalid_edit", "Edit history is full"));
    }
    // Validate against actual graph/source data before publishing the decision.
    course::match_with_edits(
        &preview.samples,
        &preview.source_samples,
        &graph,
        &book.decisions,
        cancel,
    )?;
    cancelled(cancel)?;
    maps::atomic_file(
        &root.join("corrections.json"),
        &serde_json::to_vec_pretty(&book).unwrap(),
    )?;
    drop(_guard);
    let saved = open(root)?;
    course::build(root, &saved.revision, cancel)
}
pub fn archive(root: &Path, revision: &str, cancel: Option<&Path>) -> Result<Snapshot> {
    let _guard = lock(root)?;
    checked(root, revision)?;
    let book = load(root)?;
    cancelled(cancel)?;
    fs::create_dir_all(root.join("jobs"))?;
    maps::atomic_file(
        &root
            .join("jobs")
            .join(format!("corrections-{}.json", hash(&book))),
        &serde_json::to_vec_pretty(&book).unwrap(),
    )?;
    maps::atomic_file(
        &root.join("corrections.json"),
        &serde_json::to_vec(&Book {
            schema_version: 1,
            ..Default::default()
        })
        .unwrap(),
    )?;
    open(root)
}
