//! Timestamped power replay, with a single latest snapshot and a bounded worker command queue.
use crate::simulation::{Config, Simulation};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
#[derive(Clone, Debug)]
pub struct PowerSample {
    pub at_ms: u64,
    pub watts: Option<f64>,
}
pub struct Capture {
    pub samples: Vec<PowerSample>,
    pub sha256: String,
}
impl Capture {
    pub fn parse(raw: &[u8]) -> Result<Self, String> {
        if raw.len() > 16 * 1024 * 1024 || !raw.ends_with(b"\n") {
            return Err("Replay requires a complete JSONL capture <=16 MiB".into());
        }
        let mut samples = Vec::new();
        let mut previous = 0;
        for (index, line) in raw
            .split(|b| *b == b'\n')
            .filter(|l| !l.is_empty())
            .enumerate()
        {
            if index >= 100000 {
                return Err("Capture row limit exceeded".into());
            }
            let row: Value = serde_json::from_slice(line).map_err(|e| e.to_string())?;
            let at = row["elapsed_ms"]
                .as_u64()
                .ok_or("Missing capture timestamp")?;
            if row["sequence"].as_u64() != Some(index as u64) || at < previous {
                return Err("Invalid capture sequence/time".into());
            }
            previous = at;
            if index == 0 && (row["event"] != "session_check_created" || row["schema"] != 1) {
                return Err("Unsupported capture header".into());
            }
            if row["event"] == "indoor_bike_data" {
                let value = &row["decoded"]["Ok"]["power_w"];
                let watts = if value.is_null() {
                    None
                } else {
                    Some(value.as_f64().ok_or("Invalid power measurement")?)
                };
                if watts.is_some_and(|v| !v.is_finite() || !(0. ..=3000.).contains(&v)) {
                    return Err("Power outside replay bounds".into());
                }
                samples.push(PowerSample { at_ms: at, watts });
            }
        }
        if samples.len() < 2 {
            return Err("Replay needs at least two power notifications".into());
        }
        let origin = samples[0].at_ms;
        for s in &mut samples {
            s.at_ms -= origin;
        }
        if samples.last().unwrap().at_ms > 24 * 3600 * 1000 {
            return Err("Replay exceeds 24 hours".into());
        }
        Ok(Self {
            samples,
            sha256: format!("{:x}", Sha256::digest(raw)),
        })
    }
}
#[derive(Clone, Serialize, Default, Debug)]
pub struct Snapshot {
    pub playing: bool,
    pub replay_ms: u64,
    pub duration_ms: u64,
    pub distance_m: f64,
    pub speed_kph: f64,
    pub grade_percent: f64,
    pub power_w: Option<f64>,
    pub power_age_ms: u64,
    pub status: String,
    pub source_sha256: String,
}
pub struct Replay {
    simulation: Simulation,
    capture: Capture,
    index: usize,
    clock: u64,
    pending: u64,
    playing: bool,
    reason: String,
}
impl Replay {
    pub fn new(simulation: Simulation, capture: Capture) -> Result<Self, String> {
        if capture.samples.len() < 2
            || capture.samples[0].at_ms != 0
            || capture.samples.last().unwrap().at_ms == 0
            || capture.samples.last().unwrap().at_ms > 86400000
            || capture.samples.windows(2).any(|s| s[0].at_ms > s[1].at_ms)
            || capture.samples.iter().any(|s| {
                s.watts
                    .is_some_and(|w| !w.is_finite() || !(0. ..=3000.).contains(&w))
            })
        {
            return Err("Invalid replay samples".into());
        }
        Ok(Self {
            simulation,
            capture,
            index: 0,
            clock: 0,
            pending: 0,
            playing: false,
            reason: "paused".into(),
        })
    }
    fn power(&self) -> Option<f64> {
        if self.clock - self.capture.samples[self.index].at_ms >= 3000 {
            None
        } else {
            self.capture.samples[self.index].watts
        }
    }
    pub fn resume(&mut self) {
        if self.clock >= self.capture.samples.last().unwrap().at_ms
            || self.simulation.status() == "finished"
        {
            return;
        }
        self.playing = true;
        if self.power().is_some() {
            self.simulation.resume();
            self.reason = "running".into();
        } else {
            self.reason = "power_missing".into();
        }
    }
    pub fn pause(&mut self) {
        self.playing = false;
        self.pending = 0;
        self.simulation.pause();
        self.reason = "paused".into();
    }
    pub fn advance(&mut self, elapsed_ms: u64) {
        if !self.playing {
            return;
        }
        if elapsed_ms > 250 {
            self.pause();
            self.reason = "worker_timing_gap".into();
            return;
        }
        self.pending += elapsed_ms;
        while self.pending >= 20 && self.playing {
            self.pending -= 20;
            while self.index + 1 < self.capture.samples.len()
                && self.capture.samples[self.index + 1].at_ms <= self.clock
            {
                self.index += 1;
            }
            let power = self.power();
            if power.is_none() {
                self.simulation.pause();
                self.reason = if self.clock - self.capture.samples[self.index].at_ms >= 3000 {
                    "power_stale"
                } else {
                    "power_missing"
                }
                .into();
            }
            if self.reason == "running" {
                if self
                    .simulation
                    .advance(
                        20.min(self.capture.samples.last().unwrap().at_ms - self.clock),
                        power,
                    )
                    .is_err()
                {
                    self.pause();
                    self.reason = "invalid_power".into();
                    return;
                }
                if self.simulation.status() == "finished" {
                    self.playing = false;
                    self.reason = "route_complete".into();
                }
            }
            self.clock = (self.clock + 20).min(self.capture.samples.last().unwrap().at_ms);
            while self.index + 1 < self.capture.samples.len()
                && self.capture.samples[self.index + 1].at_ms <= self.clock
            {
                self.index += 1;
            }
            if self.clock >= self.capture.samples.last().unwrap().at_ms
                && self.reason != "route_complete"
            {
                self.simulation.pause();
                self.playing = false;
                self.reason = "replay_complete".into();
            }
        }
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            playing: self.playing,
            replay_ms: self.clock,
            duration_ms: self.capture.samples.last().unwrap().at_ms,
            distance_m: self.simulation.distance_m(),
            speed_kph: self.simulation.speed_mps() * 3.6,
            grade_percent: self.simulation.grade_percent(),
            power_w: self.power(),
            power_age_ms: self.clock - self.capture.samples[self.index].at_ms,
            status: self.reason.clone(),
            source_sha256: self.capture.sha256.clone(),
        }
    }
}
pub enum Command {
    Resume,
    Pause,
}
pub struct ReplayWorker {
    commands: SyncSender<Command>,
    state: Arc<Mutex<Snapshot>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl ReplayWorker {
    pub fn start(
        profile: Vec<(f64, f64)>,
        config: Config,
        capture: Capture,
    ) -> Result<Self, String> {
        let mut replay = Replay::new(Simulation::new(profile, config)?, capture)?;
        let state = Arc::new(Mutex::new(replay.snapshot()));
        let output = state.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let (commands, rx) = mpsc::sync_channel(16);
        let thread = thread::spawn(move || {
            let mut previous = Instant::now();
            let mut remainder = Duration::ZERO;
            while !stopped.load(Ordering::Relaxed) {
                let command = rx.recv_timeout(Duration::from_millis(10));
                let now = Instant::now();
                remainder += now.duration_since(previous);
                previous = now;
                let elapsed = remainder.as_millis() as u64;
                remainder -= Duration::from_millis(elapsed);
                replay.advance(elapsed);
                match command {
                    Ok(Command::Resume) => replay.resume(),
                    Ok(Command::Pause) => replay.pause(),
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    _ => {}
                }
                *output.lock().unwrap() = replay.snapshot();
            }
        });
        Ok(Self {
            commands,
            state,
            stop,
            thread: Some(thread),
        })
    }
    pub fn command(&self, command: Command) -> bool {
        self.commands.try_send(command).is_ok()
    }
    pub fn snapshot(&self) -> Snapshot {
        self.state.lock().unwrap().clone()
    }
}
impl Drop for ReplayWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
