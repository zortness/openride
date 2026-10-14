use openride_author::{create, import, maps, select_sources};
use serde_json::json;
use std::{
    fs,
    io::Cursor,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Work(PathBuf);
impl Drop for Work {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn fixture() -> (Work, openride_author::Snapshot) {
    let root = std::env::temp_dir().join(format!(
        "or-maps-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&root).unwrap();
    let source = root.join("source.gpx");
    fs::write(
        &source,
        include_str!("../../../tools/fixtures/ride-project.gpx"),
    )
    .unwrap();
    let project = root.join("Project");
    let empty = create(&project, "Maps", None).unwrap();
    let imported = import(&project, &empty.revision, &[source], None).unwrap();
    let hash = &imported.review.as_ref().unwrap().document.sources[0].sha256;
    let selected = select_sources(&project, &imported.revision, hash, hash, None).unwrap();
    (Work(root), selected)
}
fn fetch(url: &str, _body: Option<&str>) -> openride_author::Result<Vec<u8>> {
    if url.ends_with("tile.png") {
        let mut raw = vec![];
        {
            let mut encoder = png::Encoder::new(&mut raw, 1024, 1024);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer
                .write_image_data(&vec![120; 1024 * 1024 * 3])
                .unwrap();
        }
        return Ok(raw);
    }
    if url.ends_with("tile.tif") {
        let mut data = Cursor::new(Vec::new());
        tiff::encoder::TiffEncoder::new(&mut data)
            .unwrap()
            .write_image::<tiff::encoder::colortype::Gray32Float>(256, 256, &vec![10.; 256 * 256])
            .unwrap();
        return Ok(data.into_inner());
    }
    if url.contains("exportImage") {
        let parsed = reqwest::Url::parse(url).unwrap();
        let bbox = parsed.query_pairs().find(|p| p.0 == "bbox").unwrap().1;
        let b: Vec<f64> = bbox.split(',').map(|v| v.parse().unwrap()).collect();
        if url.contains("imagery.nationalmap.gov") {
            return Ok(serde_json::to_vec(&json!({"width":1024,"height":1024,"extent":{"xmin":b[0],"ymin":b[1],"xmax":b[2],"ymax":b[3],"spatialReference":{"wkid":4326}},"href":"https://imagery.nationalmap.gov/tile.png"})).unwrap());
        }
        return Ok(serde_json::to_vec(&json!({"width":256,"height":256,"extent":{"xmin":b[0],"ymin":b[1],"xmax":b[2],"ymax":b[3],"spatialReference":{"wkid":4326}},"href":"https://elevation.nationalmap.gov/tile.tif"})).unwrap());
    }
    if url.contains("overpass") {
        return Ok(b"<osm version=\"0.6\"></osm>".to_vec());
    }
    Ok(b"{}".to_vec())
}
#[test]
fn imagery_receipts_resume_and_corrupt_tiles_are_reacquired() {
    let (work, current) = fixture();
    let root = work.0.join("Project");
    let first = maps::acquire_with(&root, &current.revision, "imagery", 1, None, fetch).unwrap();
    let plan = first.maps.unwrap();
    assert_eq!(plan.imagery_ready, 1);
    let mut calls = 0;
    let next = maps::acquire_with(&root, &current.revision, "imagery", 1, None, |u, b| {
        calls += 1;
        fetch(u, b)
    })
    .unwrap();
    assert_eq!(calls, 3);
    assert_eq!(next.maps.unwrap().imagery_ready, 2);
    fs::write(
        root.join("maps")
            .join(&plan.key)
            .join(&plan.tiles[0].id)
            .join("imagery.png"),
        b"bad",
    )
    .unwrap();
    assert_eq!(
        maps::acquire_with(&root, &current.revision, "imagery", 1, None, fetch)
            .unwrap()
            .maps
            .unwrap()
            .imagery_ready,
        2
    );
}
#[test]
fn full_corridor_plans_sparse_segments_and_resumes_verified_tiles() {
    let (work, current) = fixture();
    let root = work.0.join("Project");
    let plan = maps::plan(&current).unwrap();
    assert!(plan.tiles.len() > 4);
    assert!(plan.resolution_m < 8.);
    let first = maps::acquire_with(&root, &current.revision, "terrain", 1, None, fetch).unwrap();
    assert_eq!(first.maps.as_ref().unwrap().terrain_ready, 1);
    let mut calls = 0;
    let next = maps::acquire_with(&root, &current.revision, "terrain", 1, None, |u, b| {
        calls += 1;
        fetch(u, b)
    })
    .unwrap();
    assert_eq!(calls, 4);
    assert_eq!(next.maps.as_ref().unwrap().terrain_ready, 2);
    let tile = &plan.tiles[0];
    let path = root
        .join("maps")
        .join(&plan.key)
        .join(&tile.id)
        .join("terrain.tif");
    fs::write(path, b"corrupt").unwrap();
    let repaired = maps::acquire_with(&root, &current.revision, "terrain", 1, None, fetch).unwrap();
    assert_eq!(repaired.maps.as_ref().unwrap().terrain_ready, 2);
    let moved = work.0.join("Moved");
    fs::rename(&root, &moved).unwrap();
    assert_eq!(
        maps::save_plan(&moved, &current.revision, None)
            .unwrap()
            .maps
            .unwrap()
            .terrain_ready,
        2
    );
}
#[test]
fn cancelled_or_bad_responses_never_mark_tile_complete() {
    let (work, current) = fixture();
    let root = work.0.join("Project");
    let cancel = work.0.join("cancel");
    let result = maps::acquire_with(
        &root,
        &current.revision,
        "terrain",
        1,
        Some(&cancel),
        |u, b| {
            let raw = fetch(u, b)?;
            if u.ends_with("tile.tif") {
                fs::write(&cancel, b"").unwrap();
            }
            Ok(raw)
        },
    );
    assert_eq!(result.unwrap_err().code, "cancelled");
    fs::remove_file(cancel).unwrap();
    assert_eq!(
        maps::save_plan(&root, &current.revision, None)
            .unwrap()
            .maps
            .unwrap()
            .terrain_ready,
        0
    );
    assert!(
        maps::acquire_with(&root, &current.revision, "streets", 1, None, |_, _| Ok(
            b"<osm><remark>timeout</remark></osm>".to_vec()
        ))
        .is_err()
    );
    let resumed = maps::acquire_with(&root, &current.revision, "terrain", 1, None, fetch).unwrap();
    assert_eq!(resumed.maps.unwrap().terrain_ready, 1);
}
#[test]
fn project_edits_reopen_undo_detect_stale_writers_and_report_map_conflicts() {
    use openride_author::{
        corrections::{self, Edit},
        course, open,
    };
    let (work, current) = fixture();
    let root = work.0.join("Project");
    let acquired = maps::acquire_with(&root, &current.revision, "streets", 1, None, fetch).unwrap();
    let built = course::build(&root, &acquired.revision, None).unwrap();
    let point = Edit {
        source_sample: 1,
        mode: "point".into(),
        position: Some([33.0201, -118.]),
        way: None,
    };
    let edited = corrections::decide(&root, &built.revision, point.clone(), None).unwrap();
    assert_ne!(edited.revision, built.revision);
    assert_eq!(edited.course.as_ref().unwrap().sections[1].status, "manual");
    let edited = openride_author::rename(&root, &edited.revision, "Renamed ride", None).unwrap();
    assert_eq!(edited.course.as_ref().unwrap().sections[1].status, "manual");
    assert_eq!(
        corrections::decide(&root, &built.revision, point, None)
            .unwrap_err()
            .code,
        "stale_revision"
    );
    let reopened = open(&root).unwrap();
    assert_eq!(
        reopened.course.unwrap().sections[1].geometry.last(),
        Some(&[33.0201, -118.])
    );
    let keep = Edit {
        source_sample: 2,
        mode: "trace".into(),
        position: None,
        way: None,
    };
    let kept = corrections::decide(&root, &edited.revision, keep, None).unwrap();
    let undone = corrections::decide(
        &root,
        &kept.revision,
        Edit {
            source_sample: 1,
            mode: "undo".into(),
            position: None,
            way: None,
        },
        None,
    )
    .unwrap();
    assert_eq!(undone.course.as_ref().unwrap().sections[2].status, "manual");
    assert_ne!(undone.course.as_ref().unwrap().sections[1].status, "manual");
    let changed = maps::acquire_with(&root, &undone.revision, "streets", 1, None, fetch).unwrap();
    assert!(changed.course.is_none());
    assert_eq!(
        course::build(&root, &changed.revision, None)
            .unwrap_err()
            .code,
        "correction_conflict"
    );
    let archived = corrections::archive(&root, &changed.revision, None).unwrap();
    assert_eq!(
        fs::read_dir(root.join("jobs"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("corrections-"))
            .count(),
        1
    );
    assert!(course::build(&root, &archived.revision, None).is_ok());
}
