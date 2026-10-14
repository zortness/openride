//! Portable Ride Project storage. No renderer, interpreter or trainer dependency.
pub mod assembly;
pub mod coastline;
pub mod corrections;
pub mod course;
pub mod imagery;
pub mod imagery_tiles;
pub mod maps;
pub mod package;
pub mod repairs;
pub mod road_course;
pub mod scenery;
pub mod scenery_mesh;
pub mod streets;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, Serialize)]
pub struct Error {
    pub code: &'static str,
    pub message: String,
}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        error("io", format!("Cannot access project: {e}"))
    }
}
fn error(code: &'static str, message: impl Into<String>) -> Error {
    Error {
        code,
        message: message.into(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub kind: String,
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub stage: String,
    pub paths: BTreeMap<String, String>,
}
#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub project: Project,
    pub revision: String,
    pub source_revision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review: Option<ride_core::source_review::Review>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repair_preview: Option<repairs::Preview>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maps: Option<maps::Plan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub course: Option<course::Course>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build: Option<assembly::Build>,
    pub scenery: scenery::View,
}

pub fn paths() -> BTreeMap<String, String> {
    [
        ("sources", "sources/index.json"),
        ("review", "review.json"),
        ("repairs", "repairs.json"),
        ("course", "course.json"),
        ("corrections", "corrections.json"),
        ("scenery", "scenery.json"),
        ("maps", "maps/index.json"),
        ("jobs", "jobs"),
        ("import_log", "logs/import.log"),
        ("authoring_log", "logs/authoring.log"),
        ("package", "build/course"),
        ("exports", "exports"),
    ]
    .into_iter()
    .map(|(k, v)| (k.into(), v.into()))
    .collect()
}
fn validate_name(name: &str) -> Result<()> {
    if name.trim().is_empty() || name.chars().count() > 120 || name.chars().any(char::is_control) {
        return Err(error(
            "invalid_name",
            "Use a project name of 1–120 characters without control characters",
        ));
    }
    Ok(())
}
fn snapshot(raw: &[u8]) -> Result<Snapshot> {
    let value: serde_json::Value = serde_json::from_slice(raw)
        .map_err(|e| error("invalid_project", format!("Invalid project.json: {e}")))?;
    if value.get("kind").and_then(|v| v.as_str()) != Some("openride.ride-project") {
        return Err(error("unsupported_project", "This folder is not a Ride Project. Open older projects with Legacy section builder; migration is not available yet."));
    }
    if value.get("schema_version").and_then(|v| v.as_u64()) != Some(1) {
        return Err(error(
            "unsupported_version",
            "Unsupported project version; use a compatible OpenRide release",
        ));
    }
    let project: Project =
        serde_json::from_value(value).map_err(|e| error("invalid_project", e.to_string()))?;
    validate_name(&project.name)?;
    if project.id.is_empty()
        || project.id.len() > 128
        || !["empty", "imported"].contains(&project.stage.as_str())
        || project.paths != paths()
    {
        return Err(error(
            "invalid_project",
            "Invalid project identity, stage or relative file conventions",
        ));
    }
    Ok(Snapshot {
        project,
        revision: format!("{:x}", Sha256::digest(raw)),
        source_revision: String::new(),
        review: None,
        repair_preview: None,
        maps: None,
        course: None,
        build: None,
        scenery: scenery::View {
            features: vec![],
            warnings: vec![],
            conflicts: vec![],
            revision: String::new(),
        },
    })
}
pub fn open(root: &Path) -> Result<Snapshot> {
    let mut raw = Vec::new();
    fs::File::open(root.join("project.json"))?
        .take(1_048_577)
        .read_to_end(&mut raw)?;
    if raw.len() > 1_048_576 {
        return Err(error("invalid_project", "project.json exceeds 1 MiB"));
    }
    let mut result = snapshot(&raw)?;
    if result.project.stage == "imported" {
        let review =
            ride_core::source_review::open(root).map_err(|e| error("invalid_sources", e))?;
        result.source_revision = review.revision.clone();
        result.revision = format!(
            "{:x}",
            Sha256::digest(format!("{}:{}", result.revision, review.revision))
        );
        let (book, repair_bytes) = repairs::load(root, &review)?;
        if let Some(raw) = repair_bytes {
            result.source_revision = format!(
                "{:x}",
                Sha256::digest(format!(
                    "{}:{:x}",
                    result.source_revision,
                    Sha256::digest(&raw)
                ))
            );
            result.revision = format!(
                "{:x}",
                Sha256::digest(format!("{}:{:x}", result.revision, Sha256::digest(raw)))
            );
        }
        result.repair_preview = repairs::preview(&review, &book);
        result.review = Some(review);
    }
    result.maps = maps::load(root)?.filter(|saved| {
        maps::plan(&result).is_ok_and(|current| {
            current.key == saved.key
                && current.tiles == saved.tiles
                && current.origin == saved.origin
        })
    });
    let edits = corrections::load(root)?;
    let edit_hash = corrections::hash(&edits);
    if !edits.decisions.is_empty() || !edits.history.is_empty() {
        result.revision = format!(
            "{:x}",
            Sha256::digest(format!("{}:{}", result.revision, edit_hash))
        );
    }
    result.course = course::load(
        root,
        &result.source_revision,
        &edit_hash,
        result.maps.as_ref(),
    )?;
    road_course::load(root, &mut result)?;
    result.build = assembly::load(root)?;
    result.scenery = scenery::view(root, result.maps.as_ref())?;
    if let Some(build) = &mut result.build {
        build.current = result.course.as_ref().is_some_and(|course| {
            format!("{:x}", Sha256::digest(serde_json::to_vec(course).unwrap()))
                == build.course_sha256
        }) && build.scenery_revision == result.scenery.revision
            && build.project_name == result.project.name;
        build.current = build.current
            && result.maps.as_ref().is_some_and(|plan| {
                assembly::map_revision(root, plan)
                    .is_ok_and(|revision| revision == build.map_revision)
            });
    }
    if root.join("scenery.json").exists()
        || result.maps.as_ref().is_some_and(|p| {
            root.join("maps")
                .join(&p.key)
                .join("scenery-catalog.json")
                .exists()
        })
    {
        result.revision = format!(
            "{:x}",
            Sha256::digest(format!("{}:{}", result.revision, result.scenery.revision))
        );
    }
    Ok(result)
}
struct Lock(PathBuf);
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn lock(root: &Path) -> Result<Lock> {
    lock_named(root, "project.lock")
}
fn lock_named(root: &Path, filename: &str) -> Result<Lock> {
    let path = root.join(filename);
    let mut file=OpenOptions::new().write(true).create_new(true).open(&path)
        .map_err(|e| error("locked", format!("Cannot lock project: {e}. Another writer may be active. If an interrupted job left {filename}, confirm no writer is running before removing it.")))?;
    let guard = Lock(path);
    writeln!(file, "{}", std::process::id())?;
    Ok(guard)
}
fn cancelled(cancel: Option<&Path>) -> Result<()> {
    if cancel.is_some_and(Path::exists) {
        Err(error(
            "cancelled",
            "Operation cancelled; previously completed files remain saved",
        ))
    } else {
        Ok(())
    }
}
fn publish(root: &Path, project: Project, cancel: Option<&Path>) -> Result<Snapshot> {
    let raw =
        serde_json::to_vec_pretty(&project).map_err(|e| error("invalid_project", e.to_string()))?;
    let result = snapshot(&raw)?;
    cancelled(cancel)?;
    let temporary = root.join("project.json.pending");
    let mut file=OpenOptions::new().write(true).create_new(true).open(&temporary)
        .map_err(|e|error("pending_write",format!("Cannot stage project: {e}. Saved project.json is intact; inspect project.json.pending from any interrupted write.")))?;
    let write_result = (|| -> Result<()> {
        file.write_all(&raw)?;
        file.sync_all()?;
        drop(file);
        cancelled(cancel)?;
        fs::rename(&temporary, root.join("project.json"))?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    write_result?;
    Ok(result)
}
pub fn create(root: &Path, name: &str, cancel: Option<&Path>) -> Result<Snapshot> {
    validate_name(name)?;
    cancelled(cancel)?;
    fs::create_dir_all(root)?;
    let _guard = lock(root)?;
    if fs::read_dir(root)?.any(|entry| {
        entry
            .map(|e| e.file_name() != "project.lock")
            .unwrap_or(true)
    }) {
        return Err(error(
            "not_empty",
            "Choose a new or empty folder. Existing files will not be replaced.",
        ));
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| error("clock", e.to_string()))?
        .as_nanos();
    publish(
        root,
        Project {
            kind: "openride.ride-project".into(),
            schema_version: 1,
            id: format!("{stamp:x}-{:x}", std::process::id()),
            name: name.trim().into(),
            stage: "empty".into(),
            paths: paths(),
        },
        cancel,
    )
}
pub fn rename(root: &Path, revision: &str, name: &str, cancel: Option<&Path>) -> Result<Snapshot> {
    validate_name(name)?;
    cancelled(cancel)?;
    let _guard = lock(root)?;
    let mut current = open(root)?;
    if current.revision != revision {
        return Err(error(
            "stale_revision",
            "Project changed since opening. Reopen before saving.",
        ));
    }
    current.project.name = name.trim().into();
    publish(root, current.project, cancel)?;
    open(root)
}

fn checked(root: &Path, revision: &str) -> Result<Snapshot> {
    let current = open(root)?;
    if current.revision != revision {
        return Err(error(
            "stale_revision",
            "Project changed since opening. Reopen before saving.",
        ));
    }
    Ok(current)
}

fn atomic_file(path: &Path, raw: &[u8]) -> Result<()> {
    let pending = path.with_extension("pending");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pending)?;
    file.write_all(raw)?;
    file.sync_all()?;
    drop(file);
    fs::rename(pending, path)?;
    Ok(())
}

/// Validate all variants before publishing. Project stage is the commit marker.
/// Failed/cancelled intake may retain verified snapshots and private staging;
/// retrying never replaces an original snapshot or exposes a partial intake.
pub fn import(
    root: &Path,
    revision: &str,
    sources: &[PathBuf],
    cancel: Option<&Path>,
) -> Result<Snapshot> {
    cancelled(cancel)?;
    let _guard = lock(root)?;
    let mut current = checked(root, revision)?;
    if current.project.stage != "empty" {
        return Err(error("already_imported", "Sources are already imported. Open this project's saved review, or create a new project for another recording."));
    }
    fs::create_dir_all(root.join("jobs"))?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| error("clock", e.to_string()))?
        .as_nanos();
    let staging = root.join("jobs").join(format!("import-{stamp:x}"));
    let mut review = ride_core::source_review::create(&staging, sources)
        .map_err(|e| error("import_failed", e))?;
    cancelled(cancel)?;
    fs::create_dir_all(root.join("sources"))?;
    for source in &mut review.document.sources {
        cancelled(cancel)?;
        let raw = fs::read(staging.join(&source.file))?;
        let extension = Path::new(&source.file)
            .extension()
            .unwrap()
            .to_str()
            .unwrap();
        source.file = format!("sources/{}.{}", source.sha256, extension);
        let destination = root.join(&source.file);
        if destination.exists() {
            if fs::read(&destination)? != raw {
                return Err(error(
                    "snapshot_conflict",
                    "An existing source snapshot was changed; original files will not be replaced.",
                ));
            }
        } else {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(destination)?;
            file.write_all(&raw)?;
            file.sync_all()?;
        }
    }
    let raw = serde_json::to_vec_pretty(&review.document)
        .map_err(|e| error("import_failed", e.to_string()))?;
    let index = serde_json::to_vec_pretty(&serde_json::json!({"kind":"openride.sources", "schema_version":1,"sources":review.document.sources})).unwrap();
    cancelled(cancel)?;
    atomic_file(&root.join("sources/index.json"), &index)?;
    atomic_file(&root.join("review.json"), &raw)?;
    // Revalidate snapshot identity before making the imported stage visible.
    ride_core::source_review::open(root).map_err(|e| error("invalid_sources", e))?;
    fs::create_dir_all(root.join("logs"))?;
    let mut log = String::from("Complete recording import; source units normalized by the native importer. No repairs or road matching applied.\n");
    for activity in &review.activities {
        log.push_str(&format!(
            "{} [{}]: {} samples, {:.3} m geometry excluding gaps\n",
            activity.source_name,
            activity.format,
            activity.samples.len(),
            activity.route_distance_m
        ));
        for diagnostic in &activity.diagnostics {
            log.push_str(&format!("{diagnostic:?}\n"));
        }
    }
    atomic_file(&root.join("logs/import.log"), log.as_bytes())?;
    current.project.stage = "imported".into();
    publish(root, current.project, cancel)?;
    open(root)
}

/// Source choice is a review operation, never an implicit repair or concatenation.
pub fn select_sources(
    root: &Path,
    revision: &str,
    geometry: &str,
    elevation: &str,
    cancel: Option<&Path>,
) -> Result<Snapshot> {
    cancelled(cancel)?;
    let _guard = lock(root)?;
    let _review_guard = lock_named(root, "review.lock")?;
    let current = checked(root, revision)?;
    let mut review = current
        .review
        .ok_or_else(|| error("not_imported", "Import source files first"))?;
    for id in [geometry, elevation] {
        if !review.document.sources.iter().any(|s| s.sha256 == id) {
            return Err(error(
                "unknown_source",
                "Select geometry and elevation from this project's imported sources",
            ));
        }
    }
    review.document.geometry_source = Some(geometry.into());
    review.document.elevation_source = Some(elevation.into());
    let raw = serde_json::to_vec_pretty(&review.document)
        .map_err(|e| error("invalid_review", e.to_string()))?;
    cancelled(cancel)?;
    atomic_file(&root.join("review.json"), &raw)?;
    open(root)
}
