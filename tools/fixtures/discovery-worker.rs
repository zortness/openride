//! Hardware-free discovery process fixture. Reject anything outside the read-only contract.
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::thread::sleep(std::time::Duration::from_millis(100));
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["--discover"] => {
            println!(r#"{{"event":"paired_device","name":"Synthetic sensor","id":"synthetic-one"}}"#);
            println!(r#"{{"event":"paired_device","name":"Synthetic sensor","id":"synthetic-two"}}"#);
        }
        ["--inspect-id", "synthetic-two"] => {
            println!(r#"{{"event":"device","name":"Synthetic sensor","id":"synthetic-two"}}"#);
            println!(r#"{{"event":"capabilities","id":"synthetic-two","roles":{{"power":true,"cadence":true,"heart_rate":false,"terrain_control":true}},"live_roles":{{"power":true,"cadence":true,"terrain_control":true}},"telemetry_verified":false,"control_verified":false}}"#);
        }
        ["--inspect-id", "failure"] => {
            eprintln!("Synthetic discovery failure");
            std::process::exit(1);
        }
        ["--inspect-id", "synthetic-sensor"] => {
            println!(r#"{{"event":"device","name":"Synthetic multi-sensor","id":"synthetic-sensor"}}"#);
            println!(r#"{{"event":"capabilities","id":"synthetic-sensor","roles":{{"power":true,"cadence":true,"heart_rate":true,"terrain_control":false}},"live_roles":{{}},"sensor_roles":{{"power":"cycling_power","cadence":"csc","heart_rate":"heart_rate"}},"telemetry_verified":false,"control_verified":false}}"#);
        }
        _ => {
            eprintln!("Unexpected arguments: {args:?}");
            std::process::exit(2);
        }
    }
    println!(r#"{{"event":"probe_complete"}}"#);
}
