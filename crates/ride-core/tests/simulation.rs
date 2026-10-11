use ride_core::simulation::{Config, Simulation};
#[test]
fn power_moves_the_rider_and_a_climb_reduces_speed() {
    fn ride(grade: f64) -> Simulation {
        let mut sim =
            Simulation::new(vec![(0., 0.), (10000., 10000. * grade)], Config::default()).unwrap();
        sim.resume();
        for _ in 0..3000 {
            sim.advance(20, Some(200.)).unwrap();
        }
        sim
    }
    let flat = ride(0.);
    let climb = ride(0.05);
    assert!(flat.distance_m() > 100.);
    assert!(flat.speed_mps() > climb.speed_mps());
    assert!(climb.speed_mps() > 0.);
}
#[test]
fn fixed_ticks_are_independent_of_call_partition_and_zero_means_coasting() {
    fn run(chunk: u64) -> Simulation {
        let mut s = Simulation::new(vec![(0., 0.), (10000., 0.)], Config::default()).unwrap();
        s.resume();
        for _ in 0..10000 / chunk {
            s.advance(chunk, Some(200.)).unwrap();
        }
        s
    }
    let mut a = run(20);
    let b = run(100);
    assert_eq!(a.distance_m(), b.distance_m());
    assert_eq!(a.speed_mps(), b.speed_mps());
    let before = a.speed_mps();
    let distance = a.distance_m();
    for _ in 0..50 {
        a.advance(20, Some(0.)).unwrap();
    }
    assert!(a.speed_mps() < before);
    assert!(a.distance_m() > distance);
    a.advance(20, None).unwrap();
    let held = a.distance_m();
    assert_eq!(a.status(), "power_missing");
    a.advance(100, Some(200.)).unwrap();
    assert_eq!(a.distance_m(), held);
    a.resume();
    a.advance(100, Some(200.)).unwrap();
    assert!(a.distance_m() > held);
}
#[test]
fn downhill_coasts_from_rest_and_route_end_and_timing_gaps_hold() {
    let mut s = Simulation::new(vec![(0., 0.), (10., -1.)], Config::default()).unwrap();
    s.resume();
    for _ in 0..500 {
        s.advance(20, Some(0.)).unwrap();
    }
    assert_eq!(s.distance_m(), 10.);
    assert_eq!(s.status(), "finished");
    assert_eq!(s.speed_mps(), 0.);
    s.reset();
    s.resume();
    s.advance(251, Some(200.)).unwrap();
    assert_eq!(s.status(), "timing_gap");
    assert_eq!(s.distance_m(), 0.);
    s.resume();
    assert!(s.advance(20, Some(f64::NAN)).is_err());
    assert_eq!(s.status(), "paused");
    assert!(Simulation::new(vec![(0., 0.), (0., 1.)], Config::default()).is_err());
}
