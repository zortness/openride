use ride_core::{
    full_route::Profile,
    source_review::{self, Decision},
};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

fn review_fixture() -> (PathBuf, source_review::Review) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "full-route-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    fs::create_dir(&root).unwrap();
    let source = root.join("source.gpx");
    fs::write(
        &source,
        r#"<gpx><trk><trkseg>
      <trkpt lat="33" lon="-118"><ele>10</ele><time>2022-01-01T00:00:00Z</time></trkpt>
      <trkpt lat="33.1" lon="-118"><ele>20</ele><time>2022-01-01T00:00:30Z</time></trkpt>
      <trkpt lat="33.10002" lon="-118"><ele>20</ele><time>2022-01-01T00:05:00Z</time></trkpt>
      <trkpt lat="33.2" lon="-118"><ele>5</ele><time>2022-01-01T00:05:30Z</time></trkpt>
    </trkseg></trk></gpx>"#,
    )
    .unwrap();
    let mut review = source_review::create(&root.join("review"), &[source]).unwrap();
    let hash = review.document.sources[0].sha256.clone();
    review.document.geometry_source = Some(hash.clone());
    review.document.elevation_source = Some(hash);
    review.document.decisions.insert(
        review.gaps[0].id.clone(),
        Decision {
            classification: "stationary_pause".into(),
            note: "Stopped".into(),
        },
    );
    (root, review)
}

#[test]
fn reviewed_pause_becomes_short_connector_in_full_profile_without_changing_source() {
    let (root, review) = review_fixture();
    let profile = Profile::from_review(&review).unwrap();
    assert!(profile.length_m() > 20_000.);
    assert_eq!(profile.bridged_pauses, 1);
    assert_eq!(profile.unresolved_breaks, 0);
    assert!(review.activities[0].samples[2].break_before);
    let state = profile.window(15_000.).unwrap();
    assert!((state.distance_m - 15_000.).abs() < 0.001);
    assert!(state.grade_percent.unwrap() < 0.);
    assert!(state
        .points
        .iter()
        .all(|p| p.position.iter().all(|v| v.is_finite())));
    assert!(profile.simulation().is_ok());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unresolved_gaps_stop_playback_and_clipped_windows_never_draw_across_them() {
    let (root, mut review) = review_fixture();
    review.document.decisions.clear();
    let p = Profile::from_review(&review).unwrap();
    assert_eq!(p.unresolved_breaks, 1);
    assert!(p.simulation().is_err());
    let boundary = p.points[2].distance;
    let (distance, stopped) = p.advance(boundary - 10., 20.).unwrap();
    assert!(stopped && distance < boundary);
    assert!(p.window(boundary).unwrap().at_break);
    assert!(p
        .window(boundary - 100.)
        .unwrap()
        .points
        .iter()
        .any(|p| p.break_before && p.sample == 2));
    assert!(p
        .window(100.)
        .unwrap()
        .points
        .iter()
        .all(|p| p.position[2].abs() < 501.));
    assert!(p.window(f64::NAN).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn missing_tail_is_not_a_continuous_simulation_route() {
    let (root, mut review) = review_fixture();
    review.activities[0].samples.last_mut().unwrap().elevation_m = None;
    let p = Profile::from_review(&review).unwrap();
    assert!(p.simulation().is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unsafe_pause_and_remote_break_cannot_create_connector_or_global_mesh_coordinates() {
    let (root, mut review) = review_fixture();
    review.activities[0].samples[2].elevation_m = Some(30.);
    let p = Profile::from_review(&review).unwrap();
    assert_eq!(p.bridged_pauses, 0);
    assert_eq!(p.unresolved_breaks, 1);
    review.activities[0].samples[2].latitude = Some(40.);
    review.activities[0].samples[3].latitude = Some(40.1);
    let p = Profile::from_review(&review).unwrap();
    let window = p.window(p.points[2].distance).unwrap();
    assert!(window
        .points
        .iter()
        .all(|p| p.position[0].abs() < 2000. && p.position[2].abs() < 2000.));
    fs::remove_dir_all(root).unwrap();
}
