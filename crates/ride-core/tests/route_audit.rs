use ride_core::{
    full_route::{Point, Profile},
    route_audit,
};

fn profile(points: &[(f64, f64)]) -> Profile {
    Profile {
        points: points
            .iter()
            .enumerate()
            .map(|(sample, (distance, height))| Point {
                sample,
                distance: *distance,
                height: *height,
                lat: 33.,
                lon: -118.,
                break_before: false,
            })
            .collect(),
        bridged_pauses: 0,
        unresolved_breaks: 0,
        issues: vec![],
        revision: "synthetic".into(),
    }
}

#[test]
fn rehearsal_finishes_flat_route_and_reports_stalls_and_limits() {
    let flat = profile(&[(0., 10.), (1000., 10.)]);
    let result = route_audit::rehearse(&flat, 200., 3600).unwrap();
    assert_eq!(result.status, "finished");
    assert_eq!(result.distance_m, 1000.);
    let result = route_audit::rehearse(&profile(&[(0., 0.), (100., 90.)]), 150., 3600).unwrap();
    assert_eq!(result.status, "stalled_at_test_power");
    assert_eq!(result.simulated_seconds, 60.);
    assert_eq!(result.source_sample, 1);
    let result = route_audit::rehearse(&flat, 200., 60).unwrap();
    assert_eq!(result.status, "time_limit");
    assert!(result.distance_m < 1000.);
    assert!(route_audit::rehearse(&flat, f64::NAN, 3600).is_err());
    assert!(route_audit::rehearse(&flat, 200., 86401).is_err());
    let mut broken = flat;
    broken.unresolved_breaks = 1;
    assert!(route_audit::rehearse(&broken, 200., 3600).is_err());
}

#[test]
fn context_distinguishes_tiny_spikes_from_sustained_climbs_without_editing() {
    let noisy = profile(&[(0., 10.), (10., 10.), (10.2, 10.4), (11., 10.), (30., 10.)]);
    let findings = route_audit::steep_findings(&noisy);
    assert_eq!(findings.len(), 2);
    assert!(findings[0].raw_grade_percent > 100.);
    assert!(findings
        .iter()
        .all(|f| f.context_20m_grade_percent.unwrap().abs() < 0.001));
    assert_eq!(noisy.points[2].height, 10.4);
    let climb = route_audit::steep_findings(&profile(&[(0., 10.), (50., 20.), (100., 30.)]));
    assert_eq!(climb.len(), 2);
    assert!(climb
        .iter()
        .all(|f| (f.context_20m_grade_percent.unwrap() - 20.).abs() < 0.001));
}
