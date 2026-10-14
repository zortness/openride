use openride_author::{
    course::match_trace,
    streets::{Feature, Graph},
};
use ride_core::Sample;
use std::collections::BTreeMap;
fn graph(points: &[(i64, f64, f64)], ways: &[(i64, Vec<i64>, &str)]) -> Graph {
    let mut g = Graph::default();
    for &(id, x, y) in points {
        g.features.insert(
            format!("node/{id}"),
            Feature {
                id,
                version: 1,
                kind: "node".into(),
                position: Some([y / 111195., x / 111195.]),
                nodes: vec![],
                members: vec![],
                tags: BTreeMap::new(),
            },
        );
    }
    for (id, nodes, oneway) in ways {
        g.features.insert(
            format!("way/{id}"),
            Feature {
                id: *id,
                version: 1,
                kind: "way".into(),
                position: None,
                nodes: nodes.clone(),
                members: vec![],
                tags: BTreeMap::from([
                    ("highway".into(), "cycleway".into()),
                    ("oneway".into(), oneway.to_string()),
                ]),
            },
        );
    }
    g
}
fn samples(points: &[(f64, f64)]) -> Vec<Sample> {
    points
        .iter()
        .map(|&(x, y)| Sample {
            latitude: Some(y / 111195.),
            longitude: Some(x / 111195.),
            elevation_m: Some(42.),
            ..Default::default()
        })
        .collect()
}
fn run(points: &[(f64, f64)], g: &Graph) -> Vec<openride_author::course::Section> {
    let s = samples(points);
    match_trace(&s, &(0..s.len()).collect::<Vec<_>>(), g, None).unwrap()
}
#[test]
fn road_sequence_uses_future_evidence_and_preserves_intermediate_way_identity() {
    let g = graph(
        &[
            (1, -5., 0.),
            (2, -5., 100.),
            (3, 50., 100.),
            (4, 5., 0.),
            (5, 5., 80.),
        ],
        &[
            (10, vec![1, 2], "no"),
            (11, vec![2, 3], "no"),
            (12, vec![4, 5], "no"),
        ],
    );
    let (s, m) = openride_author::road_course::infer(
        &samples(&[(0., 10.), (0., 60.), (45., 100.)]),
        &g,
        &BTreeMap::new(),
        None,
    )
    .unwrap();
    assert!(m.issues.is_empty(), "{s:?} {m:?}");
    assert_eq!(s[0].way, Some(10));
    assert!(m.section_ways[&2].contains(&10) && m.section_ways[&2].contains(&11));
    for pair in s.windows(2) {
        assert_eq!(pair[0].geometry.last(), pair[1].geometry.first());
    }
}
#[test]
fn road_sequence_groups_ambiguity_and_road_choice_resolves_the_run() {
    let g = graph(
        &[(1, -5., 0.), (2, -5., 200.), (3, 5., 0.), (4, 5., 200.)],
        &[(10, vec![1, 2], "no"), (11, vec![3, 4], "no")],
    );
    let recording = samples(&[(0., 10.), (0., 60.), (0., 110.), (0., 160.)]);
    let (_, m) =
        openride_author::road_course::infer(&recording, &g, &BTreeMap::new(), None).unwrap();
    assert_eq!(m.issues.len(), 1);
    assert_eq!(m.issues[0].last_sample, 3);
    let (s, m) =
        openride_author::road_course::infer(&recording, &g, &BTreeMap::from([(1, 11)]), None)
            .unwrap();
    assert!(m.issues.is_empty());
    assert!(s.iter().all(|s| s.way == Some(11)));
}
#[test]
fn road_only_excludes_trails_and_preserves_recording_gaps_and_oneway() {
    let mut g = graph(
        &[(1, 0., 0.), (2, 0., 200.), (3, 5., 0.), (4, 5., 200.)],
        &[(10, vec![1, 2], "no"), (11, vec![3, 4], "yes")],
    );
    g.features
        .get_mut("way/10")
        .unwrap()
        .tags
        .insert("highway".into(), "path".into());
    let mut recording = samples(&[(0., 10.), (0., 60.), (0., 110.)]);
    recording[1].break_before = true;
    let (s, m) =
        openride_author::road_course::infer(&recording, &g, &BTreeMap::new(), None).unwrap();
    assert!(s.iter().all(|s| s.way == Some(11)));
    assert_eq!(m.issues[0].reason, "recording_gap");
    recording.reverse();
    recording.iter_mut().for_each(|s| s.break_before = false);
    let (_, m) =
        openride_author::road_course::infer(&recording, &g, &BTreeMap::new(), None).unwrap();
    assert!(!m.issues.is_empty());
}

#[test]
fn recorded_connection_does_not_offer_long_or_missing_recording_links() {
    use openride_author::road_course::infer;
    let g = graph(
        &[(1, 0., 0.), (2, 0., 100.), (3, 0., 400.), (4, 0., 500.)],
        &[(10, vec![1, 2], "no"), (11, vec![3, 4], "no")],
    );
    let (_, meta) = infer(
        &samples(&[(0., 50.), (0., 450.)]),
        &g,
        &BTreeMap::new(),
        None,
    )
    .unwrap();
    assert!(meta.connection_previews.is_empty());
    let g = graph(
        &[(1, 0., 0.), (2, 0., 100.), (3, 0., 120.), (4, 0., 200.)],
        &[(10, vec![1, 2], "no"), (11, vec![3, 4], "no")],
    );
    let mut recording = samples(&[(0., 50.), (0., 170.)]);
    recording[1].break_before = true;
    let (_, meta) = infer(&recording, &g, &BTreeMap::new(), None).unwrap();
    assert!(meta.connection_previews.is_empty());
}

#[test]
fn propagation_follows_named_connected_pieces_but_stops_at_gaps_choices_and_returns() {
    use openride_author::road_course::{infer, nearby_choices};
    let mut g = graph(
        &[
            (1, -5., 0.),
            (2, -5., 100.),
            (3, -5., 200.),
            (4, 5., 0.),
            (5, 5., 200.),
        ],
        &[
            (10, vec![1, 2], "no"),
            (11, vec![2, 3], "no"),
            (12, vec![4, 5], "no"),
        ],
    );
    for id in [10, 11, 12] {
        g.features
            .get_mut(&format!("way/{id}"))
            .unwrap()
            .tags
            .insert("name".into(), "Main Road".into());
    }
    let mut recording = samples(&[(0., 10.), (0., 60.), (0., 110.), (0., 160.)]);
    let (sections, _) = infer(&recording, &g, &BTreeMap::new(), None).unwrap();
    let choices = nearby_choices(&recording, &sections, &g, &BTreeMap::new(), 0, 10).unwrap();
    assert_eq!(
        choices,
        BTreeMap::from([(0, 10), (1, 10), (2, 11), (3, 11)])
    );
    // A disconnected parallel way with the same name must not join the chain.
    assert!(!choices.values().any(|w| *w == 12));
    let constrained =
        nearby_choices(&recording, &sections, &g, &BTreeMap::from([(2, 12)]), 0, 10).unwrap();
    assert_eq!(constrained.len(), 2);
    recording[2].break_before = true;
    assert_eq!(
        nearby_choices(&recording, &sections, &g, &BTreeMap::new(), 0, 10)
            .unwrap()
            .len(),
        2
    );
    let recording = samples(&[(0., 10.), (0., 60.), (0., 110.), (0., 60.), (0., 10.)]);
    let (sections, _) = infer(&recording, &g, &BTreeMap::new(), None).unwrap();
    let choices = nearby_choices(&recording, &sections, &g, &BTreeMap::new(), 0, 10).unwrap();
    assert!(!choices.contains_key(&3) && !choices.contains_key(&4));
    // A three-way fork sharing the road name is not a unique continuation.
    let mut fork = g.features["way/11"].clone();
    fork.id = 13;
    fork.nodes = vec![2, 5];
    g.features.insert("way/13".into(), fork);
    let recording = samples(&[(0., 10.), (0., 60.), (0., 110.), (0., 160.)]);
    let (sections, _) = infer(&recording, &g, &BTreeMap::new(), None).unwrap();
    assert!(
        !nearby_choices(&recording, &sections, &g, &BTreeMap::new(), 0, 10)
            .unwrap()
            .values()
            .any(|w| *w == 11)
    );
}
#[test]
fn follows_connected_corner_and_preserves_repeated_occurrences_and_heights() {
    let g = graph(
        &[(1, 0., 0.), (2, 0., 100.), (3, 100., 100.)],
        &[(10, vec![1, 2], "no"), (11, vec![2, 3], "no")],
    );
    let sections = run(
        &[(0., 10.), (0., 90.), (90., 100.), (0., 90.), (0., 10.)],
        &g,
    );
    assert!(
        sections.iter().all(|s| s.status == "matched"),
        "{sections:?}"
    );
    assert!(sections[2].geometry.len() >= 3);
    assert!((sections[2].length_m - 100.).abs() < 0.1);
    assert_eq!(sections[4].source_sample, 4);
    assert!(sections.iter().all(|s| s.recorded_elevation_m == Some(42.)));
    for w in sections.windows(2) {
        assert_eq!(w[0].geometry.last(), w[1].geometry.first());
    }
}
#[test]
fn parallel_paths_and_stacked_crossings_remain_uncertain() {
    let parallel = graph(
        &[(1, -5., 0.), (2, -5., 100.), (3, 5., 0.), (4, 5., 100.)],
        &[(10, vec![1, 2], "no"), (11, vec![3, 4], "no")],
    );
    assert!(run(&[(0., 10.), (0., 90.)], &parallel)
        .iter()
        .all(|s| s.status == "ambiguous"));
    let crossing = graph(
        &[(1, -100., 0.), (2, 100., 0.), (3, 0., -100.), (4, 0., 100.)],
        &[(10, vec![1, 2], "no"), (11, vec![3, 4], "no")],
    );
    let result = run(&[(-80., 0.), (0., 80.)], &crossing);
    assert_eq!(result[1].status, "unmatched");
}
#[test]
fn oneway_coverage_gaps_and_recording_breaks_do_not_invent_connections() {
    let g = graph(&[(1, 0., 0.), (2, 0., 100.)], &[(10, vec![1, 2], "yes")]);
    assert_eq!(run(&[(0., 90.), (0., 10.)], &g)[1].status, "unmatched");
    assert_eq!(run(&[(200., 0.), (200., 100.)], &g)[1].status, "unmatched");
    let mut s = samples(&[(0., 10.), (0., 90.)]);
    s[1].break_before = true;
    let result = match_trace(&s, &[7, 90], &g, None).unwrap();
    assert!(result[1].break_before);
    assert_eq!(result[1].length_m, 0.);
    assert_eq!(result[1].source_sample, 90);
}
#[test]
fn manual_way_choice_and_gap_join_apply_only_to_the_chosen_occurrence() {
    use openride_author::{corrections::Edit, course::match_with_edits};
    let g = graph(
        &[(1, -5., 0.), (2, -5., 100.), (3, 5., 0.), (4, 5., 100.)],
        &[(10, vec![1, 2], "no"), (11, vec![3, 4], "no")],
    );
    let mut s = samples(&[(0., 10.), (0., 90.), (0., 10.)]);
    let edits = BTreeMap::from([(
        0,
        Edit {
            source_sample: 0,
            mode: "way".into(),
            position: None,
            way: Some(11),
        },
    )]);
    let selected = match_with_edits(&s, &[0, 1, 2], &g, &edits, None).unwrap();
    assert_eq!(selected[0].way, Some(11));
    assert_eq!(selected[0].status, "manual_match");
    assert_eq!(selected[2].source_sample, 2);
    assert_eq!(selected[2].status, "matched");
    s[1].break_before = true;
    let joined = BTreeMap::from([(
        1,
        Edit {
            source_sample: 1,
            mode: "join".into(),
            position: Some([90. / 111195., 0.]),
            way: None,
        },
    )]);
    let result = match_with_edits(&s, &[0, 1, 2], &g, &joined, None).unwrap();
    assert!(!result[1].break_before);
    assert!(result[1].length_m > 79.);
    assert_eq!(result[1].status, "manual");
    assert!(match_trace(&s, &[0, 1, 2], &g, None).unwrap()[1].break_before);
}
