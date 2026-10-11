//! Offline authoring preparation. Outputs are previews, never trainer profiles.
pub mod terrain;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};

const EARTH_M: f64 = 6_371_000.;

pub fn write_bundle(
    source: &Path,
    destination: &Path,
    start: f64,
    end: f64,
) -> Result<Value, String> {
    let mut bytes = Vec::new();
    fs::File::open(source)
        .map_err(|e| e.to_string())?
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 64 * 1024 * 1024 {
        return Err("Source exceeds 64 MiB".into());
    }
    let mut report = prepare(
        std::str::from_utf8(&bytes).map_err(|e| e.to_string())?,
        start,
        end,
    )?;
    let mut vertices: Vec<[f64; 3]> = Vec::new();
    let mut indices = Vec::new();
    for pair in report["points"].as_array().unwrap().windows(2) {
        if pair[1]["break_before"] == true {
            continue;
        }
        let position = |p: &Value| -> Option<[f64; 3]> {
            Some([
                p["position"][0].as_f64()?,
                p["position"][1].as_f64()?,
                p["position"][2].as_f64()?,
            ])
        };
        let (Some(a), Some(b)) = (position(&pair[0]), position(&pair[1])) else {
            continue;
        };
        let dx = b[0] - a[0];
        let dz = b[2] - a[2];
        let length = dx.hypot(dz);
        if !(1. ..=100.).contains(&length) {
            continue;
        }
        let side = [-3. * dz / length, 0., 3. * dx / length];
        let base = vertices.len();
        for (p, sign) in [(a, 1.), (a, -1.), (b, 1.), (b, -1.)] {
            vertices.push([p[0] + sign * side[0], p[1], p[2] + sign * side[2]]);
        }
        // Godot clockwise front faces; OBJ export reverses these for CCW readers.
        indices.extend([base, base + 1, base + 2, base + 1, base + 3, base + 2]);
    }
    // Missing elevations still permit geographic acquisition; findings block riding until reviewed.
    report["preview_mesh"] = json!({"vertices":vertices,"indices":indices,"width_m":6.,
        "meaning":"Independent road ribbons, not surveyed road widths or junction geometry"});
    let mut obj = String::from(
        "# OpenRide unvalidated road-profile preview; metres; Y up\no road_profile_preview\n",
    );
    for v in &vertices {
        obj.push_str(&format!("v {} {} {}\n", v[0], v[1], v[2]));
    }
    for face in indices.chunks_exact(3) {
        obj.push_str(&format!(
            "f {} {} {}\n",
            face[0] + 1,
            face[2] + 1,
            face[1] + 1
        ));
    }
    let bounds = &report["acquisition_bounds"];
    let s = bounds["south"].as_f64().unwrap();
    let w = bounds["west"].as_f64().unwrap();
    let n = bounds["north"].as_f64().unwrap();
    let e = bounds["east"].as_f64().unwrap();
    let geo = json!({"type":"FeatureCollection","features":[{"type":"Feature","properties":{"purpose":"source acquisition, 500 m buffer"},"geometry":{"type":"Polygon","coordinates":[[[w,s],[e,s],[e,n],[w,n],[w,s]]]}}]});
    let query = format!("[out:xml][timeout:60];\n(way[highway]({s},{w},{n},{e});way[building]({s},{w},{n},{e});relation[building]({s},{w},{n},{e});way[landuse]({s},{w},{n},{e});way[natural]({s},{w},{n},{e}););\n(._;>>;);\nout meta;\n");
    let assets = [
        ("road-preview.obj", obj),
        (
            "bounds.geojson",
            serde_json::to_string_pretty(&geo).unwrap(),
        ),
        ("osm-query.overpass", query),
    ];
    report["files"] = json!(assets.iter().map(|(name,text)|json!({"path":name,"sha256":format!("{:x}",Sha256::digest(text.as_bytes()))})).collect::<Vec<_>>());
    // All validation and generation precedes creating the exclusive destination.
    fs::create_dir(destination).map_err(|e| format!("Create new output directory: {e}"))?;
    for (name, text) in assets
        .iter()
        .map(|(n, t)| (*n, t.clone()))
        .chain(std::iter::once((
            "preparation.json",
            serde_json::to_string_pretty(&report).unwrap(),
        )))
    {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination.join(name))
            .map_err(|e| e.to_string())?;
        file.write_all(text.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|e| e.to_string())?;
    }
    Ok(report)
}

pub fn prepare(json_text: &str, start: f64, end: f64) -> Result<Value, String> {
    let route = ride_core::read_route(json_text)?;
    if !start.is_finite()
        || !end.is_finite()
        || start < 0.
        || end <= start
        || end - start > 5000.
        || end > route.points.last().unwrap().distance_m
    {
        return Err("Choose a finite interval within the route, up to 5,000 metres".into());
    }
    let selected: Vec<_> = route
        .points
        .iter()
        .filter(|p| p.distance_m >= start && p.distance_m <= end)
        .collect();
    let first = selected.first().ok_or("No points in selected interval")?;
    if selected.len() < 2 {
        return Err("Selected interval needs at least two points".into());
    }
    if selected.iter().any(|p| {
        p.latitude.abs() > 80.
            || (p.longitude - first.longitude).abs() > 1.
            || ride_core::haversine(first.latitude, first.longitude, p.latitude, p.longitude)
                > 6000.
    }) {
        return Err("Preview requires a local section below 80 degrees latitude without antimeridian crossings".into());
    }
    let last = selected.last().unwrap();
    let origin = [
        first.latitude,
        first.longitude,
        first.elevation_m.unwrap_or(0.),
    ];
    let mut south = first.latitude;
    let mut north = south;
    let mut west = first.longitude;
    let mut east = west;
    let mut points = Vec::new();
    let mut findings = Vec::new();
    for (i, p) in selected.iter().enumerate() {
        south = south.min(p.latitude);
        north = north.max(p.latitude);
        west = west.min(p.longitude);
        east = east.max(p.longitude);
        let position = [
            Some(EARTH_M * (p.longitude - origin[1]).to_radians() * origin[0].to_radians().cos()),
            p.elevation_m.map(|h| h - origin[2]),
            Some(-EARTH_M * (p.latitude - origin[0]).to_radians()),
        ];
        let mut grade = None;
        if i > 0 && !p.break_before {
            let prev = selected[i - 1];
            let distance =
                ride_core::haversine(prev.latitude, prev.longitude, p.latitude, p.longitude);
            if let (Some(a), Some(b)) = (prev.elevation_m, p.elevation_m) {
                if distance >= 1. {
                    grade = Some(100. * (b - a) / distance);
                }
            }
            if distance > 100. {
                findings.push(json!({"source_sample":p.source_sample,"issue":"position_gap_over_100m","value_m":distance}));
            }
            if grade.is_some_and(|g: f64| g.abs() > 15.) {
                findings.push(json!({"source_sample":p.source_sample,"issue":"raw_grade_over_15_percent","grade_percent":grade}));
            }
        }
        if p.elevation_m.is_none() {
            findings.push(json!({"source_sample":p.source_sample,"issue":"missing_elevation"}));
        }
        if i > 0 && p.break_before {
            findings.push(json!({"source_sample":p.source_sample,"issue":"segment_break"}));
        }
        points.push(
            json!({"source_sample":p.source_sample,"route_distance_m":p.distance_m,
            "distance_m":p.distance_m-first.distance_m,"position":position,
            "latitude":p.latitude,"longitude":p.longitude,"elevation_m":p.elevation_m,
            "break_before":i==0 || p.break_before,"raw_grade_percent":grade}),
        );
    }
    let pad_lat = (500. / EARTH_M).to_degrees();
    let pad_lon = pad_lat / origin[0].to_radians().cos();
    Ok(
        json!({"schema_version":1,"kind":"openride.scenery-preparation","name":route.name,
        "trainer_ready":false,"source_route_sha256":format!("{:x}",Sha256::digest(json_text.as_bytes())),
        "source_activity_sha256":route.source_sha256,"elevation_provenance":route.elevation_provenance,
        "requested_distance_m":[start,end],"selected_distance_m":[first.distance_m,last.distance_m],
        "origin_lat_lon_elevation":origin,"coordinates":"metres: X east, Y up, -Z north; local equirectangular preview",
        "acquisition_bounds":{"south":south-pad_lat,"west":west-pad_lon,"north":north+pad_lat,"east":east+pad_lon},
        "acquisition_buffer_m":500,"points":points,"findings":findings,
        "warnings":["Activity elevation is unvalidated. Preview only; no trainer control.",
            "No terrain or OSM sources included. Geographic acquisition bounds use a 500 m buffer.",
            "Raw segment grades are diagnostic, not smoothed or corrected road grades."]}),
    )
}
