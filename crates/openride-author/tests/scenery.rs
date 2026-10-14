use openride_author::{
    scenery::{self, Feature, View},
    scenery_mesh::World,
};
use std::{collections::BTreeSet, fs};
fn ring(x: f64, z: f64, w: f64, h: f64) -> Vec<[f64; 2]> {
    vec![
        [z / 100000., x / 100000.],
        [z / 100000., (x + w) / 100000.],
        [(z + h) / 100000., (x + w) / 100000.],
        [(z + h) / 100000., x / 100000.],
    ]
}
fn feature(id: &str, kind: &str, x: f64, z: f64, w: f64, h: f64) -> Feature {
    Feature {
        id: id.into(),
        kind: kind.into(),
        outer: ring(x, z, w, h),
        holes: vec![],
        height_m: Some(6.),
        elevation_m: None,
        density: 1.,
        trees: true,
        hidden: false,
        provenance: "Synthetic test".into(),
    }
}
fn world(features: Vec<Feature>) -> World {
    World::prepare(
        &View {
            features,
            warnings: vec![],
            conflicts: vec![],
            revision: String::new(),
        },
        &BTreeSet::from([(0, 0), (1, 0)]),
        &[[0., 20., 0.], [512., 20., 0.]],
        |p| Ok([p[1] * 100000., p[0] * 100000.]),
        |_, _| Ok(10.),
        0.,
    )
    .unwrap()
}
#[test]
fn water_preserves_islands_chunk_edges_and_surface_level() {
    let mut water = feature("user-lake", "water", 200., 20., 120., 120.);
    water.holes.push(ring(240., 50., 40., 40.));
    water.elevation_m = Some(9.);
    let w = world(vec![water]);
    assert_eq!(w.water_level(220., 30.), Some(9.));
    assert_eq!(w.water_level(256., 70.), None);
    let mut area = 0.;
    let mut edge = vec![];
    for cx in 0..2 {
        let c = w.chunk(cx, 0, |_, _| Ok(10.)).unwrap();
        assert!(!c.water.indices.is_empty());
        for n in &c.water.normals {
            assert!(n[1] > 0.99);
        }
        for t in c.water.indices.chunks_exact(3) {
            let p = t.iter().map(|i| c.water.vertices[*i]).collect::<Vec<_>>();
            let center = [
                p.iter().map(|v| v[0]).sum::<f64>() / 3.,
                p.iter().map(|v| v[2]).sum::<f64>() / 3.,
            ];
            assert!(!((240.0..280.).contains(&center[0]) && (50.0..90.).contains(&center[1])));
            area += ((p[1][0] - p[0][0]) * (p[2][2] - p[0][2])
                - (p[1][2] - p[0][2]) * (p[2][0] - p[0][0]))
                .abs()
                / 2.;
        }
        edge.push(
            c.water
                .vertices
                .iter()
                .filter(|p| (p[0] - 256.).abs() < 1e-8)
                .map(|p| p[1])
                .collect::<Vec<_>>(),
        );
    }
    assert!((area - (120. * 120. - 40. * 40.)).abs() < 0.001);
    assert!(!edge[0].is_empty() && !edge[1].is_empty());
    assert!(edge.iter().flatten().all(|y| (*y - 9.05).abs() < 1e-8));
}
#[test]
fn buildings_and_deterministic_plants_respect_clearance_and_exclusions() {
    let trees = feature("user-trees", "vegetation", 0., 0., 510., 200.);
    let exclusion = feature("user-clear", "exclusion", 100., 20., 50., 80.);
    let building = feature("user-house", "building", 230., 120., 45., 30.);
    let blocked = feature("user-blocked", "building", 10., -2., 10., 8.);
    let w = world(vec![trees, exclusion, building, blocked]);
    assert!(w.warnings.iter().any(|s| s.contains("clearance")));
    for cx in 0..2 {
        let a = w.chunk(cx, 0, |_, _| Ok(10.)).unwrap();
        let b = w.chunk(cx, 0, |_, _| Ok(10.)).unwrap();
        assert_eq!(a.vegetation, b.vegetation);
        assert!(!a.buildings.vertices.is_empty());
        assert!(!a.vegetation.is_empty());
        assert!(a.vegetation.len() <= 1024);
        for p in a.vegetation {
            assert!(p[2] >= 8.);
            assert!(!(100.0..150.).contains(&p[0]) || !(20.0..100.).contains(&p[2]));
            assert!(!(230.0..275.).contains(&p[0]) || !(120.0..150.).contains(&p[2]));
            assert!((cx as f64 * 256. ..(cx + 1) as f64 * 256.).contains(&p[0]));
        }
    }
}
#[test]
fn source_multipolygons_join_members_preserve_holes_and_report_open_coast() {
    let raw=br#"<osm><node id="1" lat="0" lon="0"/><node id="2" lat="0" lon="0.002"/><node id="3" lat="0.002" lon="0.002"/><node id="4" lat="0.002" lon="0"/><node id="5" lat="0.0005" lon="0.0005"/><node id="6" lat="0.0005" lon="0.001"/><node id="7" lat="0.001" lon="0.001"/><node id="8" lat="0.001" lon="0.0005"/><way id="10"><nd ref="1"/><nd ref="2"/><nd ref="3"/></way><way id="11"><nd ref="1"/><nd ref="4"/><nd ref="3"/></way><way id="12"><nd ref="5"/><nd ref="6"/><nd ref="7"/><nd ref="8"/><nd ref="5"/></way><way id="13"><nd ref="1"/><nd ref="2"/><tag k="natural" v="coastline"/></way><relation id="20"><member type="way" ref="10" role="outer"/><member type="way" ref="11" role="outer"/><member type="way" ref="12" role="inner"/><tag k="type" v="multipolygon"/><tag k="natural" v="water"/></relation></osm>"#;
    let raw = String::from_utf8(raw.to_vec())
        .unwrap()
        .replace("<osm>", "<osm version=\"0.6\">")
        .replace(" id=", " version=\"1\" id=");
    let graph = openride_author::streets::parse(raw.as_bytes()).unwrap();
    let c = scenery::catalog(&graph);
    assert_eq!(c.features.len(), 1);
    assert_eq!(c.features["relation/20/0"].holes.len(), 1);
    assert_eq!(c.warnings.len(), 1);
}
#[test]
fn edits_persist_hide_undo_and_reject_stale_or_invalid_writes() {
    let root = std::env::temp_dir().join(format!("or-scenery-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let created = openride_author::create(&root, "Scenery", None).unwrap();
    let f = feature("user-one", "building", 10., 10., 20., 20.);
    let saved = scenery::edit(
        &root,
        &created.revision,
        "save",
        &f.id,
        Some(f.clone()),
        None,
    )
    .unwrap();
    assert_eq!(saved.scenery.features.len(), 1);
    assert_eq!(
        scenery::edit(&root, &created.revision, "hide", &f.id, None, None)
            .unwrap_err()
            .code,
        "stale_revision"
    );
    let hidden = scenery::edit(&root, &saved.revision, "hide", &f.id, None, None).unwrap();
    assert!(openride_author::open(&root).unwrap().scenery.features[0].hidden);
    let restored = scenery::edit(&root, &hidden.revision, "undo", &f.id, None, None).unwrap();
    assert!(!restored.scenery.features[0].hidden);
    let mut invalid = f.clone();
    invalid.holes.push(ring(200., 200., 10., 10.));
    assert!(scenery::edit(
        &root,
        &restored.revision,
        "save",
        &f.id,
        Some(invalid),
        None
    )
    .is_err());
    assert_eq!(
        openride_author::open(&root).unwrap().revision,
        restored.revision
    );
    let reset = scenery::edit(&root, &restored.revision, "reset", &f.id, None, None).unwrap();
    assert!(reset.scenery.features.is_empty());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn directed_coast_closes_only_within_coverage_and_preserves_islands() {
    use openride_author::streets::{Feature as Osm, Graph};
    use std::collections::BTreeMap;
    let mut graph = Graph::default();
    for (i, p) in [
        [-0.02, 0.],
        [0.02, 0.],
        [-0.002, 0.002],
        [-0.002, 0.004],
        [0.002, 0.004],
        [0.002, 0.002],
    ]
    .iter()
    .enumerate()
    {
        let id = i as i64 + 1;
        graph.features.insert(
            format!("node/{id}"),
            Osm {
                id,
                version: 1,
                kind: "node".into(),
                position: Some(*p),
                nodes: vec![],
                members: vec![],
                tags: BTreeMap::new(),
            },
        );
    }
    for (id, nodes) in [(10, vec![1, 2]), (11, vec![3, 4, 5, 6, 3])] {
        graph.features.insert(
            format!("way/{id}"),
            Osm {
                id,
                version: 1,
                kind: "way".into(),
                position: None,
                nodes,
                members: vec![],
                tags: BTreeMap::from([("natural".into(), "coastline".into())]),
            },
        );
    }
    let tiles = [openride_author::maps::Tile {
        id: "tile_0_0".into(),
        bounds: [-0.01, -0.01, 0.01, 0.01],
    }];
    let (features, warnings) = openride_author::coastline::water(&graph, &tiles);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(features.len(), 1);
    let f = &features[0];
    assert_eq!(f.holes.len(), 1);
    assert!(scenery::contains([0., 0.006], &f.outer, &f.holes));
    assert!(!scenery::contains([0., -0.006], &f.outer, &f.holes));
    assert!(!scenery::contains([0., 0.003], &f.outer, &f.holes));
    assert!(f
        .outer
        .iter()
        .all(|p| p[0].abs() < 0.011 && p[1].abs() < 0.011));
    let mut duplicate = graph.features["way/10"].clone();
    duplicate.id = 12;
    graph.features.insert("way/12".into(), duplicate);
    let (features, warnings) = openride_author::coastline::water(&graph, &tiles);
    assert!(features.is_empty());
    assert!(warnings[0].contains("branches"));
}

#[test]
fn acquisition_resumes_and_changed_features_conflict_without_losing_edits() {
    use openride_author::{create, import, maps, select_sources};
    let root = std::env::temp_dir().join(format!("or-scenery-maps-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let c = create(&root, "Scenery sources", None).unwrap();
    let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/fixtures/project-repairs.gpx");
    let imported = import(&root, &c.revision, &[input], None).unwrap();
    let id = &imported.review.as_ref().unwrap().document.sources[0].sha256;
    let chosen = select_sources(&root, &imported.revision, id, id, None).unwrap();
    let xml = |version: u32| {
        format!("<osm version=\"0.6\"><node id=\"1\" version=\"1\" lat=\"33\" lon=\"-118\"/><node id=\"2\" version=\"1\" lat=\"33\" lon=\"-117.999\"/><node id=\"3\" version=\"1\" lat=\"33.001\" lon=\"-117.999\"/><node id=\"4\" version=\"1\" lat=\"33.001\" lon=\"-118\"/><way id=\"20\" version=\"{version}\"><nd ref=\"1\"/><nd ref=\"2\"/><nd ref=\"3\"/><nd ref=\"4\"/><nd ref=\"1\"/><tag k=\"building\" v=\"yes\"/></way></osm>").into_bytes()
    };
    let acquired = maps::acquire_with(&root, &chosen.revision, "buildings", 1, None, |_, query| {
        assert!(query.unwrap().contains("way[building]"));
        Ok(xml(1))
    })
    .unwrap();
    assert_eq!(acquired.scenery.features.len(), 1);
    assert_eq!(
        acquired.maps.as_ref().unwrap().scenery_ready["buildings"],
        1
    );
    let mut f = acquired.scenery.features[0].clone();
    f.height_m = Some(12.);
    let saved = scenery::edit(
        &root,
        &acquired.revision,
        "save",
        &f.id,
        Some(f.clone()),
        None,
    )
    .unwrap();
    let plan = saved.maps.as_ref().unwrap();
    let dir = root
        .join("maps")
        .join(&plan.key)
        .join(&plan.tiles[0].id)
        .join("buildings");
    fs::write(dir.join("scenery.osm"), b"corrupt").unwrap();
    let updated = maps::acquire_with(&root, &saved.revision, "buildings", 1, None, |_, _| {
        Ok(xml(2))
    })
    .unwrap();
    assert_eq!(updated.scenery.conflicts, vec![f.id.clone()]);
    assert_eq!(updated.scenery.features[0].height_m, Some(12.));
    let reset = scenery::edit(&root, &updated.revision, "reset", &f.id, None, None).unwrap();
    assert!(reset.scenery.conflicts.is_empty());
    assert_eq!(reset.scenery.features[0].height_m, Some(6.));
    fs::remove_dir_all(root).unwrap();
}
