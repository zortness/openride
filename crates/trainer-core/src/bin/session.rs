use std::io::Write;
use trainer_core::{recover, telemetry_csv, Event, Journal, Session};

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("demo") if args.len() == 3 => {
            let mut journal = Journal::create(&args[2])?;
            let mut session = Session::default();
            let mut events = vec![(0, Event::Connected), (10, Event::ControlGranted)];
            for second in 1..=20 {
                let ms = second * 1000;
                events.push((
                    ms,
                    Event::Telemetry {
                        power_w: if second == 8 {
                            None
                        } else if second == 9 {
                            Some(0)
                        } else {
                            Some(150)
                        },
                        cadence_rpm: Some(80.0),
                        heart_rate_bpm: None,
                    },
                ));
                if second == 1 {
                    events.push((ms, Event::Resume));
                }
                events.push((
                    ms,
                    Event::Grade {
                        percent: if second < 10 { 3.0 } else { -2.0 },
                    },
                ));
            }
            events.extend([
                (24000, Event::Tick),
                (25000, Event::Disconnected),
                (26000, Event::Connected),
                (26010, Event::ControlGranted),
            ]);
            for (ms, event) in events {
                journal.append(&session.apply(ms, event)?)?;
            }
            println!(
                "SIMULATED session recorded; final phase {:?}; no hardware accessed",
                session.phase()
            );
        }
        Some("recover") if args.len() == 4 => {
            let (records, truncated) =
                recover(&std::fs::read(&args[2]).map_err(|e| e.to_string())?)?;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&args[3])
                .map_err(|e| e.to_string())?;
            file.write_all(telemetry_csv(&records).as_bytes())
                .and_then(|_| file.sync_all())
                .map_err(|e| e.to_string())?;
            println!(
                "Recovered {} records; incomplete final line discarded: {truncated}",
                records.len()
            );
        }
        _ => {
            return Err("Usage: session demo NEW_JOURNAL | session recover JOURNAL NEW_CSV".into())
        }
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
