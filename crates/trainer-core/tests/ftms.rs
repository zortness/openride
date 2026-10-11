use trainer_core::ftms::*;

fn capabilities() -> Features {
    features(&[134, 80, 0, 0, 12, 224, 0, 0]).unwrap()
}
fn acquired() -> ControlPoint {
    let mut cp = ControlPoint::new(&capabilities());
    let (id, bytes) = cp.begin(0, Request::Acquire).unwrap();
    assert_eq!(bytes, [0]);
    assert_eq!(cp.write_complete(1, id, true).unwrap(), None);
    assert!(!cp.has_control());
    assert_eq!(
        cp.indication(2, &[0x80, 0, 1]).unwrap(),
        Some(Outcome::Accepted(Request::Acquire))
    );
    cp
}

#[test]
fn suito_capture_decodes_without_inventing_missing_heart_rate() {
    let f = capabilities();
    assert!(f.cadence && f.power && f.simulation && f.target_power);
    assert_eq!(f.machine_bits, 0x5086);
    assert_eq!(f.target_bits, 0xe00c);
    let d = indoor_bike(&[84, 8, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0]).unwrap();
    assert_eq!(d.speed_kmh, Some(0.0));
    assert_eq!(d.cadence_rpm, Some(0.0));
    assert_eq!(d.power_w, Some(0));
    assert_eq!(d.distance_m, Some(1));
    assert_eq!(d.elapsed_s, Some(0));
    assert_eq!(d.heart_rate_bpm, None);
}

#[test]
fn units_signed_power_and_partial_packets() {
    let d = indoor_bike(&[0x44, 2, 0xd2, 4, 181, 0, 0xf6, 0xff, 140]).unwrap();
    assert_eq!(d.speed_kmh, Some(12.34));
    assert_eq!(d.cadence_rpm, Some(90.5));
    assert_eq!(d.power_w, Some(-10));
    assert_eq!(d.heart_rate_bpm, Some(140));
    let partial = indoor_bike(&[0x41, 0, 100, 0]).unwrap();
    assert!(partial.more_data);
    assert_eq!(partial.speed_kmh, None);
    assert_eq!(partial.cadence_rpm, None);
    assert_eq!(partial.power_w, Some(100));
}

#[test]
fn malformed_packets_fail_without_panics_or_offset_guessing() {
    let packet = [84, 8, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    for n in 0..packet.len() {
        assert!(indoor_bike(&packet[..n]).is_err());
    }
    assert!(indoor_bike(&[0, 0, 0, 0, 1]).is_err());
    assert!(indoor_bike(&[0, 0x80, 0, 0]).is_err());
    assert!(indoor_bike(&[0x20, 0, 0, 0, 0]).is_err());
    assert!(features(&[0; 7]).is_err());
}

#[test]
fn encodes_simulation_and_pause_exactly() {
    let req = Request::Simulation {
        wind_mm_s: -1000,
        grade_hundredths: -250,
        crr: 40,
        cw: 51,
    };
    assert_eq!(req.encode(), [0x11, 0x18, 0xfc, 0x06, 0xff, 40, 51]);
    assert_eq!(Request::Pause.encode(), [8, 2]);
    assert_eq!(Request::Start.encode(), [7]);
}

#[test]
fn response_before_write_completion_is_not_prematurely_accepted() {
    let mut cp = ControlPoint::new(&capabilities());
    assert!(cp.begin(0, Request::Start).is_err());
    let (id, _) = cp.begin(1, Request::Acquire).unwrap();
    assert_eq!(cp.indication(2, &[0x80, 0, 1]).unwrap(), None);
    assert!(!cp.has_control());
    assert!(cp.begin(3, Request::Acquire).is_err());
    assert_eq!(
        cp.write_complete(4, id, true).unwrap(),
        Some(Outcome::Accepted(Request::Acquire))
    );
    assert!(cp.has_control());
}

#[test]
fn timeout_requires_new_connection_and_rejects_late_reply() {
    let mut cp = acquired();
    let (id, _) = cp.begin(10, Request::Start).unwrap();
    cp.write_complete(11, id, true).unwrap();
    assert!(cp.tick(5010).is_err());
    assert!(!cp.has_control());
    assert!(cp.indication(5011, &[0x80, 7, 1]).is_err());
    assert!(cp.begin(5012, Request::Acquire).is_err());
}

#[test]
fn faults_and_control_rejection_do_not_leave_control_granted() {
    for response in [&[0x80, 8, 1][..], &[0x80, 7][..], &[0x80, 7, 6][..]] {
        let mut cp = acquired();
        cp.begin(10, Request::Start).unwrap();
        assert!(cp.indication(11, response).is_err());
        assert!(!cp.has_control());
    }
    let mut cp = acquired();
    let (id, _) = cp.begin(10, Request::Start).unwrap();
    cp.write_complete(11, id, true).unwrap();
    assert_eq!(
        cp.indication(12, &[0x80, 7, 5]).unwrap(),
        Some(Outcome::Rejected {
            request: Request::Start,
            code: 5
        })
    );
    assert!(!cp.has_control());
    assert!(cp.begin(13, Request::Start).is_err());
    let (id, _) = cp.begin(14, Request::Acquire).unwrap();
    assert!(cp.write_complete(15, id, false).is_err());
    assert!(cp.begin(16, Request::Acquire).is_err());
}

#[test]
fn disconnect_and_unsupported_features_block_commands() {
    let mut cp = acquired();
    cp.invalidate();
    assert!(!cp.has_control());
    assert!(cp.begin(10, Request::Start).is_err());
    let mut cp = ControlPoint::new(&features(&[0; 8]).unwrap());
    let (id, _) = cp.begin(0, Request::Acquire).unwrap();
    cp.write_complete(1, id, true).unwrap();
    cp.indication(2, &[0x80, 0, 1]).unwrap();
    assert!(cp
        .begin(
            3,
            Request::Simulation {
                wind_mm_s: 0,
                grade_hundredths: 0,
                crr: 40,
                cw: 51
            }
        )
        .is_err());
}

#[test]
fn control_loss_cancels_pending_procedure_before_late_ack() {
    for status in [0xff, 0x01] {
        let mut cp = acquired();
        cp.begin(10, Request::Start).unwrap();
        cp.machine_status(&[status]).unwrap();
        assert!(!cp.has_control());
        assert!(cp.indication(11, &[0x80, 7, 1]).is_err());
    }
}
