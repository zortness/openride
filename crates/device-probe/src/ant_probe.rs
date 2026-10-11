//! Bounded ANT adapter/sensor diagnostic. It never creates an FE-C channel.
use crate::{ant_usb::Usb, journal::Journal};
use serde_json::json;
use std::{
    collections::VecDeque,
    path::Path,
    time::{Duration, Instant},
};
use trainer_core::ant::{encode, Frame, Framer, Profile, Sensor};
pub(crate) struct Probe {
    usb: Usb,
    framer: Framer,
    queue: VecDeque<Frame>,
    sink: Box<dyn FnMut(serde_json::Value) -> Result<(), String>>,
    clock: Instant,
}
impl Probe {
    pub(crate) fn open(
        sink: impl FnMut(serde_json::Value) -> Result<(), String> + 'static,
    ) -> Result<Self, String> {
        Ok(Self {
            usb: Usb::open()?,
            framer: Framer::default(),
            queue: VecDeque::new(),
            sink: Box::new(sink),
            clock: Instant::now(),
        })
    }

    fn log(&mut self, row: serde_json::Value) -> Result<(), String> {
        (self.sink)(row)
    }
    fn send(&mut self, id: u8, data: &[u8]) -> Result<(), String> {
        // Network key material is never written to the capture.
        self.log(json!({"event":"ant_command","id":id}))?;
        self.usb.send(&encode(id, data)?)
    }
    pub(crate) fn next(&mut self) -> Result<Option<Frame>, String> {
        if let Some(frame) = self.queue.pop_front() {
            return Ok(Some(frame));
        }
        let bytes = self.usb.receive()?;
        for frame in self.framer.push(&bytes) {
            self.log(json!({"event":"ant_frame","id":frame.id,"bytes":frame.data}))?;
            self.queue.push_back(frame);
        }
        Ok(self.queue.pop_front())
    }
    fn wait(&mut self, id: u8, ack: Option<u8>) -> Result<Frame, String> {
        let until = Instant::now() + Duration::from_secs(3);
        while Instant::now() < until {
            if let Some(f) = self.next()? {
                if f.id == id {
                    if let Some(command) = ack {
                        if f.data.len() < 3 || f.data[0] != 0 || f.data[1] != command {
                            continue;
                        }
                        if f.data[2] != 0 {
                            return Err(format!(
                                "ANT command {command:02x} rejected: {:02x}",
                                f.data[2]
                            ));
                        }
                    }
                    return Ok(f);
                }
            }
        }
        Err(format!(
            "ANT response timeout: message {id:02x}, command {ack:?}"
        ))
    }
    pub(crate) fn command(&mut self, id: u8, data: &[u8]) -> Result<(), String> {
        self.send(id, data)?;
        self.wait(0x40, Some(id))?;
        Ok(())
    }
    pub(crate) fn reset(&mut self) -> Result<(), String> {
        self.queue.clear();
        self.send(0x4a, &[0])?;
        self.wait(0x6f, None)?;
        Ok(())
    }
    fn capture(
        &mut self,
        profile: Option<Profile>,
        number: u16,
        seconds: u64,
        key: Option<[u8; 8]>,
    ) -> Result<(), String> {
        self.reset()?;
        self.send(0x4d, &[0, 0x3e])?;
        let version = self.wait(0x3e, None)?;
        self.send(0x4d, &[0, 0x54])?;
        let caps = self.wait(0x54, None)?;
        self.log(json!({"event":"ant_adapter_ready","version":String::from_utf8_lossy(&version.data).trim_end_matches('\0'),"capabilities":caps.data,"vid":"0fcf","pid":"1009"}))?;
        let Some(profile) = profile else {
            return Ok(());
        };
        let mut network = vec![0];
        network.extend(key.ok_or("ANT+ network key missing")?);
        self.command(0x46, &network)?;
        self.command(0x42, &[0, 0, 0])?; // receive channel, network zero
        let [lo, hi] = number.to_le_bytes();
        self.command(0x51, &[0, lo, hi, profile.device_type(), 0])?;
        self.command(0x44, &[0, 12])?; // bounded search timeout, 30 seconds
        self.command(0x45, &[0, 57])?;
        let [lo, hi] = profile.period().to_le_bytes();
        self.command(0x43, &[0, lo, hi])?;
        self.command(0x6e, &[0, 0x80])?; // extended device identity on each received packet
        self.command(0x4b, &[0])?;
        self.log(json!({"event":"ant_sensor_search","profile":profile,"requested_device_number":number,"seconds":seconds}))?;
        let deadline = Instant::now() + Duration::from_secs(seconds);
        let mut sensor = Sensor::new(profile);
        let mut identity = None;
        let mut readings = 0;
        let mut next_status = Instant::now();
        while Instant::now() < deadline {
            if let Some(f) = self.next()? {
                if f.id == 0x4e && f.data.len() >= 14 && f.data[0] == 0 && f.data[9] & 0x80 != 0 {
                    let found = u16::from_le_bytes([f.data[10], f.data[11]]);
                    let device_type = f.data[12];
                    let transmission = f.data[13];
                    if device_type != profile.device_type() || (number != 0 && found != number) {
                        continue;
                    }
                    let id = (found, device_type, transmission);
                    if identity != Some(id) {
                        sensor = Sensor::new(profile);
                        identity = Some(id);
                        self.log(json!({"event":"ant_sensor_identity","device_number":found,"device_type":device_type,"transmission_type":transmission}))?;
                    }
                    let now = self.clock.elapsed().as_millis() as u64;
                    let decoded = serde_json::to_value(sensor.update(&f.data[1..9], now)?)
                        .map_err(|e| e.to_string())?;
                    self.log(json!({"event":"ant_sensor_data","device_number":found,"device_type":device_type,"transmission_type":transmission,"decoded":decoded}))?;
                    readings += 1;
                }
                if f.id == 0x40 && f.data.as_slice() == [0, 1, 7] {
                    return Err("ANT receive channel closed (search timeout or radio loss)".into());
                }
            }
            if Instant::now() >= next_status {
                let now = self.clock.elapsed().as_millis() as u64;
                self.log(json!({"event":"ant_sensor_status","identity":identity,"reading":sensor.reading(now)}))?;
                next_status = Instant::now() + Duration::from_secs(1);
            }
        }
        self.log(json!({"event":"ant_capture_summary","sensor_packets":readings,"framing_errors":self.framer.errors,"sensor_verified":readings>0}))?;
        Ok(())
    }
}
pub(crate) fn parse_key(value: &str) -> Result<[u8; 8], String> {
    if value.len() != 16 || !value.is_ascii() {
        return Err("OPENRIDE_ANT_NETWORK_KEY must be 16 hexadecimal characters".into());
    }
    let mut out = [0; 8];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&value[i * 2..i * 2 + 2], 16)
            .map_err(|_| "Invalid ANT network key format")?;
    }
    Ok(out)
}
pub fn run(args: &[String]) -> Result<(), String> {
    let option = |prefix: &str| args.iter().find_map(|a| a.strip_prefix(prefix));
    let path = option("--ant-journal=").ok_or("Provide --ant-journal=NEW_PATH")?;
    let profile = option("--ant-profile=").map(Profile::parse).transpose()?;
    let seconds = option("--seconds=")
        .unwrap_or("20")
        .parse::<u64>()
        .map_err(|_| "Invalid duration")?;
    if !(1..=120).contains(&seconds) {
        return Err("ANT capture duration must be 1-120 seconds".into());
    }
    let number = option("--device-number=")
        .unwrap_or("0")
        .parse::<u16>()
        .map_err(|_| "Invalid device number")?;
    let key = if profile.is_some() {
        Some(parse_key(&std::env::var("OPENRIDE_ANT_NETWORK_KEY").map_err(|_|"Set OPENRIDE_ANT_NETWORK_KEY from Garmin's ANT+ network-key documentation; adapter-only checks need no key")?)?)
    } else {
        None
    };
    let mut journal = Journal::create(Path::new(path))?;
    let usb = match Usb::open() {
        Ok(u) => u,
        Err(e) => {
            journal.record(0, json!({"event":"ant_probe_failed","message":e}))?;
            return Err(e);
        }
    };
    let clock = Instant::now();
    let mut p = Probe {
        usb,
        framer: Framer::default(),
        queue: VecDeque::new(),
        sink: Box::new(move |row| journal.record(clock.elapsed().as_millis() as u64, row)),
        clock: Instant::now(),
    };
    let result = p.capture(profile, number, seconds, key);
    // Reset closes all channels in this exclusively owned diagnostic adapter.
    let cleanup = p.reset();
    p.log(json!({"event":"ant_probe_finished","ok":result.is_ok() && cleanup.is_ok(),"message":result.as_ref().err(),"radio_reset_confirmed":cleanup.is_ok(),"cleanup_error":cleanup.as_ref().err()}))?;
    result.and(cleanup)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_key_validation() {
        assert!(parse_key("0011223344556677").is_ok());
        assert!(parse_key("123").is_err());
        assert!(parse_key("gg11223344556677").is_err());
    }
}
