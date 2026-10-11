use trainer_core::*;

fn telemetry(power_w: Option<u16>) -> Event {
    Event::Telemetry {
        power_w,
        cadence_rpm: None,
        heart_rate_bpm: None,
    }
}

fn running() -> Session {
    let mut s = Session::default();
    s.apply(0, Event::Connected).unwrap();
    assert!(s.apply(0, Event::Resume).is_err());
    s.apply(0, Event::ControlGranted).unwrap();
    assert!(s.apply(0, Event::Resume).is_err());
    s.apply(0, telemetry(Some(150))).unwrap();
    s.apply(0, Event::Resume).unwrap();
    s
}

#[test]
fn reconnect_requires_fresh_telemetry_control_and_explicit_resume() {
    let mut s = running();
    s.apply(100, Event::Disconnected).unwrap();
    assert!(s.apply(100, Event::Grade { percent: 1.0 }).is_err());
    s.apply(200, Event::Connected).unwrap();
    s.apply(200, telemetry(Some(0))).unwrap();
    assert!(s.apply(200, Event::Resume).is_err());
    s.apply(200, Event::ControlGranted).unwrap();
    assert_eq!(s.phase(), Phase::Ready);
    assert_eq!(
        s.apply(200, Event::Resume).unwrap().commands,
        vec![Command::SetGrade { percent: 0.0 }]
    );
}

#[test]
fn stale_data_pauses_and_new_data_does_not_resume() {
    let mut s = running();
    assert!(s.apply(3001, Event::Grade { percent: 1.0 }).is_err());
    assert_eq!(
        s.apply(3001, Event::Tick).unwrap().commands,
        vec![Command::Pause]
    );
    s.apply(3100, telemetry(None)).unwrap();
    assert_eq!(s.phase(), Phase::Paused);
    s.apply(3100, Event::Resume).unwrap();
    assert!(s.apply(3099, Event::Tick).is_err());
}

#[test]
fn grades_are_bounded_and_rate_limited() {
    let mut s = running();
    for second in 1..=10 {
        s.apply(second * 1000, telemetry(Some(100))).unwrap();
        let r = s
            .apply(second * 1000, Event::Grade { percent: 20.0 })
            .unwrap();
        assert_eq!(
            r.commands,
            vec![Command::SetGrade {
                percent: (second as f64).min(5.0)
            }]
        );
    }
    assert!(s.apply(10000, Event::Grade { percent: f64::NAN }).is_err());
    assert_eq!(
        s.apply(10000, Event::Grade { percent: -5.0 })
            .unwrap()
            .commands,
        vec![Command::SetGrade { percent: 5.0 }]
    );
    s.apply(11000, Event::Pause).unwrap();
    assert!(s.apply(11000, Event::Grade { percent: 0.0 }).is_err());
}

#[test]
fn crash_tail_recovers_committed_samples_preserving_zero_and_missing() {
    let mut s = running();
    let records = [
        s.apply(1000, telemetry(None)).unwrap(),
        s.apply(2000, telemetry(Some(0))).unwrap(),
    ];
    let mut bytes = Vec::new();
    for r in &records {
        bytes.extend(serde_json::to_vec(r).unwrap());
        bytes.push(b'\n');
    }
    bytes.extend(b"{\"schema\":");
    let (recovered, tail) = recover(&bytes).unwrap();
    assert!(tail);
    assert_eq!(recovered.len(), 2);
    let csv = telemetry_csv(&recovered);
    assert!(csv.contains("1000,Running,,,\n"));
    assert!(csv.contains("2000,Running,0,,\n"));
    bytes.push(b'\n');
    assert!(recover(&bytes).is_err());
}

#[test]
fn journal_refuses_overwrite_and_syncs_readable_records() {
    let path = std::env::temp_dir().join(format!("openride-session-{}.jsonl", std::process::id()));
    let mut journal = Journal::create(&path).unwrap();
    assert!(Journal::create(&path).is_err());
    journal
        .append(&Session::default().apply(0, Event::Connected).unwrap())
        .unwrap();
    let (records, tail) = recover(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(records.len(), 1);
    assert!(!tail);
    drop(journal);
    std::fs::remove_file(path).unwrap();
}
