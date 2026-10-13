use ride_core::source_review::{self as review, Decision};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
fn workspace() -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "openride-review-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&p).unwrap();
    p
}
fn fixture() -> &'static str {
    r#"<gpx><trk><name>Synthetic long ride</name><trkseg>
    <trkpt lat="33" lon="-118"><time>2022-01-01T00:00:00Z</time><ele>10</ele></trkpt>
    <trkpt lat="33.1" lon="-118"><time>2022-01-01T00:00:30Z</time><ele>500</ele></trkpt>
    <trkpt lat="33.11" lon="-118"><time>2022-01-01T00:05:00Z</time><ele>510</ele></trkpt>
    <trkpt lat="33.12" lon="-118"><time>2022-01-01T00:05:30Z</time><ele>490</ele></trkpt>
    </trkseg></trk></gpx>"#
}
#[test]
fn long_review_reopens_source_choices_and_classification_without_joining_gap() {
    let root = workspace();
    let source = root.join("ride.gpx");
    fs::write(&source, fixture()).unwrap();
    let destination = root.join("review");
    let created = review::create(&destination, &[source.clone()]).unwrap();
    assert!(created.activities[0].route_distance_m > 5000.);
    assert_eq!(created.gaps.len(), 1);
    let mut document = created.document.clone();
    document.geometry_source = Some(document.sources[0].sha256.clone());
    document.elevation_source = document.geometry_source.clone();
    document.decisions.insert(
        created.gaps[0].id.clone(),
        Decision {
            classification: "stationary_pause".into(),
            note: "Reviewed stop; preserve break".into(),
        },
    );
    let saved = review::save(&destination, &created.revision, document).unwrap();
    fs::remove_file(&source).unwrap();
    let reopened = review::open(&destination).unwrap();
    assert_eq!(reopened.document, saved.document);
    assert!(reopened.activities[0].samples[2].break_before);
    assert_eq!(
        reopened.activities[0].samples[2].route_distance_m,
        reopened.activities[0].samples[1].route_distance_m
    );
    assert_eq!(reopened.activities[0].samples.len(), 4);
    assert!(review::save(&destination, &created.revision, created.document).is_err());
}

#[test]
fn variants_preserve_independent_decisions_and_invalid_saves_leave_review_intact() {
    let root = workspace();
    let a = root.join("a.gpx");
    let b = root.join("b.gpx");
    fs::write(&a, fixture()).unwrap();
    fs::write(&b, fixture().replace("<ele>500</ele>", "<ele>505</ele>")).unwrap();
    let directory = root.join("review");
    let created = review::create(&directory, &[a, b]).unwrap();
    assert_eq!(created.activities.len(), 2);
    assert_eq!(created.gaps.len(), 2);
    assert_ne!(created.gaps[0].id, created.gaps[1].id);
    let mut doc = created.document.clone();
    doc.geometry_source = Some(doc.sources[0].sha256.clone());
    doc.elevation_source = Some(doc.sources[1].sha256.clone());
    doc.decisions.insert(
        created.gaps[0].id.clone(),
        Decision {
            classification: "keep_break".into(),
            note: "Leave intact".into(),
        },
    );
    let saved = review::save(&directory, &created.revision, doc).unwrap();
    assert!(!saved.document.decisions.contains_key(&created.gaps[1].id));
    let before = fs::read(directory.join("review.json")).unwrap();
    let mut bad = saved.document.clone();
    bad.geometry_source = Some("unknown".into());
    assert!(review::save(&directory, &saved.revision, bad).is_err());
    let mut bad = saved.document.clone();
    bad.decisions.insert(
        "unknown:2".into(),
        Decision {
            classification: "keep_break".into(),
            note: String::new(),
        },
    );
    assert!(review::save(&directory, &saved.revision, bad).is_err());
    let mut bad = saved.document.clone();
    bad.decisions
        .get_mut(&created.gaps[0].id)
        .unwrap()
        .classification = "join".into();
    assert!(review::save(&directory, &saved.revision, bad).is_err());
    assert_eq!(fs::read(directory.join("review.json")).unwrap(), before);
    fs::write(directory.join("review.lock"), "other writer").unwrap();
    assert!(review::save(&directory, &saved.revision, saved.document).is_err());
    assert_eq!(fs::read(directory.join("review.json")).unwrap(), before);
}

#[test]
fn rejects_tampered_snapshots_unsafe_paths_and_reusing_a_directory() {
    let root = workspace();
    let source = root.join("ride.gpx");
    fs::write(&source, fixture()).unwrap();
    let directory = root.join("review");
    let created = review::create(&directory, &[source.clone()]).unwrap();
    assert!(review::create(&directory, &[source.clone()]).is_err());
    assert_eq!(fs::read_to_string(&source).unwrap(), fixture());
    let snapshot = directory.join(&created.document.sources[0].file);
    fs::write(&snapshot, fixture().replace("500", "501")).unwrap();
    assert!(
        review::open(&directory)
            .unwrap_err()
            .contains("Source snapshot changed")
    );
    fs::write(&snapshot, fixture()).unwrap();
    let mut doc = created.document.clone();
    doc.sources[0].file = "../ride.gpx".into();
    fs::write(
        directory.join("review.json"),
        serde_json::to_vec(&doc).unwrap(),
    )
    .unwrap();
    assert!(
        review::open(&directory)
            .unwrap_err()
            .contains("snapshot path")
    );
    assert!(review::create(&root.join("duplicate"), &[source.clone(), source]).is_err());
    assert!(!root.join("duplicate").exists());
}
