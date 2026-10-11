use std::process::Command;

#[test]
fn exports_timing_with_missing_speed_and_refuses_overwrite() {
    let root = std::env::temp_dir().join(format!(
        "openride-evidence-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let source = root.join("activity.gpx");
    let output = root.join("evidence.json");
    std::fs::write(&source,r#"<gpx><trk><trkseg><trkpt lat="34" lon="-118"><time>2012-02-12T20:00:00Z</time></trkpt><trkpt lat="34.00001" lon="-118"><time>2012-02-12T20:01:00Z</time></trkpt></trkseg></trk></gpx>"#).unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_content-builder"))
            .arg("--activity-evidence")
            .arg(&source)
            .arg(&output)
            .output()
            .unwrap()
    };
    assert!(run().status.success());
    let raw = std::fs::read(&output).unwrap();
    let evidence: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(evidence["samples"][1]["source_sample"], 1);
    assert!(evidence["samples"][0]["speed_mps"].is_null());
    assert_eq!(
        evidence["samples"][1]["time_ms"].as_i64().unwrap()
            - evidence["samples"][0]["time_ms"].as_i64().unwrap(),
        60000
    );
    assert_eq!(
        evidence["source_activity_sha256"],
        ride_core::import_file(&source).unwrap().source_sha256
    );
    assert!(!run().status.success());
    assert_eq!(std::fs::read(&output).unwrap(), raw);
    std::fs::remove_file(output).unwrap();
    std::fs::remove_file(source).unwrap();
    std::fs::remove_dir(root).unwrap();
}
