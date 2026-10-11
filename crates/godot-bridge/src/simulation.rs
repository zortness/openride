//! Prototype adapter for manual power demonstration. No device I/O or recording.
use godot::{classes::RefCounted, prelude::*};
use ride_core::simulation::{Config, Simulation};
use serde_json::{json, Value};
#[derive(GodotClass)]
#[class(base=RefCounted,init)]
struct RideSimulation {
    base: Base<RefCounted>,
    simulation: Option<Simulation>,
}
#[godot_api]
impl RideSimulation {
    #[func]
    fn configure(&mut self, profile_json: GString) -> GString {
        self.simulation = None;
        let result = (|| -> Result<Simulation, String> {
            let raw = profile_json.to_string();
            if raw.len() > 2 * 1024 * 1024 {
                return Err("Profile exceeds 2 MiB".into());
            }
            let value: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
            let points = value["points"].as_array().ok_or("Missing profile points")?;
            let profile = points
                .iter()
                .map(|p| {
                    Ok((
                        p["distance_m"].as_f64().ok_or("Missing distance")?,
                        p["elevation_m"].as_f64().ok_or("Missing elevation")?,
                    ))
                })
                .collect::<Result<Vec<_>, String>>()?;
            Simulation::new(profile, Config::default())
        })();
        match result {
            Ok(sim) => {
                self.simulation = Some(sim);
                GString::new()
            }
            Err(error) => error.as_str().into(),
        }
    }
    #[func]
    fn command(&mut self, command: GString) {
        if let Some(sim) = &mut self.simulation {
            match command.to_string().as_str() {
                "resume" => sim.resume(),
                "pause" => sim.pause(),
                "reset" => sim.reset(),
                _ => {}
            }
        }
    }
    #[func]
    fn advance(&mut self, elapsed_ms: i64, power_w: f64, has_power: bool) -> GString {
        let Some(sim) = &mut self.simulation else {
            return "{\"error\":\"Simulation not configured\"}".into();
        };
        if elapsed_ms < 0 {
            sim.pause();
            return "{\"error\":\"Invalid elapsed time\"}".into();
        }
        if let Err(error) = sim.advance(elapsed_ms as u64, has_power.then_some(power_w)) {
            return json!({"error":error}).to_string().as_str().into();
        }
        json!({"distance_m":sim.distance_m(),"speed_kph":sim.speed_mps()*3.6,"grade_percent":sim.grade_percent(),"status":sim.status()}).to_string().as_str().into()
    }
}

/// Capture replay owns its clock on a Rust worker; Godot only polls snapshots.
#[derive(GodotClass)]
#[class(base=RefCounted,init)]
struct RideReplay {
    base: Base<RefCounted>,
    worker: Option<ride_core::replay::ReplayWorker>,
}
#[godot_api]
impl RideReplay {
    #[func]
    fn configure(&mut self, profile_json: GString, path: GString) -> GString {
        self.worker = None;
        let result = (|| -> Result<ride_core::replay::ReplayWorker, String> {
            use std::io::Read;
            let raw = profile_json.to_string();
            if raw.len() > 2 * 1024 * 1024 {
                return Err("Profile exceeds 2 MiB".into());
            }
            let value: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
            let profile = value["points"]
                .as_array()
                .ok_or("Missing points")?
                .iter()
                .map(|p| {
                    Ok((
                        p["distance_m"].as_f64().ok_or("Missing distance")?,
                        p["elevation_m"].as_f64().ok_or("Missing elevation")?,
                    ))
                })
                .collect::<Result<Vec<_>, String>>()?;
            let file = std::fs::File::open(path.to_string()).map_err(|e| e.to_string())?;
            let mut bytes = Vec::new();
            file.take(16 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            let capture = ride_core::replay::Capture::parse(&bytes)?;
            ride_core::replay::ReplayWorker::start(profile, Config::default(), capture)
        })();
        match result {
            Ok(worker) => {
                self.worker = Some(worker);
                GString::new()
            }
            Err(e) => e.as_str().into(),
        }
    }
    #[func]
    fn command(&self, action: GString) -> bool {
        use ride_core::replay::Command;
        let command = match action.to_string().as_str() {
            "resume" => Command::Resume,
            "pause" => Command::Pause,
            _ => return false,
        };
        self.worker.as_ref().is_some_and(|w| w.command(command))
    }
    #[func]
    fn snapshot(&self) -> GString {
        self.worker
            .as_ref()
            .map(|w| serde_json::to_string(&w.snapshot()).unwrap())
            .unwrap_or_else(|| "{}".into())
            .as_str()
            .into()
    }
}
