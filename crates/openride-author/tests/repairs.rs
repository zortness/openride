use openride_author::{create, import, open, repairs, select_sources};
use ride_core::{Activity, Sample};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Work(PathBuf);
impl Work {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "or-repairs-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn activity(xy: &[(f64, f64)]) -> Activity {
    Activity {
        name: "Synthetic".into(),
        source_name: "synthetic.gpx".into(),
        source_sha256: "a".repeat(64),
        format: "GPX".into(),
        samples: xy
            .iter()
            .enumerate()
            .map(|(i, (x, y))| Sample {
                latitude: Some(33. + y / 111195.),
                longitude: Some(-118. + x / (111195. * 33_f64.to_radians().cos())),
                elevation_m: Some(10.),
                time_ms: Some(i as i64 * 2000),
                ..Sample::default()
            })
            .collect(),
        laps: vec![],
        recorded_distance_m: None,
        timer_s: None,
        elapsed_s: None,
        route_distance_m: 0.,
        diagnostics: vec![],
    }
}
#[test]
fn loop_detection_preserves_hairpins_later_visits_and_recorded_motion() {
    let looped = activity(&[(0., 0.), (3., 0.), (3., 3.), (0., 3.), (0., 0.), (20., 0.)]);
    assert_eq!(repairs::candidates(&looped).len(), 1);
    assert_eq!(repairs::candidates(&looped)[0].decision.last, 4);
    for xy in [
        vec![(0., 0.), (3., 0.), (6., 0.), (3., 0.), (0., 0.)],
        vec![(0., 0.), (30., 0.), (30., 30.), (0., 30.), (0., 0.)],
    ] {
        assert!(repairs::candidates(&activity(&xy)).is_empty());
    }
    let mut moving = looped.clone();
    for s in &mut moving.samples {
        s.speed_mps = Some(4.);
    }
    assert!(repairs::candidates(&moving).is_empty());
    let mut gap = looped.clone();
    gap.samples[2].break_before = true;
    assert!(repairs::candidates(&gap)
        .iter()
        .all(|c| c.decision.kind != "tiny_loop"));
    let mut slow = looped;
    slow.samples[4].time_ms = Some(60_000);
    assert!(repairs::candidates(&slow).is_empty());
}
#[test]
fn gaps_never_flatten_elevations_or_invent_missing_positions() {
    let mut a = activity(&[(0., 0.), (3., 0.)]);
    a.samples[1].break_before = true;
    assert!(repairs::candidates(&a)[0].allowed);
    a.samples[1].elevation_m = Some(12.);
    assert!(!repairs::candidates(&a)[0].allowed);
    a.samples[1].latitude = None;
    assert!(!repairs::candidates(&a)[0].allowed);
}
#[test]
fn decisions_reopen_undo_and_preserve_source_occurrences_and_heights() {
    let work = Work::new();
    let mut a = activity(&[
        (0., 0.),
        (3., 0.),
        (3., 3.),
        (0., 3.),
        (0., 0.),
        (20., 0.),
        (21., 0.),
    ]);
    a.samples[6].break_before = true;
    let mut xml = String::from("<gpx><trk><trkseg>");
    for (i, s) in a.samples.iter().enumerate() {
        if i == 6 {
            xml.push_str("</trkseg><trkseg>");
        }
        xml.push_str(&format!("<trkpt lat=\"{}\" lon=\"{}\"><ele>10</ele><time>2022-01-01T00:00:{:02}Z</time></trkpt>",s.latitude.unwrap(),s.longitude.unwrap(),i*2));
    }
    xml.push_str("</trkseg></trk></gpx>");
    let source = work.0.join("source.gpx");
    fs::write(&source, &xml).unwrap();
    let root = work.0.join("Project");
    let empty = create(&root, "Repairs", None).unwrap();
    let intake = import(&root, &empty.revision, &[source.clone()], None).unwrap();
    let hash = intake.review.as_ref().unwrap().document.sources[0]
        .sha256
        .clone();
    let selected = select_sources(&root, &intake.revision, &hash, &hash, None).unwrap();
    let report = selected.repair_preview.as_ref().unwrap();
    let loop_id = report
        .candidates
        .iter()
        .find(|c| c.decision.kind == "tiny_loop")
        .unwrap()
        .id
        .clone();
    let gap_id = report
        .candidates
        .iter()
        .find(|c| c.decision.kind == "gap")
        .unwrap()
        .id
        .clone();
    let accepted = repairs::decide(&root, &selected.revision, &loop_id, "accepted", None).unwrap();
    let preview = accepted.repair_preview.as_ref().unwrap();
    assert_eq!(preview.source_samples, vec![0, 4, 5, 6]);
    assert!(preview.samples.iter().all(|s| s.elevation_m == Some(10.)));
    assert!(preview.length_m < report.length_m);
    let joined = repairs::decide(&root, &accepted.revision, &gap_id, "accepted", None).unwrap();
    assert_eq!(joined.repair_preview.as_ref().unwrap().unresolved_breaks, 0);
    assert_eq!(open(&root).unwrap().revision, joined.revision);
    assert!(repairs::decide(&root, &selected.revision, &loop_id, "pending", None).is_err());
    let undone = repairs::decide(&root, &joined.revision, &loop_id, "pending", None).unwrap();
    assert_eq!(undone.repair_preview.as_ref().unwrap().samples.len(), 7);
    let rejected = repairs::decide(&root, &undone.revision, &gap_id, "rejected", None).unwrap();
    assert_eq!(
        rejected.repair_preview.as_ref().unwrap().unresolved_breaks,
        1
    );
    assert_eq!(fs::read_to_string(source).unwrap(), xml);
    let moved = work.0.join("Moved");
    fs::rename(root, &moved).unwrap();
    assert_eq!(open(&moved).unwrap().revision, rejected.revision);
}
