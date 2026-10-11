//! Transport-independent session policy and durable recording. No hardware I/O.
use serde::{Deserialize, Serialize};
use std::io::Write;
pub mod ant;
pub mod ble_sensors;
pub mod diagnostic;
pub mod discovery;
pub mod ftms;
pub mod live;
pub mod pairing;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Disconnected,
    Ready,
    Running,
    Paused,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Event {
    Connected,
    ControlGranted,
    Resume,
    Pause,
    Disconnected,
    Tick,
    Grade {
        percent: f64,
    },
    Telemetry {
        power_w: Option<u16>,
        cadence_rpm: Option<f64>,
        heart_rate_bpm: Option<u8>,
    },
}

/// Requests for a future transport adapter, never evidence of trainer acceptance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Command {
    RequestControl,
    SetGrade { percent: f64 },
    Pause,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub schema: u32,
    pub elapsed_ms: u64,
    pub event: Event,
    pub phase: Phase,
    pub commands: Vec<Command>,
}

pub struct Session {
    phase: Phase,
    control: bool,
    clock_ms: u64,
    telemetry_ms: Option<u64>,
    grade: f64,
    grade_ms: u64,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            phase: Phase::Disconnected,
            control: false,
            clock_ms: 0,
            telemetry_ms: None,
            grade: 0.0,
            grade_ms: 0,
        }
    }
}

impl Session {
    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// Monotonic timestamps are supplied by the owner thread. Invalid events do
    /// not advance state. Tick at least once a second to detect stale telemetry.
    pub fn apply(&mut self, ms: u64, event: Event) -> Result<Record, String> {
        if ms < self.clock_ms {
            return Err("Session clock moved backwards".into());
        }
        let mut commands = Vec::new();
        match &event {
            Event::Connected => {
                if self.phase != Phase::Disconnected {
                    return Err("Already connected".into());
                }
                self.phase = Phase::Ready;
                self.control = false;
                self.telemetry_ms = None;
                commands.push(Command::RequestControl);
            }
            Event::ControlGranted => {
                if self.phase == Phase::Disconnected {
                    return Err("Not connected".into());
                }
                self.control = true;
            }
            Event::Resume => {
                if !matches!(self.phase, Phase::Ready | Phase::Paused) || !self.control {
                    return Err(
                        "Resume requires a connected trainer and acknowledged control".into(),
                    );
                }
                if self.telemetry_ms.is_none_or(|t| ms - t > 3000) {
                    return Err("Resume requires fresh telemetry".into());
                }
                self.phase = Phase::Running;
                self.grade = 0.0;
                self.grade_ms = ms;
                commands.push(Command::SetGrade { percent: 0.0 });
            }
            Event::Pause => {
                if self.phase != Phase::Running {
                    return Err("Session is not running".into());
                }
                self.phase = Phase::Paused;
                commands.push(Command::Pause);
            }
            Event::Disconnected => {
                self.phase = Phase::Disconnected;
                self.control = false;
                self.telemetry_ms = None;
            }
            Event::Tick => {
                if self.phase == Phase::Running && self.telemetry_ms.is_none_or(|t| ms - t > 3000) {
                    self.phase = Phase::Paused;
                    commands.push(Command::Pause);
                }
            }
            Event::Grade { percent } => {
                if !percent.is_finite() {
                    return Err("Grade must be finite".into());
                }
                if self.phase != Phase::Running || self.telemetry_ms.is_none_or(|t| ms - t > 3000) {
                    return Err("Grade requires a running session with fresh telemetry".into());
                }
                // Initial synthetic-test limits: +/-5%, at most 1 percentage point/s.
                let delta = (ms - self.grade_ms).min(1000) as f64 / 1000.0;
                self.grade = percent
                    .clamp(-5.0, 5.0)
                    .clamp(self.grade - delta, self.grade + delta);
                self.grade_ms = ms;
                commands.push(Command::SetGrade {
                    percent: self.grade,
                });
            }
            Event::Telemetry { cadence_rpm, .. } => {
                if self.phase == Phase::Disconnected {
                    return Err("Telemetry without connection".into());
                }
                if cadence_rpm.is_some_and(|v| !v.is_finite() || !(0.0..=300.0).contains(&v)) {
                    return Err("Invalid cadence".into());
                }
                self.telemetry_ms = Some(ms);
            }
        }
        self.clock_ms = ms;
        Ok(Record {
            schema: 1,
            elapsed_ms: ms,
            event,
            phase: self.phase,
            commands,
        })
    }
}

/// Append-only JSONL, exclusively created. Successful append means sync_all
/// completed. Call before dispatching the record's transport requests.
pub struct Journal(std::fs::File);
impl Journal {
    pub fn create(path: impl AsRef<std::path::Path>) -> Result<Self, String> {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map(Self)
            .map_err(|e| e.to_string())
    }
    pub fn append(&mut self, record: &Record) -> Result<(), String> {
        let mut bytes = serde_json::to_vec(record).map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        self.0
            .write_all(&bytes)
            .and_then(|_| self.0.sync_all())
            .map_err(|e| e.to_string())
    }
}

/// Only a final incomplete line may be discarded after a crash. Complete
/// malformed lines, unsupported schemas, and reversed clocks are errors.
pub fn recover(bytes: &[u8]) -> Result<(Vec<Record>, bool), String> {
    let complete = bytes.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
    let mut records: Vec<Record> = Vec::new();
    for line in bytes[..complete]
        .split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
    {
        let r: Record = serde_json::from_slice(line).map_err(|e| e.to_string())?;
        if r.schema != 1 || records.last().is_some_and(|p| p.elapsed_ms > r.elapsed_ms) {
            return Err("Unsupported schema or non-monotonic journal".into());
        }
        records.push(r);
    }
    Ok((records, complete != bytes.len()))
}

pub fn telemetry_csv(records: &[Record]) -> String {
    let mut out = String::from("elapsed_ms,phase,power_w,cadence_rpm,heart_rate_bpm\n");
    for r in records {
        if let Event::Telemetry {
            power_w,
            cadence_rpm,
            heart_rate_bpm,
        } = r.event
        {
            out.push_str(&format!(
                "{},{:?},{},{},{}\n",
                r.elapsed_ms,
                r.phase,
                power_w.map_or(String::new(), |v| v.to_string()),
                cadence_rpm.map_or(String::new(), |v| v.to_string()),
                heart_rate_bpm.map_or(String::new(), |v| v.to_string())
            ));
        }
    }
    out
}

pub mod route_grade;
