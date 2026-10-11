//! Live simulation owned by the controller process, never by the renderer.
use crate::simulation::{Config, Simulation};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
pub struct LiveRide {
    sim: Simulation,
    last_ms: u64,
    power: Option<(u64, Option<f64>)>,
    source: String,
}
impl LiveRide {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > 2 * 1024 * 1024 {
            return Err("Route profile exceeds 2 MiB".into());
        }
        let value: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        let points = value["points"]
            .as_array()
            .ok_or("Missing route profile points")?;
        let profile = points
            .iter()
            .enumerate()
            .map(|(i, p)| {
                if i > 0 && p["break_before"] == true {
                    return Err("Route has unresolved breaks".into());
                }
                Ok((
                    p["distance_m"].as_f64().ok_or("Missing distance")?,
                    p["elevation_m"].as_f64().ok_or("Missing elevation")?,
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Self {
            sim: Simulation::new(profile, Config::default())?,
            last_ms: 0,
            power: None,
            source: format!("{:x}", Sha256::digest(bytes)),
        })
    }
    pub fn telemetry(&mut self, received_ms: u64, power: Option<f64>) {
        if self.power.is_none_or(|(t, _)| received_ms >= t) {
            self.power = Some((received_ms, power));
        }
    }
    pub fn fresh_power(&self, now: u64) -> Option<f64> {
        self.power
            .filter(|(t, _)| now >= *t && now - *t < 3000)
            .and_then(|(_, p)| p)
            .filter(|w| w.is_finite() && (0. ..=3000.).contains(w))
    }
    pub fn resume(&mut self, now: u64) -> bool {
        if self.fresh_power(now).is_none() || self.sim.status() == "finished" {
            return false;
        }
        self.last_ms = now;
        self.sim.resume();
        true
    }
    pub fn pause(&mut self, now: u64) {
        self.sim.pause();
        self.last_ms = now;
    }
    pub fn tick(&mut self, now: u64) {
        let elapsed = now.saturating_sub(self.last_ms);
        self.last_ms = now;
        let _ = self.sim.advance(elapsed, self.fresh_power(now));
    }
    pub fn status(&self) -> &str {
        self.sim.status()
    }
    pub fn grade_percent(&self) -> f64 {
        self.sim.grade_percent()
    }
    pub fn snapshot(&self) -> Value {
        json!({"event":"virtual_state","distance_m":self.sim.distance_m(),"speed_kph":self.sim.speed_mps()*3.6,"course_grade_percent":self.sim.grade_percent(),"simulation_state":self.sim.status(),"power_w":self.fresh_power(self.last_ms),"power_received_ms":self.power.map(|(t,_)|t),"route_sha256":self.source})
    }
    pub fn metadata(&self) -> Value {
        let c = Config::default();
        json!({"event":"route_session_config","schema":1,"route_sha256":self.source,"total_mass_kg":c.total_mass_kg,"rolling_coefficient":c.rolling_coefficient,"drag_area_m2":c.drag_area_m2,"air_density":c.air_density,"drivetrain_efficiency":c.drivetrain_efficiency,"grade_min_percent":0,"grade_max_percent":2,"grade_step_max_percent":1,"grade_interval_ms":1000,"virtual_positions":true})
    }
}
