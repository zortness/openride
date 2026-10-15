use ride_core::live::LiveRide;
#[test]
fn route_byte_budget_accepts_eight_mib_but_keeps_point_limit() {
    let mut raw =
        br#"{"points":[{"distance_m":0,"elevation_m":0},{"distance_m":1000,"elevation_m":0}]}"#
            .to_vec();
    raw.resize(ride_core::MAX_ROUTE_PROFILE_BYTES, b' ');
    assert!(LiveRide::parse(&raw).is_ok());
    raw.push(b' ');
    assert!(LiveRide::parse(&raw).err().unwrap().contains("8 MiB"));
    let points: Vec<_> = (0..10001)
        .map(|i| serde_json::json!({"distance_m":i,"elevation_m":0}))
        .collect();
    assert!(
        LiveRide::parse(&serde_json::to_vec(&serde_json::json!({"points":points})).unwrap())
            .is_err()
    );
}
#[test]
fn fresh_power_moves_and_loss_requires_explicit_resume() {
    let mut ride = LiveRide::parse(
        br#"{"points":[{"distance_m":0,"elevation_m":0},{"distance_m":10000,"elevation_m":0}]}"#,
    )
    .unwrap();
    ride.telemetry(0, Some(200.));
    assert!(ride.resume(0));
    for t in (20..=1000).step_by(20) {
        ride.tick(t);
    }
    assert!(ride.snapshot()["distance_m"].as_f64().unwrap() > 0.);
    ride.telemetry(1000, None);
    ride.tick(1020);
    let held = ride.snapshot()["distance_m"].clone();
    ride.telemetry(1040, Some(200.));
    ride.tick(1060);
    assert_eq!(ride.snapshot()["distance_m"], held);
    assert!(ride.resume(1060));
    ride.tick(1080);
    assert!(ride.snapshot()["distance_m"].as_f64().unwrap() > held.as_f64().unwrap());
}

#[test]
fn freshness_gap_and_pause_do_not_auto_resume_or_catch_up() {
    let profile =
        br#"{"points":[{"distance_m":0,"elevation_m":0},{"distance_m":10000,"elevation_m":1000}]}"#;
    let mut ride = LiveRide::parse(profile).unwrap();
    assert!(!ride.resume(0));
    ride.telemetry(0, Some(300.));
    assert!(ride.resume(0));
    for t in (20..=3000).step_by(20) {
        ride.tick(t);
    }
    assert_eq!(ride.status(), "power_missing");
    let held = ride.snapshot()["distance_m"].clone();
    ride.telemetry(3020, Some(300.));
    ride.tick(3040);
    assert_eq!(ride.snapshot()["distance_m"], held);
    assert!(ride.resume(3040));
    ride.tick(3300);
    assert_eq!(ride.status(), "timing_gap");
    assert_eq!(ride.snapshot()["distance_m"], held);
    assert_eq!(ride.grade_percent(), 10.);
    ride.pause(3400);
    ride.tick(4000);
    assert_eq!(ride.snapshot()["distance_m"], held);
}
