use activity_core::{export, Activity};
use serde_json::{json, Value};
use std::collections::BTreeMap;
fn journal(mut rows: Vec<Value>) -> Vec<u8> {
    rows.insert(0,json!({"event":"session_check_created","schema":1,"elapsed_ms":0,"utc_unix_ms":1700000000000u64}));
    let mut out = vec![];
    for (i, mut r) in rows.into_iter().enumerate() {
        r["sequence"] = json!(i);
        out.extend(serde_json::to_vec(&r).unwrap());
        out.push(b'\n');
    }
    out
}
fn ack(ms: u64, op: u8) -> Value {
    json!({"event":"command_acknowledged","elapsed_ms":ms,"opcode":op})
}
fn sample(ms: u64, power: Option<i16>, cadence: Option<f64>) -> Value {
    json!({"event":"indoor_bike_data","elapsed_ms":ms,"received_ms":ms,"decoded":{"Ok":{"power_w":power,"cadence_rpm":cadence,"speed_kmh":36.0,"heart_rate_bpm":null,"distance_m":900,"elapsed_s":200}}})
}
fn complete(ms: u64) -> Value {
    json!({"event":"session_check_complete","elapsed_ms":ms,"pause_acknowledged":true})
}
fn fixture() -> Vec<u8> {
    journal(vec![
        sample(500, Some(99), None),
        ack(1000, 7),
        sample(1100, Some(0), Some(82.5)),
        sample(2100, None, None),
        ack(3000, 8),
        sample(4000, Some(200), None),
        ack(5000, 7),
        sample(5100, Some(80), Some(90.0)),
        ack(6000, 8),
        sample(6500, Some(0), None),
        complete(7000),
    ])
}

// Structural test decoder deliberately reads definitions, rather than sharing writer layouts.
fn decode(bytes: &[u8]) -> Vec<(u16, BTreeMap<u8, u64>)> {
    assert_eq!(&bytes[8..12], b".FIT");
    let len = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    assert_eq!(bytes.len(), 14 + len + 2);
    let mut definitions: BTreeMap<u8, (u16, Vec<(u8, usize)>)> = BTreeMap::new();
    let mut rows = vec![];
    let mut offset = 14;
    while offset < 14 + len {
        let header = bytes[offset];
        offset += 1;
        if header & 0x40 != 0 {
            assert_eq!(bytes[offset + 1], 0);
            let global = u16::from_le_bytes(bytes[offset + 2..offset + 4].try_into().unwrap());
            let count = bytes[offset + 4] as usize;
            offset += 5;
            let mut fields = vec![];
            for _ in 0..count {
                fields.push((bytes[offset], bytes[offset + 1] as usize));
                offset += 3;
            }
            definitions.insert(header & 15, (global, fields));
        } else {
            let (global, fields) = &definitions[&(header & 15)];
            let mut row = BTreeMap::new();
            for &(field, size) in fields {
                let mut raw = [0; 8];
                raw[..size].copy_from_slice(&bytes[offset..offset + size]);
                offset += size;
                row.insert(field, u64::from_le_bytes(raw));
            }
            rows.push((*global, row));
        }
    }
    assert_eq!(offset, 14 + len);
    rows
}
#[test]
fn acknowledged_timer_excludes_pause_and_observation_but_csv_keeps_all() {
    let a = Activity::parse(&fixture()).unwrap();
    assert!(a.complete);
    assert_eq!(a.timer_ms(), 3000);
    assert_eq!(a.end_ms - a.start_ms, 5000);
    assert_eq!(a.timestamp(1100).unwrap(), "2023-11-14T22:13:21.100Z");
    let csv = a.csv().unwrap();
    assert_eq!(csv.lines().count(), 7);
    assert!(csv.contains(",1100,100,true,0,82.5,,36,900,200,"));
    assert!(csv.contains(",4000,3000,false,200,"));
    let (fit, count, _) = a.fit().unwrap();
    assert_eq!(count, 3);
    let rows = decode(&fit);
    let records: Vec<_> = rows.iter().filter(|r| r.0 == 20).collect();
    assert_eq!(records[0].1[&7], 0);
    assert_eq!(records[1].1[&7], 65535);
    assert_eq!(records[0].1[&3], 255);
    assert_eq!(records[0].1[&53], 64);
    assert_eq!(records[0].1[&6], 10000);
    let session = &rows.iter().find(|r| r.0 == 18).unwrap().1;
    assert_eq!(session[&7], 5000);
    assert_eq!(session[&8], 3000);
    assert_eq!(session[&5], 2);
    assert_eq!(session[&6], 6);
    assert_eq!(
        rows.iter()
            .filter(|r| r.0 == 21)
            .map(|r| r.1[&1])
            .collect::<Vec<_>>(),
        vec![0, 1, 0, 4]
    );
}
#[test]
fn recovery_keeps_last_running_sample_without_claiming_a_hardware_stop() {
    let mut bytes = journal(vec![
        ack(1000, 7),
        sample(1100, Some(30), None),
        sample(2100, Some(40), None),
    ]);
    bytes.extend(b"{\"event\":");
    let a = Activity::parse(&bytes).unwrap();
    assert!(!a.complete);
    assert!(a.discarded_partial_line);
    assert_eq!(a.timer_ms(), 1100);
    assert!(a.active(2100));
    assert_eq!(a.fit().unwrap().1, 2);
    bytes.push(b'\n');
    assert!(Activity::parse(&bytes).is_err());
}
#[test]
fn reject_corrupt_order_missing_start_and_overflow() {
    assert!(Activity::parse(&journal(vec![sample(5, Some(0), None)])).is_err());
    assert!(Activity::parse(&journal(vec![ack(100, 7), sample(90, Some(0), None)])).is_err());
    assert!(Activity::parse(&journal(vec![ack(1, 7), sample(u64::MAX, Some(0), None)])).is_err());
    let mut rows: Vec<Value> = String::from_utf8(fixture())
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    rows[2]["sequence"] = json!(999);
    let input = rows.iter().map(|r| format!("{r}\n")).collect::<String>();
    assert!(Activity::parse(input.as_bytes()).is_err());
}
#[test]
fn fit_quantization_and_unsigned_limits_are_reported_not_silently_clamped() {
    let a = Activity::parse(&journal(vec![
        ack(1000, 7),
        sample(1100, Some(2), None),
        sample(1500, Some(-4), Some(45.5)),
        ack(2100, 8),
        complete(2200),
    ]))
    .unwrap();
    let (fit, count, warnings) = a.fit().unwrap();
    assert_eq!(count, 1);
    let rows = decode(&fit);
    let record = &rows.iter().find(|r| r.0 == 20).unwrap().1;
    assert_eq!(record[&7], 65535);
    assert_eq!(record[&53], 64);
    assert!(warnings.iter().any(|s| s.contains("outside FIT")));
    assert!(warnings.iter().any(|s| s.contains("last sample")));
    assert!(a.csv().unwrap().contains(",-4,45.5,"));
}
#[test]
fn export_is_exclusive_and_does_not_change_source() {
    let root = std::env::temp_dir().join(format!(
        "openride-activity-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let source = root.join("session.jsonl");
    let output = root.join("export");
    let bytes = fixture();
    std::fs::write(&source, &bytes).unwrap();
    let report = export(&source, &output).unwrap();
    assert_eq!(report.timer_ms, 3000);
    assert!(output.join("report.json").exists());
    assert!(export(&source, &output).is_err());
    assert_eq!(std::fs::read(source).unwrap(), bytes);
    std::fs::remove_dir_all(root).unwrap();
}

fn virtual_fixture() -> Vec<u8> {
    journal(vec![
        json!({"event":"route_session_config","schema":1,"elapsed_ms":0,"route_sha256":"a".repeat(64),"package_manifest_sha256":"b".repeat(64)}),
        ack(1000, 7),
        sample(1100, Some(100), Some(80.)),
        virtual_row(1200, 2., "running"),
        virtual_row(2200, 8., "running"),
        virtual_row(2900, 12., "paused"),
        ack(3000, 8),
        sample(4000, Some(0), None),
        complete(5000),
    ])
}
fn virtual_row(ms: u64, d: f64, state: &str) -> Value {
    json!({"event":"virtual_state","elapsed_ms":ms,"distance_m":d,"speed_kph":18.,"course_grade_percent":3.,"acknowledged_grade_percent":2.,"simulation_state":state,"power_w":100.,"route_sha256":"a".repeat(64)})
}
#[test]
fn virtual_fit_uses_recorded_progress_not_trainer_counters() {
    let a = Activity::parse(&virtual_fixture()).unwrap();
    let (fit, _, _) = a.fit().unwrap();
    let rows = decode(&fit);
    let records: Vec<_> = rows.iter().filter(|r| r.0 == 20).collect();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].1[&5], 200);
    assert_eq!(records[1].1[&5], 800);
    assert_eq!(records[0].1[&73], 5000);
    assert_eq!(records[0].1[&7], 100);
    let session = &rows.iter().find(|r| r.0 == 18).unwrap().1;
    assert_eq!(session[&9], 1200);
    assert_eq!(session[&6], 58);
    assert_eq!(session[&8], 2000);
    assert!(a.csv().unwrap().contains(",36,900,200,"));
}

#[test]
fn virtual_recovery_rejects_reversed_distance_and_does_not_fill_stale_cadence() {
    let mut rows: Vec<Value> = String::from_utf8(virtual_fixture())
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let i = rows
        .iter()
        .position(|r| r["event"] == "virtual_state" && r["distance_m"] == 8.)
        .unwrap();
    rows[i]["distance_m"] = json!(1.);
    let raw = rows.iter().map(|r| format!("{r}\n")).collect::<String>();
    assert!(Activity::parse(raw.as_bytes()).is_err());
    let raw = journal(vec![
        json!({"event":"route_session_config","schema":1,"elapsed_ms":0,"route_sha256":"a".repeat(64),"package_manifest_sha256":"b".repeat(64)}),
        ack(1000, 7),
        sample(1100, Some(100), Some(80.)),
        virtual_row(1200, 2., "running"),
        virtual_row(4200, 12., "running"),
    ]);
    let a = Activity::parse(&raw).unwrap();
    assert!(!a.complete);
    assert_eq!(a.end_ms, 4200);
    let (fit, _, _) = a.fit().unwrap();
    let rows = decode(&fit);
    let last = &rows.iter().rfind(|r| r.0 == 20).unwrap().1;
    assert_eq!(last[&4], 255);
    assert_eq!(last[&5], 1200);
}

#[test]
fn virtual_export_requires_matching_profile_and_preserves_both_streams() {
    use sha2::{Digest, Sha256};
    let root = std::env::temp_dir().join(format!(
        "openride-virtual-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let source = root.join("session.jsonl");
    let output = root.join("export");
    std::fs::write(&source, virtual_fixture()).unwrap();
    std::fs::write(root.join("route-profile.json"), b"{}").unwrap();
    assert!(export(&source, &output)
        .unwrap_err()
        .contains("hash mismatch"));
    assert!(!output.exists());
    let bytes = String::from_utf8(virtual_fixture())
        .unwrap()
        .replace(&"a".repeat(64), &format!("{:x}", Sha256::digest(b"{}")));
    std::fs::write(&source, bytes.as_bytes()).unwrap();
    let report = export(&source, &output).unwrap();
    assert_eq!(report.csv_samples, 2);
    assert_eq!(report.virtual_samples, 3);
    assert_eq!(report.virtual_distance_m, Some(12.));
    assert_eq!(report.tcx_records, 2);
    assert_eq!(report.gpx_records, 0);
    for r in decode(&std::fs::read(output.join("activity.fit")).unwrap())
        .iter()
        .filter(|r| r.0 == 20)
    {
        assert_eq!(r.1[&0], i32::MAX as u64);
        assert_eq!(r.1[&1], i32::MAX as u64);
        assert_eq!(r.1[&2], u16::MAX as u64);
    }
    assert!(!output.join("activity.gpx").exists());
    assert!(output.join("virtual.csv").exists());
    assert_eq!(
        std::fs::read(output.join("route-profile.json")).unwrap(),
        b"{}"
    );
    assert_eq!(std::fs::read(&source).unwrap(), bytes.as_bytes());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn geographic_xml_exports_validate_origin_and_split_pauses() {
    use sha2::{Digest, Sha256};
    let root = std::env::temp_dir().join(format!(
        "openride-xml-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let source = root.join("session.jsonl");
    let profile = serde_json::to_vec(&json!({"points":[
        {"position":[0.,0.,0.],"distance_m":0.,"elevation_m":100.,"break_before":true},
        {"position":[20.,0.,0.],"distance_m":20.,"elevation_m":120.,"break_before":false}
    ]}))
    .unwrap();
    let manifest =
        serde_json::to_vec(&json!({"origin_lat_lon_elevation":[33.,-118.,100.]})).unwrap();
    let route_hash = format!("{:x}", Sha256::digest(&profile));
    let manifest_hash = format!("{:x}", Sha256::digest(&manifest));
    let raw = journal(vec![
        hr_config(),
        json!({"event":"route_session_config","schema":1,"elapsed_ms":0,"route_sha256":route_hash,"package_manifest_sha256":manifest_hash}),
        ack(1000, 7),
        sample(1100, Some(100), Some(80.)),
        hr(1150, Some(88)),
        virtual_row(1200, 2., "running"),
        ack(2000, 8),
        virtual_row(2500, 2., "paused"),
        ack(3000, 7),
        virtual_row(3200, 8., "running"),
        ack(4000, 8),
        complete(5000),
    ]);
    let raw = String::from_utf8(raw)
        .unwrap()
        .replace(&"a".repeat(64), &route_hash);
    std::fs::write(&source, &raw).unwrap();
    std::fs::write(root.join("route-profile.json"), &profile).unwrap();
    let mp = root.join("package-manifest.json");
    std::fs::write(&mp, b"{}").unwrap();
    assert!(export(&source, &root.join("bad"))
        .unwrap_err()
        .contains("manifest hash mismatch"));
    assert!(!root.join("bad").exists());
    std::fs::write(&mp, &manifest).unwrap();
    let report = export(&source, &root.join("good")).unwrap();
    assert_eq!(report.timer_ms, 2000);
    assert_eq!(report.elapsed_ms, 3000);
    assert_eq!(report.gpx_records, 2);
    let fit = std::fs::read(root.join("good/activity.fit")).unwrap();
    let decoded = decode(&fit);
    let records: Vec<_> = decoded.iter().filter(|r| r.0 == 20).collect();
    assert_eq!(records.len(), 2);
    assert!(records.iter().all(|r| r.1[&3] == 88));
    assert_eq!(report.hr_samples, 1);
    assert!(root.join("good/heart-rate.csv").exists());
    for (r, elevation) in records.iter().zip([102., 108.]) {
        let lat = r.1[&0] as u32 as i32 as f64 * 180. / 2147483648.;
        let lon = r.1[&1] as u32 as i32 as f64 * 180. / 2147483648.;
        assert!((lat - 33.).abs() < 0.000001);
        assert!((-118. ..-117.999).contains(&lon));
        assert_eq!(r.1[&2] as f64 / 5. - 500., elevation);
        assert_eq!(r.1[&78], r.1[&2]);
        assert_eq!(r.1[&7], 100);
        assert_eq!(r.1[&73], 5000);
    }
    for (file, segment, point) in [
        ("activity.tcx", "Track", "Trackpoint"),
        ("activity.gpx", "trkseg", "trkpt"),
    ] {
        let xml = std::fs::read_to_string(root.join("good").join(file)).unwrap();
        let mut reader = quick_xml::Reader::from_str(&xml);
        let (mut segments, mut points) = (0, 0);
        loop {
            match reader.read_event().unwrap() {
                quick_xml::events::Event::Start(e) => {
                    if e.name().as_ref() == segment.as_bytes() {
                        segments += 1;
                    }
                    if e.name().as_ref() == point.as_bytes() {
                        points += 1;
                    }
                }
                quick_xml::events::Event::Eof => break,
                _ => (),
            }
        }
        assert_eq!((segments, points), (2, 2));
        assert!(xml.contains("102.000000"));
        assert!(xml.contains("108.000000"));
        assert!(xml.contains("derived virtual positions"));
    }
    let tcx = std::fs::read_to_string(root.join("good/activity.tcx")).unwrap();
    assert!(tcx.contains("<Value>88</Value>"));
    let gpx = std::fs::read_to_string(root.join("good/activity.gpx")).unwrap();
    assert!(gpx.contains(":hr>88</"));
    assert!(tcx.contains("<Speed>5.000000</Speed>"));
    assert!(tcx.contains("<Watts>100</Watts>"));
    // Equivalent prefixes were not accepted in the user's GoldenCheetah 3.8 import.
    assert!(tcx.contains("<TPX xmlns=\"http://www.garmin.com/xmlschemas/ActivityExtension/v2\">"));
    assert!(!tcx.contains("<ae:Watts>"));
    assert!(tcx.contains("<DistanceMeters>8.000000</DistanceMeters>"));
    // A matching hash alone must not permit extrapolation beyond the saved route.
    std::fs::write(
        &source,
        raw.replace("\"distance_m\":8.0", "\"distance_m\":28.0"),
    )
    .unwrap();
    assert!(export(&source, &root.join("beyond"))
        .unwrap_err()
        .contains("outside saved route"));
    assert!(!root.join("beyond").exists());
    std::fs::remove_dir_all(root).unwrap();
}

fn hr_config() -> Value {
    json!({"event":"ant_hr_config","elapsed_ms":0,"schema":1,"device_number":42,"device_type":120,"transmission_type":241,"freshness_ms":3000})
}
fn hr(ms: u64, bpm: Option<u8>) -> Value {
    json!({"event":"ant_hr_data","elapsed_ms":ms,"received_ms":ms,"device_number":42,"device_type":120,"transmission_type":241,"heart_rate_bpm":bpm})
}
#[test]
fn selected_hr_expires_without_fallback_and_rejects_wrong_identity() {
    let config = json!({"event":"route_session_config","schema":1,"elapsed_ms":0,"route_sha256":"a".repeat(64),"package_manifest_sha256":"b".repeat(64)});
    let mut ble = sample(1100, Some(120), Some(80.));
    ble["decoded"]["Ok"]["heart_rate_bpm"] = json!(199);
    let rows = vec![
        hr_config(),
        config,
        ack(1000, 7),
        ble,
        virtual_row(1150, 1., "running"),
        hr(1200, Some(85)),
        virtual_row(1300, 2., "running"),
        virtual_row(4200, 3., "running"),
        hr(4300, Some(90)),
        virtual_row(4400, 4., "running"),
        hr(4500, None),
        virtual_row(4600, 5., "running"),
        ack(5000, 8),
        complete(5100),
    ];
    let a = Activity::parse(&journal(rows.clone())).unwrap();
    assert_eq!(
        a.fit_samples()
            .iter()
            .map(|s| s.data.heart_rate_bpm)
            .collect::<Vec<_>>(),
        vec![None, Some(85), None, Some(90), None]
    );
    assert_eq!(a.samples[0].data.heart_rate_bpm, Some(199)); // raw BLE provenance unchanged
    let mut wrong = rows.clone();
    wrong[5]["transmission_type"] = json!(1);
    assert!(Activity::parse(&journal(wrong))
        .unwrap_err()
        .contains("identity mismatch"));
    let mut future = rows.clone();
    future[5]["received_ms"] = json!(1300);
    assert!(Activity::parse(&journal(future)).is_err());
    let mut invalid = rows;
    invalid[5]["heart_rate_bpm"] = json!(255);
    assert!(Activity::parse(&journal(invalid)).is_err());
}
