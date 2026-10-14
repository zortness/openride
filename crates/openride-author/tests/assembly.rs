use openride_author::{assembly, course, create, import, maps, select_sources};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, io::Cursor, path::PathBuf};
struct Work {
    root: PathBuf,
    keep: bool,
}
impl Drop for Work {
    fn drop(&mut self) {
        if !self.keep {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
fn fixture(count: usize) -> Work {
    let chosen = std::env::var_os("OPENRIDE_SYNTHETIC_PROJECT");
    let keep = chosen.is_some();
    let root = chosen.map(PathBuf::from).unwrap_or_else(|| {
        std::env::temp_dir().join(format!("or-assembly-{}-{count}", std::process::id()))
    });
    fs::create_dir(&root).unwrap();
    Work { root, keep }
}
fn fetch(url: &str, _body: Option<&str>, count: usize) -> openride_author::Result<Vec<u8>> {
    if url.contains("tile.png") {
        let mut bytes = vec![];
        let pixels: Vec<u8> = (0..1024 * 1024)
            .flat_map(|i| [(i % 1024 / 4) as u8, (i / 1024 / 4) as u8, 80, 255])
            .collect();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 1024, 1024);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&pixels)
                .unwrap();
        }
        return Ok(bytes);
    }
    if url.contains("tile.tif") {
        let mut raw = Cursor::new(Vec::new());
        let u = reqwest::Url::parse(url).unwrap();
        let bbox = u.query_pairs().find(|p| p.0 == "bbox").unwrap().1;
        let b: Vec<f64> = bbox.split(',').map(|v| v.parse().unwrap()).collect();
        let heights: Vec<f32> = (0..256 * 256)
            .map(|i| {
                let lat = b[3] - (b[3] - b[1]) * (i / 256) as f64 / 256.;
                let lon = b[0] + (b[2] - b[0]) * (i % 256) as f64 / 256.;
                (10. + (lat - 33.) * 200. + (lon + 118.) * 100.) as f32
            })
            .collect();
        tiff::encoder::TiffEncoder::new(&mut raw)
            .unwrap()
            .write_image::<tiff::encoder::colortype::Gray32Float>(256, 256, &heights)
            .unwrap();
        return Ok(raw.into_inner());
    }
    if url.contains("exportImage") {
        let u = reqwest::Url::parse(url).unwrap();
        let bbox = u.query_pairs().find(|p| p.0 == "bbox").unwrap().1;
        let b: Vec<f64> = bbox.split(',').map(|v| v.parse().unwrap()).collect();
        if url.contains("USGSNAIPPlus") {
            return Ok(serde_json::to_vec(&json!({"width":1024,"height":1024,"extent":{"xmin":b[0],"ymin":b[1],"xmax":b[2],"ymax":b[3],"spatialReference":{"wkid":4326}},"href":"https://imagery.nationalmap.gov/tile.png"})).unwrap());
        }
        return Ok(serde_json::to_vec(&json!({"width":256,"height":256,"extent":{"xmin":b[0],"ymin":b[1],"xmax":b[2],"ymax":b[3],"spatialReference":{"wkid":4326}},"href":format!("https://elevation.nationalmap.gov/tile.tif?bbox={},{},{},{}",b[0],b[1],b[2],b[3])})).unwrap());
    }
    if url.contains("overpass") {
        let mut xml = "<osm version=\"0.6\">".to_string();
        for i in 0..count {
            xml += &format!(
                "<node id=\"{}\" version=\"1\" lat=\"{}\" lon=\"-118\"/>",
                i + 1,
                33. + i as f64 * 0.001
            );
        }
        xml += "<way id=\"100\" version=\"1\">";
        for i in 1..=count {
            xml += &format!("<nd ref=\"{i}\"/>");
        }
        xml += "<tag k=\"highway\" v=\"cycleway\"/></way></osm>";
        return Ok(xml.into_bytes());
    }
    Ok(b"{}".to_vec())
}
#[test]
fn builds_continuous_native_package_reuses_assets_and_preserves_last_build_on_failure() {
    exercise(21);
}
#[test]
fn road_first_builds_without_recorded_elevations_and_archives_choices() {
    let work = fixture(22);
    let root = work.root.join("No recorded heights");
    let source = work.root.join("source.gpx");
    let gpx = format!(
        "<gpx><trk><trkseg>{}</trkseg></trk></gpx>",
        (0..21)
            .map(|i| format!("<trkpt lat=\"{}\" lon=\"-118\"/>", 33. + i as f64 * 0.001))
            .collect::<String>()
    );
    fs::write(&source, &gpx).unwrap();
    let s = create(&root, "Road elevations", None).unwrap();
    let s = import(&root, &s.revision, &[source.clone()], None).unwrap();
    let id = &s.review.as_ref().unwrap().document.sources[0].sha256;
    let s = select_sources(&root, &s.revision, id, id, None).unwrap();
    let s = maps::acquire_with(&root, &s.revision, "terrain", 1024, None, |u, b| {
        fetch(u, b, 21)
    })
    .unwrap();
    let s = maps::acquire_with(&root, &s.revision, "streets", 1, None, |u, b| {
        fetch(u, b, 21)
    })
    .unwrap();
    let s = openride_author::road_course::build(&root, &s.revision, Some((10, Some(100))), None)
        .unwrap();
    assert!(s
        .course
        .as_ref()
        .unwrap()
        .sections
        .iter()
        .all(|s| s.recorded_elevation_m.is_none()));
    let s = assembly::build(&root, &s.revision, false, None).unwrap();
    assert!(s.build.as_ref().unwrap().current);
    let saved = fs::read(root.join("roads.json")).unwrap();
    assert!(openride_author::road_course::build(&root, "stale", None, None).is_err());
    assert_eq!(fs::read(root.join("roads.json")).unwrap(), saved);
    let s = openride_author::road_course::reset(&root, &s.revision, None).unwrap();
    assert!(s
        .course
        .unwrap()
        .sections
        .iter()
        .all(|s| s.status == "matched"));
    assert!(fs::read_dir(root.join("jobs")).unwrap().any(|p| p
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with("roads-")));
    assert_eq!(fs::read_to_string(source).unwrap(), gpx);
}

#[test]
fn nearby_road_choice_is_atomic_persists_and_undoes_as_a_group() {
    use openride_author::road_course;
    let work = fixture(23);
    let root = work.root.join("Grouped choices");
    let source = work.root.join("source.gpx");
    let gpx = format!(
        "<gpx><trk><trkseg>{}</trkseg></trk></gpx>",
        (0..21)
            .map(|i| format!("<trkpt lat=\"{}\" lon=\"-118\"/>", 33. + i as f64 * 0.001))
            .collect::<String>()
    );
    fs::write(&source, &gpx).unwrap();
    let s = create(&root, "Grouped choices", None).unwrap();
    let s = import(&root, &s.revision, &[source.clone()], None).unwrap();
    let id = &s.review.as_ref().unwrap().document.sources[0].sha256;
    let s = select_sources(&root, &s.revision, id, id, None).unwrap();
    let s=maps::acquire_with(&root,&s.revision,"streets",1,None,|_,_|{
        let mut xml="<osm version=\"0.6\">".to_string();
        for (id,lon) in [(1,-118.00005),(3,-117.99995)] {
            xml+=&format!("<node id=\"{id}\" version=\"1\" lat=\"33\" lon=\"{lon}\"/><node id=\"{}\" version=\"1\" lat=\"33.021\" lon=\"{lon}\"/><way id=\"{}\" version=\"1\"><nd ref=\"{id}\"/><nd ref=\"{}\"/><tag k=\"highway\" v=\"residential\"/></way>",id+1,id+100,id+1);
        }
        xml+="</osm>";Ok(xml.into_bytes())
    }).unwrap();
    let s = road_course::build(&root, &s.revision, None, None).unwrap();
    let original = fs::read(root.join("roads.json")).unwrap();
    assert!(road_course::choose_nearby(&root, "stale", 10, 101, None).is_err());
    assert_eq!(original, fs::read(root.join("roads.json")).unwrap());
    let s = road_course::choose_nearby(&root, &s.revision, 10, 101, None).unwrap();
    assert_eq!(s.course.as_ref().unwrap().unresolved, 0);
    let s = openride_author::open(&root).unwrap();
    assert_eq!(
        s.course
            .as_ref()
            .unwrap()
            .roads
            .as_ref()
            .unwrap()
            .choice_groups[&10],
        21
    );
    // Editing one member later must not be overwritten when the rest is undone.
    let s = road_course::build(&root, &s.revision, Some((0, Some(101))), None).unwrap();
    let s = road_course::build(&root, &s.revision, Some((10, None)), None).unwrap();
    assert!(s
        .course
        .as_ref()
        .unwrap()
        .roads
        .as_ref()
        .unwrap()
        .choice_groups
        .is_empty());
    let state: Value = serde_json::from_slice(&fs::read(root.join("roads.json")).unwrap()).unwrap();
    assert_eq!(state["choices"], json!({"0":101}));
    let s = road_course::build(&root, &s.revision, Some((0, None)), None).unwrap();
    assert!(s.course.unwrap().unresolved > 0);
    assert_eq!(fs::read_to_string(source).unwrap(), gpx);
}

#[test]
fn recorded_connection_preserves_choices_builds_on_terrain_and_undoes() {
    use openride_author::road_course;
    let work = fixture(24);
    let root = work.root.join("Recorded connector");
    let source = work.root.join("source.gpx");
    let gpx = format!(
        "<gpx><trk><trkseg>{}</trkseg></trk></gpx>",
        [33., 33.0005, 33.001, 33.0012, 33.0017, 33.0022]
            .iter()
            .map(|lat| format!("<trkpt lat=\"{lat}\" lon=\"-118\"/>"))
            .collect::<String>()
    );
    fs::write(&source, &gpx).unwrap();
    let s = create(&root, "Recorded connector", None).unwrap();
    let s = import(&root, &s.revision, &[source.clone()], None).unwrap();
    let id = &s.review.as_ref().unwrap().document.sources[0].sha256;
    let s = select_sources(&root, &s.revision, id, id, None).unwrap();
    let s = maps::acquire_with(&root, &s.revision, "terrain", 1024, None, |u, b| {
        fetch(u, b, 21)
    })
    .unwrap();
    let s=maps::acquire_with(&root,&s.revision,"streets",1024,None,|_,_|Ok(br#"<osm version="0.6"><node id="1" version="1" lat="33" lon="-118"/><node id="2" version="1" lat="33.001" lon="-118"/><node id="3" version="1" lat="33.0012" lon="-118"/><node id="4" version="1" lat="33.003" lon="-118"/><way id="100" version="1"><nd ref="1"/><nd ref="2"/><tag k="highway" v="residential"/></way><way id="200" version="1"><nd ref="3"/><nd ref="4"/><tag k="highway" v="residential"/></way></osm>"#.to_vec())).unwrap();
    let s = road_course::build(&root, &s.revision, Some((0, Some(100))), None).unwrap();
    let course = s.course.as_ref().unwrap();
    assert_eq!(course.unresolved, 1);
    let end = course.roads.as_ref().unwrap().issues[0].first_sample;
    assert!(course
        .roads
        .as_ref()
        .unwrap()
        .connection_previews
        .contains_key(&end));
    let saved = fs::read(root.join("roads.json")).unwrap();
    assert!(road_course::connect(&root, "stale", end, false, None).is_err());
    assert!(road_course::connect(&root, &s.revision, 0, false, None).is_err());
    assert_eq!(saved, fs::read(root.join("roads.json")).unwrap());
    let s = road_course::connect(&root, &s.revision, end, false, None).unwrap();
    assert_eq!(s.course.as_ref().unwrap().unresolved, 0);
    assert!(s
        .course
        .as_ref()
        .unwrap()
        .sections
        .iter()
        .any(|s| s.status == "manual_connection" && s.way.is_none() && !s.break_before));
    for pair in s.course.as_ref().unwrap().sections.windows(2) {
        assert_eq!(pair[0].geometry.last(), pair[1].geometry.first());
    }
    let s = openride_author::open(&root).unwrap();
    let s = road_course::build(&root, &s.revision, None, None).unwrap();
    assert_eq!(s.course.as_ref().unwrap().unresolved, 0);
    let s = assembly::build(&root, &s.revision, false, None).unwrap();
    assert!(s.build.as_ref().unwrap().current);
    let s = road_course::connect(&root, &s.revision, end, true, None).unwrap();
    assert_eq!(s.course.unwrap().unresolved, 1);
    let state: Value = serde_json::from_slice(&fs::read(root.join("roads.json")).unwrap()).unwrap();
    assert_eq!(state["choices"], json!({"0":100}));
    assert_eq!(fs::read_to_string(source).unwrap(), gpx);
}
#[test]
#[ignore = "150 km synthetic assembly endurance check; run explicitly in release mode"]
fn builds_long_native_corridor() {
    exercise(1351);
}
fn exercise(count: usize) {
    let work = fixture(count);
    let root = work.root.join("Ride Project");
    let mut gpx = "<gpx><trk><trkseg>".to_string();
    for i in 0..count {
        gpx+=&format!("<trkpt lat=\"{}\" lon=\"-118\"><ele>10</ele><time>2022-01-01T{:02}:{:02}:{:02}Z</time></trkpt>",33.+i as f64*0.001,i/360,(i/6)%60,(i%6)*10);
    }
    gpx += "</trkseg></trk></gpx>";
    let source = work.root.join("source.gpx");
    fs::write(&source, gpx).unwrap();
    let created = create(&root, "Native synthetic corridor", None).unwrap();
    let imported = import(&root, &created.revision, &[source], None).unwrap();
    let id = &imported.review.as_ref().unwrap().document.sources[0].sha256;
    let chosen = select_sources(&root, &imported.revision, id, id, None).unwrap();
    let fetch = |url: &str, body: Option<&str>| fetch(url, body, count);
    let terrain =
        maps::acquire_with(&root, &chosen.revision, "terrain", 1024, None, fetch).unwrap();
    let streets = maps::acquire_with(
        &root,
        &terrain.revision,
        "streets",
        if count == 21 { 1024 } else { 1 },
        None,
        fetch,
    )
    .unwrap();
    let streets = maps::acquire_with(
        &root,
        &streets.revision,
        "imagery",
        if count > 100 { 2 } else { 1024 },
        None,
        fetch,
    )
    .unwrap();
    let matched = course::build(&root, &streets.revision, None).unwrap();
    assert_eq!(matched.course.as_ref().unwrap().unresolved, 0);
    let mut matched = matched;
    let ring = |south: f64, west: f64, north: f64, east: f64| {
        vec![[south, west], [south, east], [north, east], [north, west]]
    };
    for (id, kind, outer, holes) in [
        (
            "user-building",
            "building",
            ring(33.003, -117.9993, 33.0032, -117.9991),
            vec![],
        ),
        (
            "user-trees",
            "vegetation",
            ring(33.0001, -118.0014, 33.0199, -117.9986),
            vec![],
        ),
        (
            "user-lake",
            "water",
            ring(33.0095, -118.0013, 33.0105, -118.0006),
            vec![ring(33.0098, -118.0011, 33.0102, -118.0008)],
        ),
    ] {
        let feature = openride_author::scenery::Feature {
            id: id.into(),
            kind: kind.into(),
            outer,
            holes,
            height_m: Some(7.),
            elevation_m: if kind == "water" { Some(10.) } else { None },
            density: 0.75,
            trees: true,
            hidden: false,
            provenance: "Synthetic scenery".into(),
        };
        matched = openride_author::scenery::edit(
            &root,
            &matched.revision,
            "save",
            id,
            Some(feature),
            None,
        )
        .unwrap();
    }
    let built = assembly::build(&root, &matched.revision, false, None).unwrap();
    let record = built.build.as_ref().unwrap();
    assert!(record.chunks > 8);
    assert!(record.length_m > 2000.);
    assert!((record.length_m - (count - 1) as f64 * 111.1949266).abs() < 0.1);
    let directory = root.join(&record.path);
    openride_author::package::validate(&directory).unwrap();
    let manifest: Value =
        serde_json::from_slice(&fs::read(directory.join("manifest.json")).unwrap()).unwrap();
    let mut vertices: BTreeMap<(i64, i64), (f64, Vec<f64>)> = BTreeMap::new();
    let mut shared = 0;
    let mut images = BTreeMap::new();
    let mut scenery_counts = [0; 3];
    for entry in manifest["chunks"].as_array().unwrap() {
        if let Some(image) = entry.get("imagery") {
            let raw = fs::read(directory.join(image["path"].as_str().unwrap())).unwrap();
            let mut reader = png::Decoder::new(Cursor::new(raw)).read_info().unwrap();
            let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
            reader.next_frame(&mut pixels).unwrap();
            images.insert(
                (entry["x"].as_i64().unwrap(), entry["z"].as_i64().unwrap()),
                pixels,
            );
        }
        let payload: Value = serde_json::from_slice(
            &fs::read(directory.join(entry["path"].as_str().unwrap())).unwrap(),
        )
        .unwrap();
        let terrain = &payload["terrain"];
        scenery_counts[0] += payload["buildings"]["vertices"].as_array().unwrap().len();
        scenery_counts[1] += payload["water"]["vertices"].as_array().unwrap().len();
        scenery_counts[2] += payload["vegetation"].as_array().unwrap().len();
        for (v, n) in terrain["vertices"]
            .as_array()
            .unwrap()
            .iter()
            .zip(terrain["normals"].as_array().unwrap())
        {
            let key = (v[0].as_f64().unwrap() as i64, v[2].as_f64().unwrap() as i64);
            let value = (
                v[1].as_f64().unwrap(),
                n.as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_f64().unwrap())
                    .collect(),
            );
            if let Some(old) = vertices.insert(key, value.clone()) {
                assert_eq!(old, value);
                shared += 1;
            }
        }
    }
    assert!(shared > 100, "must compare actual shared chunk edges");
    assert!(scenery_counts.iter().all(|v| *v > 0));
    assert!(!images.is_empty());
    assert!(
        manifest["tiled_imagery"]["peak_decoded_source_tiles"]
            .as_u64()
            .unwrap()
            <= 8
    );
    let mut image_seams = 0;
    for (&(x, z), image) in &images {
        if let Some(right) = images.get(&(x + 1, z)) {
            for row in 0..130 {
                assert_eq!(
                    &image[(row * 130 + 128) * 4..(row * 130 + 130) * 4],
                    &right[row * 130 * 4..(row * 130 + 2) * 4]
                );
            }
            image_seams += 1;
        }
        if let Some(below) = images.get(&(x, z + 1)) {
            assert_eq!(&image[128 * 130 * 4..], &below[..2 * 130 * 4]);
            image_seams += 1;
        }
    }
    assert!(image_seams > 0);
    let zip = work.root.join("imagery.zip");
    openride_author::package::pack(&directory, &zip).unwrap();
    openride_author::package::install(&zip, &work.root.join("cache")).unwrap();
    let repeated = assembly::build(&root, &built.revision, false, None).unwrap();
    assert_eq!(repeated.build.as_ref().unwrap().key, record.key);
    assert_eq!(
        repeated.build.as_ref().unwrap().reused_chunks,
        record.chunks
    );
    let pointer = fs::read(root.join("build/course/current.json")).unwrap();
    let cancel = work.root.join("cancel");
    fs::write(&cancel, b"").unwrap();
    assert_eq!(
        assembly::build(&root, &built.revision, false, Some(&cancel))
            .unwrap_err()
            .code,
        "cancelled"
    );
    assert_eq!(
        fs::read(root.join("build/course/current.json")).unwrap(),
        pointer
    );
    // A corrupt acquisition cannot replace a previously usable compiled package.
    let plan = built.maps.as_ref().unwrap();
    let tile = root
        .join("maps")
        .join(&plan.key)
        .join(&plan.tiles[0].id)
        .join("terrain.tif");
    let original = fs::read(&tile).unwrap();
    fs::write(&tile, b"bad").unwrap();
    assert!(assembly::build(&root, &built.revision, false, None).is_err());
    assert_eq!(
        fs::read(root.join("build/course/current.json")).unwrap(),
        pointer
    );
    fs::write(tile, original).unwrap();
    // A local course edit changes nearby geometry while reusing distant assets;
    // the previous immutable package remains available for comparison/recovery.
    let edited = openride_author::corrections::decide(
        &root,
        &built.revision,
        openride_author::corrections::Edit {
            source_sample: count / 2,
            mode: "point".into(),
            position: Some([33. + (count / 2) as f64 * 0.001, -117.99997]),
            way: None,
        },
        None,
    )
    .unwrap();
    assert_eq!(edited.course.as_ref().unwrap().unresolved, 1);
    // Returning from an off-road edit needs an explicit reviewed connector.
    let edited = openride_author::corrections::decide(
        &root,
        &edited.revision,
        openride_author::corrections::Edit {
            source_sample: count / 2 + 1,
            mode: "trace".into(),
            position: None,
            way: None,
        },
        None,
    )
    .unwrap();
    let rebuilt = assembly::build(&root, &edited.revision, false, None).unwrap();
    let changed = rebuilt.build.as_ref().unwrap();
    assert_ne!(changed.key, record.key);
    assert!(changed.reused_chunks > 0);
    assert!(changed.reused_chunks < changed.chunks);
    // A reviewed manual detour/trace must not leave the rider inside the DEM.
    // Check actual compiled terrain, including the off-road edit and its return.
    if count == 21 {
        let changed_dir = root.join(&changed.path);
        let route: Value =
            serde_json::from_slice(&fs::read(changed_dir.join("route.json")).unwrap()).unwrap();
        let route_points = route["points"].as_array().unwrap();
        let manifest: Value =
            serde_json::from_slice(&fs::read(changed_dir.join("manifest.json")).unwrap()).unwrap();
        let mut clearance_checks = 0;
        for entry in manifest["chunks"].as_array().unwrap() {
            let payload: Value = serde_json::from_slice(
                &fs::read(changed_dir.join(entry["path"].as_str().unwrap())).unwrap(),
            )
            .unwrap();
            for v in payload["terrain"]["vertices"].as_array().unwrap() {
                let x = v[0].as_f64().unwrap();
                let z = v[2].as_f64().unwrap();
                for pair in route_points.windows(2) {
                    let a = &pair[0]["position"];
                    let b = &pair[1]["position"];
                    let ax = a[0].as_f64().unwrap();
                    let az = a[2].as_f64().unwrap();
                    let dx = b[0].as_f64().unwrap() - ax;
                    let dz = b[2].as_f64().unwrap() - az;
                    let t = (((x - ax) * dx + (z - az) * dz) / (dx * dx + dz * dz)).clamp(0., 1.);
                    if (x - ax - t * dx).hypot(z - az - t * dz) <= 12. {
                        let y = a[1].as_f64().unwrap() * (1. - t) + b[1].as_f64().unwrap() * t;
                        assert!(
                            v[1].as_f64().unwrap() <= y - 0.2,
                            "Terrain buries reviewed course at {x},{z}"
                        );
                        clearance_checks += 1;
                        break;
                    }
                }
            }
        }
        assert!(clearance_checks > 100);
    }
    openride_author::package::validate(&directory).unwrap();
    if count == 21 {
        let legacy = fs::read(root.join("course.json")).unwrap();
        let corrections = fs::read(root.join("corrections.json")).unwrap();
        let roads =
            openride_author::road_course::build(&root, &rebuilt.revision, None, None).unwrap();
        assert_eq!(roads.course.as_ref().unwrap().unresolved, 0);
        assert!(roads.course.as_ref().unwrap().roads.is_some());
        let generated = assembly::build(&root, &roads.revision, false, None).unwrap();
        let path = root.join(&generated.build.as_ref().unwrap().path);
        let road_route: Value =
            serde_json::from_slice(&fs::read(path.join("route.json")).unwrap()).unwrap();
        let points = road_route["points"].as_array().unwrap();
        assert!(
            points.last().unwrap()["elevation_m"].as_f64().unwrap()
                - points[0]["elevation_m"].as_f64().unwrap()
                > 3.,
            "Road elevation must follow sloped terrain, not the flat recording"
        );
        assert_eq!(fs::read(root.join("course.json")).unwrap(), legacy);
        assert_eq!(
            fs::read(root.join("corrections.json")).unwrap(),
            corrections
        );
        assert!(openride_author::open(&root).unwrap().build.unwrap().current);
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_openride-author"))
            .args(["project", "choose-road"])
            .arg(&root)
            .args([
                "--expected-revision",
                &generated.revision,
                "--sample",
                "10",
                "--way",
                "100",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        let chosen = openride_author::open(&root).unwrap();
        assert!(chosen
            .course
            .unwrap()
            .sections
            .iter()
            .any(|s| s.source_sample == 10 && s.status == "manual_match"));
        println!("ROAD_FIRST_OK terrain_profile preserved_recording_and_legacy cli_choice reopen");
    }
    println!(
        "NATIVE_ASSEMBLY_OK chunks={} shared_edges={} edit_reused={} project={}",
        record.chunks,
        shared,
        changed.reused_chunks,
        root.display()
    );
}
