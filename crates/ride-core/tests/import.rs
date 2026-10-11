use ride_core::import_xml;

#[test]
fn imports_gpx_measurements_and_preserves_absent_values() {
    let activity = import_xml(r#"<gpx version="1.1" xmlns="http://www.topografix.com/GPX/1/1" xmlns:t="http://www.garmin.com/xmlschemas/TrackPointExtension/v1"><trk><name>Example</name><trkseg><trkpt lat="33" lon="-118"><time>2022-03-17T17:03:40Z</time><ele>12</ele><extensions><t:TrackPointExtension><t:hr>120</t:hr><t:cad>0</t:cad></t:TrackPointExtension></extensions></trkpt><trkpt lat="33.001" lon="-118"><time>2022-03-17T17:03:45Z</time></trkpt></trkseg></trk></gpx>"#, "example.gpx").unwrap();
    assert_eq!(activity.samples.len(), 2);
    assert_eq!(activity.samples[0].heart_rate_bpm, Some(120.0));
    assert_eq!(activity.samples[0].cadence_rpm, Some(0.0));
    assert_eq!(activity.samples[1].cadence_rpm, None);
    assert_eq!(activity.samples[1].elevation_m, None);
    assert!(activity.route_distance_m > 100.0);
    assert_eq!(activity.name, "Example");
}

#[test]
fn subsection_round_trip_preserves_gaps_without_exporting_biometrics() {
    let xml = r#"<gpx><trk><name>A &amp; B</name><trkseg><trkpt lat="33" lon="-118"><time>2022-01-01T00:00:00Z</time><ele>10</ele></trkpt><trkpt lat="33.001" lon="-118"><time>2022-01-01T00:00:05Z</time><ele>20</ele></trkpt><trkpt lat="33.002" lon="-118"><time>2022-01-01T00:05:00Z</time><ele>30</ele></trkpt><trkpt lat="33.003" lon="-118"><time>2022-01-01T00:05:05Z</time><ele>40</ele></trkpt></trkseg></trk></gpx>"#;
    let activity = import_xml(xml, "sample.gpx").unwrap();
    let route = ride_core::extract_route(&activity, 1, 3).unwrap();
    assert_eq!(route.points.len(), 3);
    assert_eq!(route.points[0].distance_m, 0.0);
    assert!(route.points[1].break_before);
    let json = serde_json::to_string(&route).unwrap();
    assert!(!json.contains("heart_rate"));
    let reopened = ride_core::read_route(&json).unwrap();
    assert_eq!(reopened.source_sha256, activity.source_sha256);
    assert_eq!(reopened.points[2].source_sample, 3);
    let gpx = ride_core::route_gpx(&route);
    let reimport = import_xml(&gpx, "route.gpx").unwrap();
    assert_eq!(reimport.samples.len(), 3);
    assert_eq!(reimport.name, "A & B");
    assert!((reimport.route_distance_m - route.points.last().unwrap().distance_m).abs() < 0.1);
    assert!(ride_core::extract_route(&activity, 3, 1).is_err());
    assert!(
        ride_core::read_route(&json.replace("\"schema_version\":1", "\"schema_version\":99"))
            .is_err()
    );
}

#[test]
#[ignore = "Requires the private Garmin sample directory"]
fn supplied_garmin_corpus_preserves_all_sources_and_groups_pairs() {
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../import");
    let mut paths: Vec<String> = std::fs::read_dir(folder)
        .unwrap()
        .map(|f| f.unwrap().path().to_string_lossy().into_owned())
        .filter(|p| p.ends_with(".gpx") || p.ends_with(".tcx"))
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 31);
    let lib = ride_core::Library::import_paths(&paths);
    assert!(lib.errors.is_empty(), "{:?}", lib.errors);
    assert_eq!(lib.groups.len(), 16);
    assert_eq!(
        lib.groups.iter().map(|g| g.sources.len()).sum::<usize>(),
        31
    );
    let rpv = lib
        .groups
        .iter()
        .flat_map(|g| &g.sources)
        .find(|a| a.source_name == "2022-03-17_kurt_rpv.tcx")
        .unwrap();
    assert_eq!(rpv.samples.len(), 1014);
    assert_eq!(rpv.laps.len(), 2);
    assert!((rpv.recorded_distance_m.unwrap() / 1000.0 - 12.54).abs() < 0.01);
    assert_eq!(
        rpv.diagnostics
            .iter()
            .filter(|d| d.code == "nonincreasing_time")
            .count(),
        1
    );
    println!("Verified 31 private source files -> 16 ride groups; RPV: {} samples, {:.3} recorded km, {:.3} unbridged GPS km",rpv.samples.len(),rpv.recorded_distance_m.unwrap()/1000.0,rpv.route_distance_m/1000.0);
}

#[test]
fn groups_matching_tracks_despite_filenames_and_keeps_source_variants() {
    let xml = r#"<gpx><trk><trkseg><trkpt lat="33" lon="-118"><time>2022-01-01T00:00:00Z</time></trkpt><trkpt lat="33.001" lon="-118"><time>2022-01-01T00:00:05Z</time></trkpt><trkpt lat="33.002" lon="-118"><time>2022-01-01T00:00:10Z</time></trkpt></trkseg></trk></gpx>"#;
    let mut lib = ride_core::Library::default();
    lib.insert(import_xml(xml, "first.gpx").unwrap());
    lib.insert(
        import_xml(
            &xml.replace("<trk>", "<trk><name>Different export</name>"),
            "renamed.gpx",
        )
        .unwrap(),
    );
    assert_eq!(lib.groups.len(), 1);
    assert_eq!(lib.groups[0].sources.len(), 2);
    lib.insert(import_xml(xml, "copy.gpx").unwrap());
    assert_eq!(lib.groups[0].sources.len(), 2);
    lib.insert(import_xml(&xml.replace("2022-01-01", "2022-01-02"), "first.gpx").unwrap());
    assert_eq!(lib.groups.len(), 2);
}
