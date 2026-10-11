use ride_core::{
    replay::{Capture, Command, PowerSample, Replay, ReplayWorker},
    simulation::{Config, Simulation},
};
fn capture() -> Capture {
    Capture {
        samples: vec![
            PowerSample {
                at_ms: 0,
                watts: Some(200.),
            },
            PowerSample {
                at_ms: 1000,
                watts: Some(0.),
            },
            PowerSample {
                at_ms: 5000,
                watts: Some(200.),
            },
            PowerSample {
                at_ms: 6000,
                watts: Some(200.),
            },
        ],
        sha256: "fixture".into(),
    }
}
#[test]
fn replay_uses_timestamps_and_latches_staleness_until_explicit_resume() {
    let mut r = Replay::new(
        Simulation::new(vec![(0., 0.), (10000., 0.)], Config::default()).unwrap(),
        capture(),
    )
    .unwrap();
    r.resume();
    for _ in 0..50 {
        r.advance(20);
    }
    assert_eq!(r.snapshot().power_w, Some(0.));
    for _ in 0..201 {
        r.advance(20);
    }
    assert_eq!(r.snapshot().status, "power_stale");
    let held = r.snapshot().distance_m;
    for _ in 0..10 {
        r.advance(20);
    }
    assert_eq!(r.snapshot().distance_m, held);
    assert_eq!(r.snapshot().power_w, Some(200.));
    r.resume();
    r.advance(20);
    assert!(r.snapshot().distance_m > held);
}
#[test]
fn worker_progress_does_not_require_polling_and_pause_holds() {
    let w =
        ReplayWorker::start(vec![(0., 0.), (10000., 0.)], Config::default(), capture()).unwrap();
    assert!(w.command(Command::Resume));
    std::thread::sleep(std::time::Duration::from_millis(350));
    assert!(w.snapshot().distance_m > 0.);
    assert!(w.snapshot().replay_ms >= 200);
    assert!(w.command(Command::Pause));
    std::thread::sleep(std::time::Duration::from_millis(50));
    let held = w.snapshot().distance_m;
    std::thread::sleep(std::time::Duration::from_millis(60));
    assert_eq!(w.snapshot().distance_m, held);
}
#[test]
fn parser_preserves_missing_and_zero_and_rejects_truncation() {
    let raw=b"{\"schema\":1,\"event\":\"session_check_created\",\"sequence\":0,\"elapsed_ms\":0}\n{\"event\":\"indoor_bike_data\",\"sequence\":1,\"elapsed_ms\":20,\"decoded\":{\"Ok\":{\"power_w\":0}}}\n{\"event\":\"indoor_bike_data\",\"sequence\":2,\"elapsed_ms\":40,\"decoded\":{\"Ok\":{\"power_w\":null}}}\n";
    let c = Capture::parse(raw).unwrap();
    assert_eq!(c.samples[0].watts, Some(0.));
    assert_eq!(c.samples[1].watts, None);
    assert_eq!(c.samples[0].at_ms, 0);
    assert!(Capture::parse(&raw[..raw.len() - 1]).is_err());
}

fn flat_replay(capture: Capture) -> Replay {
    Replay::new(
        Simulation::new(vec![(0., 0.), (10000., 0.)], Config::default()).unwrap(),
        capture,
    )
    .unwrap()
}
#[test]
fn timestamps_are_partition_independent_and_capture_end_holds() {
    let mut a = flat_replay(capture());
    let mut b = flat_replay(capture());
    a.resume();
    b.resume();
    for _ in 0..20 {
        a.advance(100);
    }
    for _ in 0..100 {
        b.advance(20);
    }
    assert_eq!(a.snapshot().distance_m, b.snapshot().distance_m);
    let mut c = capture();
    c.samples.truncate(2);
    c.samples[1].at_ms = 1007;
    let mut r = flat_replay(c);
    r.resume();
    for _ in 0..60 {
        r.advance(20);
    }
    assert_eq!(r.snapshot().status, "replay_complete");
    assert_eq!(r.snapshot().replay_ms, 1007);
    let held = r.snapshot().distance_m;
    r.resume();
    r.advance(100);
    assert_eq!(held, r.snapshot().distance_m);
}
#[test]
fn invalid_samples_and_worker_gap_are_rejected_safely() {
    let mut c = capture();
    c.samples.clear();
    assert!(ReplayWorker::start(vec![(0., 0.), (100., 0.)], Config::default(), c).is_err());
    let mut r = flat_replay(capture());
    r.resume();
    r.advance(251);
    assert_eq!(r.snapshot().status, "worker_timing_gap");
    assert_eq!(r.snapshot().distance_m, 0.);
    r.advance(20);
    assert_eq!(r.snapshot().replay_ms, 0);
    r.resume();
    r.advance(20);
    assert!(r.snapshot().distance_m > 0.);
    let mut c = capture();
    c.samples[0].watts = None;
    let mut r = flat_replay(c);
    r.resume();
    r.advance(20);
    assert_eq!(r.snapshot().status, "power_missing");
    assert_eq!(r.snapshot().distance_m, 0.);
}
#[test]
#[ignore = "Requires OPENRIDE_REPLAY_CAPTURE and OPENRIDE_REPLAY_PROFILE private paths"]
fn private_capture_runs_to_completion_on_road_profile() {
    let bytes = std::fs::read(std::env::var("OPENRIDE_REPLAY_CAPTURE").unwrap()).unwrap();
    let profile: serde_json::Value = serde_json::from_slice(
        &std::fs::read(std::env::var("OPENRIDE_REPLAY_PROFILE").unwrap()).unwrap(),
    )
    .unwrap();
    let points = profile["points"].as_array().unwrap();
    let points = points
        .iter()
        .map(|p| {
            (
                p["distance_m"].as_f64().unwrap(),
                p["elevation_m"].as_f64().unwrap(),
            )
        })
        .collect();
    let mut replay = Replay::new(
        Simulation::new(points, Config::default()).unwrap(),
        Capture::parse(&bytes).unwrap(),
    )
    .unwrap();
    replay.resume();
    while replay.snapshot().playing {
        replay.advance(20);
    }
    let state = replay.snapshot();
    assert_eq!(state.status, "replay_complete");
    assert!(state.distance_m > 0.);
    println!("PRIVATE_REPLAY {}", serde_json::to_string(&state).unwrap());
}
