//! Diagnostic tool: discovers paired BLE devices and reads a named device.
//! Read-only by default. Explicit modes test acquisition or bounded start/pause.
#[cfg(windows)]
mod ant_hr;
#[cfg(windows)]
mod ant_probe;
#[cfg(windows)]
mod ant_usb;
#[cfg(windows)]
mod ble;
#[cfg(windows)]
mod ble_sensor;
#[cfg(windows)]
mod continuous;
#[cfg(windows)]
mod control;
mod discovery_args;
mod journal;
mod ui_control;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Err(message) = discovery_args::validate(&args) {
        eprintln!("{message}");
        std::process::exit(1);
    }
    if let Err(message) = selected_pairing(&args) {
        eprintln!("{message}");
        std::process::exit(1);
    }
    #[cfg(windows)]
    if args.iter().any(|a| a.starts_with("--ant-journal=")) {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(ant_probe::run(&args));
        });
        match rx.recv_timeout(std::time::Duration::from_secs(180)) {
            Ok(Ok(())) => (),
            Ok(Err(e)) => {
                eprintln!("ANT probe failed: {e}");
                std::process::exit(1);
            }
            Err(_) => {
                eprintln!("ANT watchdog expired; adapter cleanup unconfirmed");
                std::process::exit(1);
            }
        }
        return;
    }
    if args.get(1).is_some_and(|s| s == "--recover-session") {
        let result = (|| -> Result<(), String> {
            let path = args
                .get(2)
                .ok_or("Usage: device-probe --recover-session JOURNAL")?;
            if std::fs::metadata(path).map_err(|e| e.to_string())?.len() > 64 * 1024 * 1024 {
                return Err("Journal exceeds diagnostic 64 MiB limit".into());
            }
            let (rows, partial) =
                journal::recover(&std::fs::read(path).map_err(|e| e.to_string())?)?;
            for row in &rows {
                println!("{row}");
            }
            println!(
                "{}",
                serde_json::json!({"event":"recovery_complete","records":rows.len(),"discarded_partial_line":partial})
            );
            Ok(())
        })();
        if let Err(e) = result {
            eprintln!("Recovery failed: {e}");
            std::process::exit(1);
        }
        return;
    }
    if args.get(1).is_some_and(|s| s == "--replay") {
        if let Err(e) = replay(args.get(2).map(String::as_str)) {
            eprintln!("Replay failed: {e}");
            std::process::exit(1);
        }
        return;
    }
    #[cfg(windows)]
    {
        let live = args.iter().any(|a| a.starts_with("--live-session="));
        if live && !args.iter().any(|a| a == "--ui-control") {
            eprintln!("Persistent sessions require the owned UI control pipe");
            std::process::exit(1);
        }
        if args.iter().any(|a| a == "--ui-control") {
            std::thread::spawn(|| {
                use std::io::BufRead;
                for line in std::io::stdin().lock().lines() {
                    match line {
                        Ok(line) => {
                            if line == "pause" {
                                continuous::request_pause();
                            }
                            ui_control::INBOX.lock().unwrap().receive(&line);
                        }
                        Err(_) => break,
                    }
                }
                // Parent exit or a closed control pipe must never silently resume.
                continuous::request_pause();
                ui_control::INBOX
                    .lock()
                    .unwrap()
                    .close("control_pipe_closed");
            });
        }
        // A stalled Windows GATT operation must not hang a diagnostic forever.
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(ble::run());
        });
        let timeout_s = if live {
            45 * 60
        } else if args.iter().any(|a| a.starts_with("--pedal-check=")) {
            180
        } else {
            90
        };
        let since = std::time::Instant::now();
        let mut progress = std::time::Instant::now();
        let mut heartbeat = 0;
        let result = loop {
            match rx.recv_timeout(std::time::Duration::from_secs(1)) {
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    let now = continuous::HEARTBEAT.load(std::sync::atomic::Ordering::Relaxed);
                    if now != heartbeat {
                        heartbeat = now;
                        progress = std::time::Instant::now();
                    }
                    if since.elapsed().as_secs() >= timeout_s
                        || (live && progress.elapsed().as_secs() >= 90)
                    {
                        break Err(());
                    }
                }
                Ok(result) => break Ok(result),
                Err(_) => break Err(()),
            }
        };
        match result {
            Ok(Ok(())) => println!("{}", serde_json::json!({"event":"probe_complete"})),
            Ok(Err(e)) => {
                println!(
                    "{}",
                    serde_json::json!({"event":"probe_error", "message":e.to_string(), "hresult":e.code().0})
                );
                if e.code().0 == 0 {
                    eprintln!(
                        "Probe failed: Windows returned a null device/result without an error code. Check trainer power, pairing, and access from this Windows session."
                    );
                } else {
                    eprintln!("Probe failed: {e}");
                }
                std::process::exit(1);
            }
            Err(_) => {
                println!(
                    "{}",
                    serde_json::json!({"event":"probe_error", "message":format!("Controller watchdog: {timeout_s}s total limit or worker stalled; terminating, trainer state unconfirmed")})
                );
                eprintln!(
                    "Controller exceeded its time/progress watchdog; trainer state unconfirmed. Check capture for command outcomes."
                );
                std::process::exit(1);
            }
        }
    }
    #[cfg(not(windows))]
    {
        eprintln!("The current probe requires Windows");
        std::process::exit(1);
    }
}

fn selected_pairing(args: &[String]) -> Result<Option<trainer_core::pairing::Pairing>, String> {
    let selections: Vec<_> = args
        .iter()
        .filter_map(|a| a.strip_prefix("--pairing="))
        .collect();
    let ids = args.iter().filter(|a| a.as_str() == "--device-id").count();
    if selections.is_empty() && ids == 0 {
        return Ok(None);
    }
    if selections.len() != 1 || ids != 1 || args.get(1).map(String::as_str) != Some("--device-id") {
        return Err(
            "Use one --device-id followed by its Windows identity and one --pairing snapshot"
                .into(),
        );
    }
    let pairing = trainer_core::pairing::Pairing::parse(selections[0])?;
    if args.get(2) != Some(&pairing.resistance.id) {
        return Err("Device identity does not match selected resistance source".into());
    }
    Ok(Some(pairing))
}

fn replay(path: Option<&str>) -> Result<(), String> {
    use std::io::BufRead;
    let file = std::fs::File::open(path.ok_or("Usage: device-probe --replay CAPTURE_JSONL")?)
        .map_err(|e| e.to_string())?;
    let mut packets = 0;
    for (index, line) in std::io::BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|e| e.to_string())?;
        let row: serde_json::Value = serde_json::from_str(line.trim_start_matches('\u{feff}'))
            .map_err(|e| format!("Line {}: {e}", index + 1))?;
        let bike = row["event"] == "indoor_bike_data";
        let feature = row["event"] == "read"
            && row["uuid"]
                .as_str()
                .is_some_and(|s| s.eq_ignore_ascii_case("00002ACC-0000-1000-8000-00805F9B34FB"));
        if !bike && !feature {
            continue;
        }
        let data: Vec<u8> =
            serde_json::from_value(row["bytes"].clone()).map_err(|e| e.to_string())?;
        let decoded = if bike {
            packets += 1;
            serde_json::to_value(trainer_core::ftms::indoor_bike(&data)?)
                .map_err(|e| e.to_string())?
        } else {
            serde_json::to_value(trainer_core::ftms::features(&data)?).map_err(|e| e.to_string())?
        };
        println!("{}", serde_json::json!({"line":index+1, "decoded":decoded}));
    }
    if packets == 0 {
        return Err("Capture contains no Indoor Bike Data notifications".into());
    }
    println!(
        "{}",
        serde_json::json!({"event":"replay_complete", "packets":packets})
    );
    Ok(())
}

#[cfg(test)]
mod pairing_tests {
    use super::selected_pairing;
    #[test]
    fn explicit_identity_requires_matching_single_snapshot() {
        let d = serde_json::json!({"id":"second-device", "name":"Duplicate name"});
        let raw = format!(
            "--pairing={}",
            serde_json::json!({"schema":1,"resistance":d,"power":d,"cadence":null})
        );
        let mut args = vec![
            "worker".into(),
            "--device-id".into(),
            "second-device".into(),
            raw.clone(),
        ];
        assert_eq!(
            selected_pairing(&args).unwrap().unwrap().resistance.id,
            "second-device"
        );
        args[2] = "first-device".into();
        assert!(selected_pairing(&args).is_err());
        args[2] = "second-device".into();
        args.push(raw);
        assert!(selected_pairing(&args).is_err());
        args.truncate(3);
        assert!(selected_pairing(&args).is_err());
    }
    #[test]
    fn external_power_dropout_holds_route_and_does_not_resume_on_recovery() {
        use trainer_core::ble_sensors::{Selected,Kind,Values};
        let p=trainer_core::pairing::Pairing::parse(r#"{"schema":2,"resistance":{"id":"trainer","name":"T"},"power":{"id":"meter","name":"P","protocol":"cycling_power"},"cadence":null}"#).unwrap();
        let mut sources=Selected::new(p);
        let mut ride=ride_core::live::LiveRide::parse(br#"{"points":[{"distance_m":0,"elevation_m":0},{"distance_m":1000,"elevation_m":0}]}"#).unwrap();
        sources.sensor("meter",Kind::CyclingPower,Values{power_w:Some(200),..Default::default()},0);
        let sample=sources.sample(0); ride.telemetry(sample.power_received_ms.unwrap(),sample.values.power_w.map(f64::from));
        assert!(ride.resume(0)); ride.tick(1000);
        sources.trainer(&trainer_core::ftms::BikeData{power_w:Some(900),..Default::default()},3001);
        let sample=sources.sample(3001); ride.telemetry(sample.power_received_ms.unwrap(),sample.values.power_w.map(f64::from)); ride.tick(3001);
        assert_ne!(ride.status(),"running"); let stopped=ride.snapshot()["distance_m"].clone();
        sources.sensor("meter",Kind::CyclingPower,Values{power_w:Some(200),..Default::default()},3100);
        let sample=sources.sample(3100); ride.telemetry(sample.power_received_ms.unwrap(),sample.values.power_w.map(f64::from)); ride.tick(3200);
        assert_ne!(ride.status(),"running"); assert_eq!(ride.snapshot()["distance_m"],stopped);
    }
}
