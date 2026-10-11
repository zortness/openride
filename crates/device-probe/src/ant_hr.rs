//! Optional receive-only HR worker. USB waits never block FTMS control.
use crate::ant_probe::{parse_key, Probe};
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};

pub struct Worker {
    pub rx: mpsc::Receiver<Value>,
    stop: Arc<AtomicBool>,
    done: mpsc::Receiver<()>,
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
impl Worker {
    pub fn stop(&self) -> bool {
        self.stop.store(true, Ordering::Relaxed);
        // Only called after the trainer loop has finished, never during control.
        self.done.recv_timeout(Duration::from_secs(5)).is_ok()
    }
    pub fn start(config: Value, clock: Instant) -> Self {
        let (tx, rx) = mpsc::sync_channel(128);
        let (done_tx, done) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let cancelled = stop.clone();
        std::thread::spawn(move || {
            let emit = |row| {
                tx.try_send(row)
                    .map_err(|_| "HR event queue unavailable".to_string())
            };
            let result = receive(&config, clock, &cancelled, &emit);
            if let Err(message) = result {
                // If the queue is full, queued measurements still expire by their receipt time.
                let _ = emit(
                    json!({"event":"ant_hr_status","status":"unavailable","message":message,"received_ms":clock.elapsed().as_millis() as u64}),
                );
            }
            let _ = done_tx.send(());
        });
        Self { rx, stop, done }
    }
}
pub fn identity(config: &Value) -> Result<(u16, u8), String> {
    let n = config["device_number"]
        .as_u64()
        .filter(|n| (1..=65535).contains(n))
        .ok_or("HR device number must be 1-65535")?;
    let t = config["transmission_type"]
        .as_u64()
        .filter(|t| (1..=255).contains(t))
        .ok_or("HR transmission type must be 1-255")?;
    Ok((n as u16, t as u8))
}
fn measurement(frame: &trainer_core::ant::Frame, selected: (u16, u8)) -> Option<Option<u8>> {
    let d = &frame.data;
    if frame.id != 0x4e
        || d.len() < 14
        || d[0] != 0
        || d[9] & 0x80 == 0
        || u16::from_le_bytes([d[10], d[11]]) != selected.0
        || d[12] != 120
        || d[13] != selected.1
        || d[1] & 0x7f > 7
    {
        return None;
    }
    Some((1..=254).contains(&d[8]).then_some(d[8]))
}
fn receive(
    config: &Value,
    clock: Instant,
    stop: &AtomicBool,
    emit: &impl Fn(Value) -> Result<(), String>,
) -> Result<(), String> {
    use std::io::Read;
    let selected = identity(config)?;
    let mut key = String::new();
    std::fs::File::open(config["key_file"].as_str().ok_or("ANT key file missing")?)
        .map_err(|e| e.to_string())?
        .take(128)
        .read_to_string(&mut key)
        .map_err(|e| e.to_string())?;
    let key = parse_key(key.trim())?;
    let mut probe = Probe::open(|_| Ok(()))?;
    let result = (|| {
        probe.reset()?;
        let mut network = vec![0];
        network.extend(key);
        probe.command(0x46, &network)?;
        probe.command(0x42, &[0, 0, 0])?;
        let [lo, hi] = selected.0.to_le_bytes();
        probe.command(0x51, &[0, lo, hi, 120, selected.1])?;
        probe.command(0x44, &[0, 12])?;
        probe.command(0x45, &[0, 57])?;
        let [lo, hi] = 8070u16.to_le_bytes();
        probe.command(0x43, &[0, lo, hi])?;
        probe.command(0x6e, &[0, 0x80])?;
        probe.command(0x4b, &[0])?;
        let mut next_status = Instant::now();
        let mut last = None;
        while !stop.load(Ordering::Relaxed) {
            if let Some(frame) = probe.next()? {
                if let Some(bpm) = measurement(&frame, selected) {
                    let now = clock.elapsed().as_millis() as u64;
                    last = bpm.map(|b| (now, b));
                    emit(
                        json!({"event":"ant_hr_data","received_ms":now,"heart_rate_bpm":bpm,"device_number":selected.0,"device_type":120,"transmission_type":selected.1,"bytes":frame.data}),
                    )?;
                }
                if frame.id == 0x40 && frame.data.as_slice() == [0, 1, 7] {
                    last = None;
                    emit(
                        json!({"event":"ant_hr_status","status":"searching","received_ms":clock.elapsed().as_millis() as u64}),
                    )?;
                    probe.command(0x4b, &[0])?;
                }
            }
            if Instant::now() >= next_status {
                let now = clock.elapsed().as_millis() as u64;
                let fresh = last.filter(|(ms, _)| now.saturating_sub(*ms) < 3000);
                emit(
                    json!({"event":"ant_hr_status","status":if fresh.is_some(){"receiving"}else{"searching"},"heart_rate_bpm":fresh.map(|(_,b)|b),"measurement_ms":fresh.map(|(ms,_)|ms),"received_ms":now}),
                )?;
                next_status = Instant::now() + Duration::from_secs(1);
            }
        }
        Ok(())
    })();
    let cleanup = probe.reset();
    let _ = emit(json!({"event":"ant_hr_closed","radio_reset_confirmed":cleanup.is_ok()}));
    result.and(cleanup)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "Needs exclusively available ANT USB-m; receive-only, no trainer commands"]
    fn hardware_receiver_cleanup() {
        let config: Value = serde_json::from_str(
            &std::env::var("OPENRIDE_TEST_HR_CONFIG").expect("Set test config JSON"),
        )
        .unwrap();
        let clock = Instant::now();
        let worker = Worker::start(config, clock);
        let mut rows = vec![];
        while clock.elapsed() < Duration::from_secs(12) {
            if let Ok(row) = worker.rx.recv_timeout(Duration::from_millis(250)) {
                rows.push(row);
            }
        }
        assert!(worker.stop(), "Receiver cleanup timed out");
        rows.extend(worker.rx.try_iter());
        if let Ok(path) = std::env::var("OPENRIDE_TEST_HR_OUTPUT") {
            std::fs::write(path, serde_json::to_vec_pretty(&rows).unwrap()).unwrap();
        }
        assert!(
            !rows.iter().any(|r| r["status"] == "unavailable"),
            "{rows:?}"
        );
        assert!(
            rows.iter()
                .any(|r| r["event"] == "ant_hr_closed" && r["radio_reset_confirmed"] == true),
            "{rows:?}"
        );
        println!(
            "HR_RECEIVER_CHECK packets={} cleanup_confirmed=true",
            rows.iter().filter(|r| r["event"] == "ant_hr_data").count()
        );
    }
    #[test]
    fn selection_and_pages() {
        assert!(identity(&json!({"device_number":0,"transmission_type":1})).is_err());
        let mut f = trainer_core::ant::Frame {
            id: 0x4e,
            data: vec![0, 0, 0, 0, 0, 0, 0, 0, 80, 128, 1, 0, 120, 241],
        };
        assert_eq!(measurement(&f, (1, 241)), Some(Some(80)));
        assert_eq!(measurement(&f, (2, 241)), None);
        assert_eq!(measurement(&f, (1, 1)), None);
        f.data[1] = 128;
        assert_eq!(measurement(&f, (1, 241)), Some(Some(80)));
        f.data[1] = 80;
        assert_eq!(measurement(&f, (1, 241)), None);
        f.data[1] = 0;
        f.data[8] = 255;
        assert_eq!(measurement(&f, (1, 241)), Some(None));
    }
}
