use ride_core::{
    full_route::Profile,
    route_repairs,
    source_review::{self, Decision},
};
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

fn grade_fixture(
    heights: &[f64],
    break_at: Option<usize>,
) -> (std::path::PathBuf, source_review::Review) {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "grade-repair-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    fs::create_dir(&root).unwrap();
    let mut xml = String::from("<gpx><trk><trkseg>");
    for (i, (metres, height)) in [0., 10., 10.2, 12., 12.2, 20., 30.]
        .iter()
        .zip(heights)
        .enumerate()
    {
        if break_at == Some(i) {
            xml.push_str("</trkseg><trkseg>");
        }
        xml.push_str(&format!(
            "<trkpt lat=\"{}\" lon=\"-118\"><ele>{height}</ele></trkpt>",
            33. + metres / 111195.
        ));
    }
    xml.push_str("</trkseg></trk></gpx>");
    let path = root.join("synthetic.gpx");
    fs::write(&path, xml).unwrap();
    let initial = source_review::create(&root.join("review"), &[path]).unwrap();
    let mut doc = initial.document.clone();
    doc.geometry_source = Some(doc.sources[0].sha256.clone());
    doc.elevation_source = doc.geometry_source.clone();
    let saved = source_review::save(&root.join("review"), &initial.revision, doc).unwrap();
    (root, saved)
}

#[test]
fn import_repairs_grade_noise_and_honors_undo_and_rejection() {
    let (root, mut review) = grade_fixture(&[10., 10., 10.4, 10.4, 10., 10.5, 11.], None);
    let directory = root.join("review");
    assert_eq!(
        review.document.repairs.len(),
        1,
        "nearby spikes form one blend"
    );
    let repair = review.document.repairs.values().next().unwrap();
    assert_eq!(repair.status, "accepted");
    assert_eq!(repair.proposal.kind, "automatic_grade_blend");
    let id = repair.proposal.id.clone();
    let fixed = Profile::from_review(&review).unwrap();
    assert!(fixed
        .points
        .windows(2)
        .all(|p| (p[1].height - p[0].height).abs() <= (p[1].distance - p[0].distance) * 0.15));
    assert_eq!(
        fs::read(root.join("synthetic.gpx")).unwrap(),
        fs::read(directory.join("source-0.gpx")).unwrap()
    );
    review = source_review::open(&directory).unwrap();
    for status in ["proposed", "rejected"] {
        review = route_repairs::decide(&directory, &review.revision, &id, status).unwrap();
        review =
            source_review::save(&directory, &review.revision, review.document.clone()).unwrap();
        assert_eq!(
            review.document.repairs[&id].status, status,
            "save must not reapply undone/rejected repairs"
        );
        let raw = Profile::from_review(&review).unwrap();
        assert_eq!(fixed.length_m(), raw.length_m());
        assert!(raw
            .points
            .windows(2)
            .any(|p| (p[1].height - p[0].height).abs() > p[1].distance - p[0].distance));
        for (a, b) in fixed.points.iter().zip(&raw.points) {
            assert_eq!(
                (a.sample, a.distance, a.lat, a.lon, a.break_before),
                (b.sample, b.distance, b.lat, b.lon, b.break_before)
            );
        }
    }
    review = route_repairs::decide(&directory, &review.revision, &id, "accepted").unwrap();
    let mut forged = review.document.clone();
    *forged
        .repairs
        .get_mut(&id)
        .unwrap()
        .proposal
        .elevations
        .values_mut()
        .next()
        .unwrap() += 0.1;
    assert!(source_review::save(&directory, &review.revision, forged).is_err());
    assert!(
        route_repairs::comparisons(&review).unwrap()[0]
            .elevation_changes
            .len()
            > 1
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn automatic_repairs_leave_real_climbs_large_errors_and_gaps_for_review() {
    for (heights, gap) in [
        ([10., 12., 12.04, 12.4, 12.44, 14., 16.], None), // continuous 20% climb
        ([10., 10., 14., 14., 14., 10., 10.], None),      // exceeds the 2 m correction budget
        ([10., 10., 10.4, 10.4, 10., 10.5, 11.], Some(3)), // unresolved boundary inside window
        ([10., 12., 12.4, 12.4, 12., 14., 16.], None),    // spike on a >15% anchor grade
    ] {
        let (root, review) = grade_fixture(&heights, gap);
        assert!(review.document.repairs.is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn repairs_are_explicit_persisted_reversible_and_source_bound() {
    let root = std::env::temp_dir().join(format!(
        "repair-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let path = root.join("synthetic.gpx");
    let xml = r#"<gpx><trk><trkseg>
      <trkpt lat="33" lon="-118"><ele>10</ele></trkpt>
      <trkpt lat="33.0001" lon="-118"><ele>10</ele></trkpt>
      </trkseg><trkseg>
      <trkpt lat="33.000107" lon="-118"><ele>11</ele></trkpt>
      <trkpt lat="33.0002" lon="-118"><ele>11</ele></trkpt>
      <trkpt lat="33.0003" lon="-118"><ele>11</ele></trkpt>
      </trkseg><trkseg>
      <trkpt lat="33.000354" lon="-118"><ele>11.8</ele></trkpt>
      <trkpt lat="33.000354" lon="-118"><ele>12</ele></trkpt>
      <trkpt lat="33.0005" lon="-118"><ele>12</ele></trkpt>
      <trkpt lat="33.0006" lon="-118"><ele>12</ele></trkpt>
      <trkpt lat="33.0006" lon="-118"><ele>12.2</ele></trkpt>
      <trkpt lat="33.0007" lon="-118"><ele>12</ele></trkpt>
    </trkseg></trk></gpx>"#;
    fs::write(&path, xml).unwrap();
    let directory = root.join("review");
    let initial = source_review::create(&directory, &[path.clone()]).unwrap();
    assert!(!fs::read_to_string(directory.join("review.json"))
        .unwrap()
        .contains("\"repairs\""));
    let mut doc = initial.document.clone();
    let hash = doc.sources[0].sha256.clone();
    doc.geometry_source = Some(hash.clone());
    doc.elevation_source = Some(hash);
    for gap in &initial.gaps {
        doc.decisions.insert(
            gap.id.clone(),
            Decision {
                classification: "stationary_pause".into(),
                note: String::new(),
            },
        );
    }
    let mut review = source_review::save(&directory, &initial.revision, doc).unwrap();
    let before = Profile::from_review(&review).unwrap();
    assert_eq!(before.unresolved_breaks, 4);
    let candidates = route_repairs::proposals(&review).unwrap();
    assert_eq!(candidates.len(), 4);
    assert_eq!(route_repairs::comparisons(&review).unwrap().len(), 4);
    let saved_revision = review.revision.clone();
    for p in &candidates {
        review = route_repairs::decide(&directory, &review.revision, &p.id, "proposed").unwrap();
    }
    assert_eq!(Profile::from_review(&review).unwrap().unresolved_breaks, 4);
    assert!(
        route_repairs::decide(&directory, &saved_revision, &candidates[0].id, "accepted").is_err()
    );
    for p in &candidates {
        review = route_repairs::decide(&directory, &review.revision, &p.id, "accepted").unwrap();
    }
    review = source_review::open(&directory).unwrap();
    let repaired = Profile::from_review(&review).unwrap();
    assert_eq!(repaired.unresolved_breaks, 0);
    assert!(repaired.length_m() > before.length_m() + 6.);
    assert!(repaired.simulation().is_ok());
    assert_eq!(fs::read_to_string(&path).unwrap(), xml);
    assert_eq!(
        fs::read_to_string(directory.join("source-0.gpx")).unwrap(),
        xml
    );
    let bytes = fs::read(directory.join("review.json")).unwrap();
    let mut invalid = review.document.clone();
    invalid
        .repairs
        .values_mut()
        .next()
        .unwrap()
        .proposal
        .source_sha256 = "wrong".into();
    assert!(source_review::save(&directory, &review.revision, invalid).is_err());
    assert_eq!(fs::read(directory.join("review.json")).unwrap(), bytes);
    let mut invalid = review.document.clone();
    invalid.decisions.clear();
    assert!(source_review::save(&directory, &review.revision, invalid).is_err());
    for p in &candidates {
        review = route_repairs::decide(&directory, &review.revision, &p.id, "rejected").unwrap();
    }
    let restored = Profile::from_review(&source_review::open(&directory).unwrap()).unwrap();
    assert_eq!(restored.length_m(), before.length_m());
    assert_eq!(restored.unresolved_breaks, 4);
    for p in &candidates {
        review = route_repairs::decide(&directory, &review.revision, &p.id, "proposed").unwrap();
    }
    assert_eq!(
        Profile::from_review(&review).unwrap().length_m(),
        before.length_m()
    );
    let mut unclassified = review.document.clone();
    unclassified.decisions.clear();
    review = source_review::save(&directory, &review.revision, unclassified).unwrap();
    assert!(route_repairs::proposals(&review)
        .unwrap()
        .iter()
        .all(|p| p.connector.is_none() && p.kind != "local_elevation_blend"));
    fs::write(
        directory.join("source-0.gpx"),
        xml.replace("<ele>10</ele>", "<ele>99</ele>"),
    )
    .unwrap();
    assert!(source_review::open(&directory).is_err());
    fs::remove_dir_all(root).unwrap();
}
