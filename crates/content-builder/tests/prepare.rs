use content_builder::prepare;
use content_builder::write_bundle;
use serde_json::{json, Value};

fn route() -> Value {
    json!({"schema_version":1,"kind":"openride.route","name":"Synthetic",
    "source_sha256":"synthetic","source_format":"GPX","source_range":[0,3],
    "elevation_provenance":"Synthetic test elevations",
    "points": [
        {"latitude":33.8,"longitude":-118.4,"elevation_m":10.,"distance_m":0.,"source_sample":0,"break_before":true},
        {"latitude":33.8001,"longitude":-118.4,"elevation_m":11.,"distance_m":11.12,"source_sample":1,"break_before":false},
        {"latitude":33.8002,"longitude":-118.4,"elevation_m":12.,"distance_m":22.24,"source_sample":2,"break_before":false},
        {"latitude":33.8003,"longitude":-118.4,"elevation_m":13.,"distance_m":33.36,"source_sample":3,"break_before":false}
    ]})
}

#[test]
fn writes_reproducible_preview_assets_without_overwriting_source_or_output() {
    let root = std::env::temp_dir().join(format!(
        "openride-content-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let source = root.join("source.orroute");
    let original = route().to_string();
    std::fs::write(&source, &original).unwrap();
    let a = root.join("a");
    let b = root.join("b");
    write_bundle(&source, &a, 0., 23.).unwrap();
    write_bundle(&source, &b, 0., 23.).unwrap();
    for file in [
        "preparation.json",
        "road-preview.obj",
        "bounds.geojson",
        "osm-query.overpass",
    ] {
        assert_eq!(
            std::fs::read(a.join(file)).unwrap(),
            std::fs::read(b.join(file)).unwrap()
        );
    }
    let obj = std::fs::read_to_string(a.join("road-preview.obj")).unwrap();
    assert_eq!(obj.lines().filter(|l| l.starts_with("v ")).count(), 8);
    assert_eq!(obj.lines().filter(|l| l.starts_with("f ")).count(), 4);
    let vertices: Vec<Vec<f64>> = obj
        .lines()
        .filter_map(|l| l.strip_prefix("v "))
        .map(|l| l.split_whitespace().map(|v| v.parse().unwrap()).collect())
        .collect();
    let face: Vec<usize> = obj
        .lines()
        .find_map(|l| l.strip_prefix("f "))
        .unwrap()
        .split_whitespace()
        .map(|v| v.parse::<usize>().unwrap() - 1)
        .collect();
    let (v0, v1, v2) = (&vertices[face[0]], &vertices[face[1]], &vertices[face[2]]);
    let normal_y = (v1[2] - v0[2]) * (v2[0] - v0[0]) - (v1[0] - v0[0]) * (v2[2] - v0[2]);
    assert!(normal_y > 0., "OBJ road front face must point upward");
    assert!(write_bundle(&source, &a, 0., 23.).is_err());
    assert_eq!(std::fs::read_to_string(source).unwrap(), original);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn bounds_preview_size_and_preserves_missing_elevations_and_breaks() {
    let mut source = route();
    for (a, b) in [
        (f64::NAN, 23.),
        (0., f64::INFINITY),
        (-1., 23.),
        (10., 10.),
        (0., 6000.),
        (0., 1.),
        (0., 40.),
    ] {
        assert!(
            prepare(&source.to_string(), a, b).is_err(),
            "accepted {a}..{b}"
        );
    }
    source["points"][1]["elevation_m"] = Value::Null;
    source["points"][2]["break_before"] = json!(true);
    let output = prepare(&source.to_string(), 0., 33.36).unwrap();
    assert!(output["points"][1]["position"][1].is_null());
    assert!(output["points"][2]["raw_grade_percent"].is_null());
    assert_eq!(output["findings"].as_array().unwrap().len(), 2);
}

#[test]
fn prepares_local_metric_profile_and_geographic_acquisition_bounds() {
    let output = prepare(&route().to_string(), 0., 23.).unwrap();
    assert_eq!(output["points"].as_array().unwrap().len(), 3);
    assert_eq!(output["trainer_ready"], false);
    assert_eq!(output["points"][0]["position"], json!([0., 0., 0.]));
    let north = output["points"][1]["position"][2].as_f64().unwrap();
    assert!((north + 11.12).abs() < 0.02); // Godot: -Z north, Y up, X east.
    assert_eq!(output["points"][1]["position"][1], 1.);
    let bounds = &output["acquisition_bounds"];
    assert!(bounds["south"].as_f64().unwrap() < 33.8);
    assert!(bounds["north"].as_f64().unwrap() > 33.8002);
    assert_eq!(output["selected_distance_m"], json!([0., 22.24]));
}
