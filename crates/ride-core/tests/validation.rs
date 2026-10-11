use ride_core::{import_xml, save_new};

#[test]
fn invalid_input_returns_errors_and_invalid_measurements_remain_missing() {
    assert!(import_xml("<gpx><trk>", "bad").is_err());
    assert!(import_xml(
        "<!DOCTYPE gpx [<!ENTITY x SYSTEM 'file:///secret'>]><gpx/>",
        "bad"
    )
    .is_err());
    assert!(import_xml("<not-an-activity/>", "bad").is_err());
    let a=import_xml(r#"<gpx><trk><trkseg><trkpt lat="NaN" lon="0"><time>bad</time><ele>Infinity</ele></trkpt><trkpt lat="0" lon="0"><time>2022-01-01T00:00:00Z</time><extensions><hr>0</hr></extensions></trkpt></trkseg></trk></gpx>"#,"bad-values").unwrap();
    assert_eq!(a.samples[0].latitude, None);
    assert_eq!(a.samples[0].elevation_m, None);
    assert_eq!(a.samples[0].time_ms, None);
    assert_eq!(a.samples[1].latitude, Some(0.0));
    assert_eq!(a.samples[1].heart_rate_bpm, Some(0.0));
    assert!(a.samples[1].break_before);
    assert!(a.diagnostics.iter().any(|d| d.code == "invalid_time"));
}

#[test]
fn export_refuses_to_overwrite_existing_data() {
    let path = std::env::temp_dir().join(format!("openride-preserve-{}.txt", std::process::id()));
    std::fs::write(&path, "original").unwrap();
    assert!(save_new(&path, "replacement").is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "original");
    std::fs::remove_file(&path).unwrap();
    save_new(&path, "new route").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "new route");
    std::fs::remove_file(path).unwrap();
}

#[test]
fn tcx_preserves_samples_without_gps_and_lap_summaries() {
    let a=import_xml(r#"<TrainingCenterDatabase><Activities><Activity><Lap StartTime="2022-01-01T00:00:00Z"><DistanceMeters>123</DistanceMeters><TotalTimeSeconds>10</TotalTimeSeconds><Track><Trackpoint><Time>2022-01-01T00:00:00Z</Time><HeartRateBpm><Value>100</Value></HeartRateBpm><Cadence>80</Cadence><Extensions><TPX><Watts>200</Watts><Speed>5</Speed></TPX></Extensions></Trackpoint><Trackpoint><Time>2022-01-01T00:00:10Z</Time></Trackpoint></Track></Lap></Activity></Activities></TrainingCenterDatabase>"#,"synthetic.tcx").unwrap();
    assert_eq!(a.samples.len(), 2);
    assert_eq!(a.samples[0].power_w, Some(200.0));
    assert_eq!(a.samples[0].latitude, None);
    assert_eq!(a.recorded_distance_m, Some(123.0));
    assert_eq!(a.timer_s, Some(10.0));
    assert_eq!(a.route_distance_m, 0.0);
}
