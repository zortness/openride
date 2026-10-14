use openride_author::{create, open, rename};
use openride_author::{import, select_sources};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
const GPX: &str = include_str!("../../../tools/fixtures/ride-project.gpx");
const TCX: &str = include_str!("../../../tools/fixtures/ride-project.tcx");

#[test]
fn complete_pair_reopens_after_move_and_keeps_originals_missing_values_and_gaps() {
    let fixture = Fixture::new();
    let root = fixture.0.join("Project");
    let gpx = fixture.0.join("Base ride.gpx");
    let tcx = fixture.0.join("Base ride.tcx");
    fs::write(&gpx, GPX).unwrap();
    fs::write(&tcx, TCX).unwrap();
    let empty = create(&root, "Whole ride", None).unwrap();
    let imported = import(&root, &empty.revision, &[gpx.clone(), tcx.clone()], None).unwrap();
    let review = imported.review.as_ref().unwrap();
    assert_eq!(review.activities.len(), 2);
    assert!(review
        .activities
        .iter()
        .all(|a| a.route_distance_m > 5000.0 && a.samples.len() == 5));
    assert_eq!(review.activities[1].samples[2].elevation_m, None);
    assert_eq!(review.gaps.len(), 2);
    assert!(review.document.geometry_source.is_none());
    let geometry = review.document.sources[0].sha256.clone();
    let elevation = review.document.sources[1].sha256.clone();
    let selected = select_sources(&root, &imported.revision, &geometry, &elevation, None).unwrap();
    assert!(selected
        .review
        .as_ref()
        .unwrap()
        .document
        .repairs
        .is_empty());
    assert!(select_sources(&root, &imported.revision, &geometry, &elevation, None).is_err());
    assert_eq!(fs::read_to_string(&gpx).unwrap(), GPX);
    assert_eq!(fs::read_to_string(&tcx).unwrap(), TCX);
    fs::remove_file(gpx).unwrap();
    fs::remove_file(tcx).unwrap();
    let moved = fixture.0.join("Moved project");
    fs::rename(root, &moved).unwrap();
    let reopened = open(&moved).unwrap();
    assert_eq!(reopened.revision, selected.revision);
    let review = reopened.review.unwrap();
    assert_eq!(review.document.geometry_source, Some(geometry));
    assert_eq!(review.document.elevation_source, Some(elevation));
    assert!(moved.join("logs/import.log").is_file());
    assert!(moved.join("sources/index.json").is_file());
    for source in &review.document.sources {
        assert!(moved.join(&source.file).is_file());
    }
    let renamed = rename(&moved, &selected.revision, "Renamed imported ride", None).unwrap();
    assert!(renamed.review.is_some());
    assert_eq!(renamed.revision, open(&moved).unwrap().revision);
    fs::write(moved.join(&review.document.sources[0].file), b"corruption").unwrap();
    assert_eq!(open(&moved).unwrap_err().code, "invalid_sources");
}

#[test]
fn failed_duplicate_unrelated_cancelled_and_stale_imports_leave_project_empty() {
    let fixture = Fixture::new();
    let root = fixture.0.join("Project");
    let source = fixture.0.join("Ride.gpx");
    fs::write(&source, GPX).unwrap();
    let other = fixture.0.join("Other.gpx");
    fs::write(&other, GPX.replace("2022-01-01", "2023-01-01")).unwrap();
    let empty = create(&root, "Ride", None).unwrap();
    assert!(import(
        &root,
        &empty.revision,
        &[source.clone(), source.clone()],
        None
    )
    .is_err());
    assert!(import(&root, &empty.revision, &[source.clone(), other], None).is_err());
    assert!(import(&root, "stale", &[source.clone()], None).is_err());
    let cancel = fixture.0.join("cancel");
    fs::write(&cancel, b"").unwrap();
    assert_eq!(
        import(&root, &empty.revision, &[source.clone()], Some(&cancel))
            .unwrap_err()
            .code,
        "cancelled"
    );
    assert_eq!(open(&root).unwrap().revision, empty.revision);
    let imported = import(&root, &empty.revision, &[source.clone()], None).unwrap();
    assert_eq!(
        import(&root, &imported.revision, &[source], None)
            .unwrap_err()
            .code,
        "already_imported"
    );
    assert_eq!(
        select_sources(&root, &imported.revision, "unknown", "unknown", None)
            .unwrap_err()
            .code,
        "unknown_source"
    );
}

#[test]
fn real_cli_import_and_selection_roundtrip() {
    let fixture = Fixture::new();
    let root = fixture.0.join("CLI project");
    let source = fixture.0.join("My recording.gpx");
    fs::write(&source, GPX).unwrap();
    let empty = create(&root, "Ride", None).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_openride-author"))
        .args([
            "project",
            "import",
            root.to_str().unwrap(),
            "--expected-revision",
            &empty.revision,
            "--source",
            source.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let imported = open(&root).unwrap();
    let hash = &imported.review.as_ref().unwrap().document.sources[0].sha256;
    let output = Command::new(env!("CARGO_BIN_EXE_openride-author"))
        .args([
            "project",
            "select-sources",
            root.to_str().unwrap(),
            "--expected-revision",
            &imported.revision,
            "--geometry",
            hash,
            "--elevation",
            hash,
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        open(&root)
            .unwrap()
            .review
            .unwrap()
            .document
            .geometry_source
            .as_ref(),
        Some(hash)
    );
}
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "openride author test {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn move_reopen_rename_and_reject_stale_writer() {
    let fixture = Fixture::new();
    let root = fixture.0.join("My Ride");
    let first = create(&root, "Mountain ride", None).unwrap();
    let moved = fixture.0.join("Moved Ride");
    fs::rename(&root, &moved).unwrap();
    let reopened = open(&moved).unwrap();
    assert_eq!(first.revision, reopened.revision);
    assert_eq!(first.project.id, reopened.project.id);
    assert!(reopened
        .project
        .paths
        .values()
        .all(|p| !PathBuf::from(p).is_absolute()));
    let updated = rename(&moved, &first.revision, "Coast ride", None).unwrap();
    assert_eq!(open(&moved).unwrap().project.name, "Coast ride");
    assert_eq!(
        rename(&moved, &first.revision, "Stale", None)
            .unwrap_err()
            .code,
        "stale_revision"
    );
    assert_eq!(updated.revision, open(&moved).unwrap().revision);
}
#[test]
fn interrupted_writes_locks_and_cancel_preserve_saved_bytes() {
    let fixture = Fixture::new();
    let root = fixture.0.join("Ride");
    let first = create(&root, "Ride", None).unwrap();
    let saved = fs::read(root.join("project.json")).unwrap();
    fs::write(root.join("project.json.pending"), b"incomplete").unwrap();
    assert_eq!(
        rename(&root, &first.revision, "Changed", None)
            .unwrap_err()
            .code,
        "pending_write"
    );
    assert_eq!(open(&root).unwrap().revision, first.revision);
    fs::remove_file(root.join("project.json.pending")).unwrap();
    fs::write(root.join("project.lock"), b"other writer").unwrap();
    assert_eq!(
        rename(&root, &first.revision, "Changed", None)
            .unwrap_err()
            .code,
        "locked"
    );
    fs::remove_file(root.join("project.lock")).unwrap();
    let cancel = fixture.0.join("cancel");
    fs::write(&cancel, b"").unwrap();
    assert_eq!(
        rename(&root, &first.revision, "Changed", Some(&cancel))
            .unwrap_err()
            .code,
        "cancelled"
    );
    assert_eq!(saved, fs::read(root.join("project.json")).unwrap());
    assert!(create(&fixture.0.join("Cancelled"), "Cancelled", Some(&cancel)).is_err());
    assert!(!fixture.0.join("Cancelled").exists());
}
#[test]
fn reject_unknown_version_legacy_unsafe_paths_and_nonempty_folders() {
    let fixture = Fixture::new();
    let root = fixture.0.join("Ride");
    let first = create(&root, "Ride", None).unwrap();
    assert_eq!(
        create(&root, "Replacement", None).unwrap_err().code,
        "not_empty"
    );
    let mut value = serde_json::to_value(first.project).unwrap();
    value["schema_version"] = 2.into();
    fs::write(root.join("project.json"), value.to_string()).unwrap();
    assert_eq!(open(&root).unwrap_err().code, "unsupported_version");
    value["schema_version"] = 1.into();
    value["paths"]["review"] = "../escape.json".into();
    fs::write(root.join("project.json"), value.to_string()).unwrap();
    assert_eq!(open(&root).unwrap_err().code, "invalid_project");
    fs::write(root.join("project.json"), b"{\"schema_version\":1}").unwrap();
    assert_eq!(open(&root).unwrap_err().code, "unsupported_project");
}
#[test]
fn cli_protocol_exit_codes_and_paths_with_spaces() {
    let fixture = Fixture::new();
    let root = fixture.0.join("CLI Ride");
    let invoke = |args: Vec<&str>| {
        Command::new(env!("CARGO_BIN_EXE_openride-author"))
            .args(args)
            .output()
            .unwrap()
    };
    let folder = root.to_str().unwrap();
    for action in ["create", "open", "status"] {
        let mut args = vec!["project", action, folder];
        if action == "create" {
            args.extend(["--name", "CLI Ride"]);
        }
        let result = invoke(args);
        assert!(result.status.success(), "{:?}", result);
        let events: Vec<serde_json::Value> = String::from_utf8(result.stdout)
            .unwrap()
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        assert_eq!(events[0]["event"], "progress");
        assert_eq!(events.last().unwrap()["status"], "complete");
        assert_eq!(events.last().unwrap()["protocol_version"], 1);
    }
    assert_eq!(
        invoke(vec!["project", "create", folder, "--name", "Again"])
            .status
            .code(),
        Some(2)
    );
    assert_eq!(
        invoke(vec!["project", "status", folder, "--name", "Ignored?"])
            .status
            .code(),
        Some(2)
    );
    let cancel = fixture.0.join("cancel");
    fs::write(&cancel, b"").unwrap();
    let result = invoke(vec![
        "project",
        "status",
        folder,
        "--cancel-file",
        cancel.to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(3));
    assert!(String::from_utf8(result.stdout)
        .unwrap()
        .contains("\"status\":\"cancelled\""));
}
