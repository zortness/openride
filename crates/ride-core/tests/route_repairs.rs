use ride_core::{
    full_route::Profile,
    route_repairs,
    source_review::{self, Decision},
};
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

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
