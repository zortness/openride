//! ANT framing and receive-only sensor measurements. No USB or trainer control.
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub id: u8,
    pub data: Vec<u8>,
}
pub fn encode(id: u8, data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() > 64 {
        return Err("ANT payload exceeds supported size".into());
    }
    let mut b = vec![0xa4, data.len() as u8, id];
    b.extend(data);
    b.push(b.iter().fold(0, |a, b| a ^ b));
    Ok(b)
}
#[derive(Default)]
pub struct Framer {
    buffer: Vec<u8>,
    pub errors: u64,
}
impl Framer {
    pub fn push(&mut self, bytes: &[u8]) -> Vec<Frame> {
        let mut out = vec![];
        for b in bytes {
            self.buffer.push(*b);
            loop {
                if self.buffer.is_empty() {
                    break;
                }
                if self.buffer[0] != 0xa4 {
                    self.buffer.remove(0);
                    self.errors += 1;
                    continue;
                }
                if self.buffer.len() < 2 {
                    break;
                }
                let len = self.buffer[1] as usize;
                if len > 64 {
                    self.buffer.remove(0);
                    self.errors += 1;
                    continue;
                }
                if self.buffer.len() < len + 4 {
                    break;
                }
                if self.buffer[..len + 4].iter().fold(0, |a, b| a ^ b) != 0 {
                    self.buffer.remove(0);
                    self.errors += 1;
                    continue;
                }
                out.push(Frame {
                    id: self.buffer[2],
                    data: self.buffer[3..3 + len].to_vec(),
                });
                self.buffer.drain(..len + 4);
            }
        }
        out
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Profile {
    HeartRate,
    Cadence,
    Speed,
    SpeedCadence,
}
impl Profile {
    pub fn parse(name: &str) -> Result<Self, String> {
        match name {
            "hr" => Ok(Self::HeartRate),
            "cadence" => Ok(Self::Cadence),
            "speed" => Ok(Self::Speed),
            "speed-cadence" => Ok(Self::SpeedCadence),
            _ => Err("Profile must be hr, cadence, speed or speed-cadence".into()),
        }
    }
    pub fn device_type(self) -> u8 {
        match self {
            Self::HeartRate => 120,
            Self::Cadence => 122,
            Self::Speed => 123,
            Self::SpeedCadence => 121,
        }
    }
    pub fn period(self) -> u16 {
        match self {
            Self::HeartRate => 8070,
            Self::Speed => 8118,
            _ => 8086,
        }
    }
}
#[derive(Debug, Default, Serialize)]
pub struct Reading {
    pub heart_rate_bpm: Option<u8>,
    pub cadence_rpm: Option<f64>,
    pub wheel_revolutions_per_s: Option<f64>,
}
#[derive(Default)]
struct Counter {
    previous: Option<(u16, u16, u64)>,
    changed_ms: u64,
    rate: Option<f64>,
}
impl Counter {
    fn update(&mut self, time: u16, count: u16, now: u64) -> Option<f64> {
        let Some((old_time, old_count, seen)) = self.previous else {
            self.previous = Some((time, count, now));
            self.changed_ms = now;
            return None;
        };
        self.previous = Some((time, count, now));
        if now < seen || now - seen >= 3000 {
            self.changed_ms = now;
            self.rate = None;
            return None;
        }
        if time == old_time && count == old_count {
            if now - self.changed_ms >= 3000 {
                self.rate = Some(0.);
            }
        } else {
            let ticks = time.wrapping_sub(old_time);
            let revolutions = count.wrapping_sub(old_count);
            self.changed_ms = now;
            self.rate = if ticks > 0 && revolutions > 0 {
                Some(f64::from(revolutions) * 1024. / f64::from(ticks))
            } else {
                None
            };
            // Reject reset/reordered counters rather than manufacture extreme cadence/speed.
            if self.rate.is_some_and(|v| v > 20.) {
                self.rate = None;
            }
        }
        self.rate
    }
}
pub struct Sensor {
    profile: Profile,
    cadence: Counter,
    speed: Counter,
    last_ms: Option<u64>,
    reading: Reading,
}
impl Sensor {
    pub fn new(profile: Profile) -> Self {
        Self {
            profile,
            cadence: Counter::default(),
            speed: Counter::default(),
            last_ms: None,
            reading: Reading::default(),
        }
    }
    pub fn update(&mut self, bytes: &[u8], now: u64) -> Result<&Reading, String> {
        if bytes.len() != 8 {
            return Err("ANT sensor payload must contain eight bytes".into());
        }
        if self.last_ms.is_some_and(|t| now < t) {
            return Err("ANT sensor clock moved backwards".into());
        }
        let word = |i| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
        let page = bytes[0] & 0x7f;
        // Common manufacturer/product pages are not measurement pages.
        match self.profile {
            Profile::HeartRate if page <= 7 => {
                self.reading.heart_rate_bpm = match bytes[7] {
                    0 | 255 => None,
                    v => Some(v),
                };
            }
            Profile::Cadence if page <= 5 => {
                self.reading.cadence_rpm = self
                    .cadence
                    .update(word(4), word(6), now)
                    .map(|v| v * 60.)
                    .filter(|v| *v <= 300.);
            }
            Profile::Speed if page <= 5 => {
                self.reading.wheel_revolutions_per_s = self.speed.update(word(4), word(6), now);
            }
            Profile::SpeedCadence => {
                self.reading.cadence_rpm = self
                    .cadence
                    .update(word(0), word(2), now)
                    .map(|v| v * 60.)
                    .filter(|v| *v <= 300.);
                self.reading.wheel_revolutions_per_s = self.speed.update(word(4), word(6), now);
            }
            _ => return Ok(self.reading(now)),
        }
        self.last_ms = Some(now);
        Ok(&self.reading)
    }
    pub fn reading(&self, now: u64) -> &Reading {
        static MISSING: Reading = Reading {
            heart_rate_bpm: None,
            cadence_rpm: None,
            wheel_revolutions_per_s: None,
        };
        if self.last_ms.is_none_or(|t| now < t || now - t >= 3000) {
            &MISSING
        } else {
            &self.reading
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fragmented_coalesced_and_corrupt_frames() {
        let b = encode(0x4e, &[0; 9]).unwrap();
        let mut p = Framer::default();
        assert!(p.push(&b[..4]).is_empty());
        assert_eq!(p.push(&b[4..]).len(), 1);
        let mut bad = b.clone();
        bad[5] = 1;
        bad.extend(&b);
        bad.extend(&b);
        assert_eq!(p.push(&bad).len(), 2);
        assert!(p.errors > 0);
        for _ in 0..1000 {
            p.push(&[0xa4, 255]);
        }
        assert!(p.buffer.len() < 68);
    }
    fn packet(time: u16, count: u16) -> [u8; 8] {
        let mut b = [0; 8];
        b[4..6].copy_from_slice(&time.to_le_bytes());
        b[6..].copy_from_slice(&count.to_le_bytes());
        b
    }
    #[test]
    fn cadence_rollover_duplicates_stopped_and_stale() {
        let mut s = Sensor::new(Profile::Cadence);
        assert_eq!(
            s.update(&packet(65000, 65535), 0).unwrap().cadence_rpm,
            None
        );
        let b = packet(488, 0);
        assert_eq!(s.update(&b, 1000).unwrap().cadence_rpm, Some(60.));
        for t in [2000, 3000] {
            assert_eq!(s.update(&b, t).unwrap().cadence_rpm, Some(60.));
        }
        assert_eq!(s.update(&b, 4000).unwrap().cadence_rpm, Some(0.));
        assert_eq!(s.reading(7000).cadence_rpm, None);
        assert_eq!(s.update(&packet(1512, 1), 8000).unwrap().cadence_rpm, None);
        assert!(s.update(&b, 7999).is_err());
        assert!(s.update(&b[..7], 9000).is_err());
    }
    #[test]
    fn hr_toggle_common_pages_and_zero_missing() {
        let mut s = Sensor::new(Profile::HeartRate);
        let mut b = [0; 8];
        b[0] = 0x80;
        b[7] = 123;
        assert_eq!(s.update(&b, 10).unwrap().heart_rate_bpm, Some(123));
        b[0] = 80;
        b[7] = 200;
        assert_eq!(s.update(&b, 3010).unwrap().heart_rate_bpm, None);
        b[0] = 0;
        b[7] = 0;
        assert_eq!(s.update(&b, 4000).unwrap().heart_rate_bpm, None);
    }
    #[test]
    fn combined_sensor_and_reset_rejection() {
        let mut s = Sensor::new(Profile::SpeedCadence);
        s.update(&[0; 8], 0).unwrap();
        let b = [0, 4, 1, 0, 0, 4, 2, 0];
        let r = s.update(&b, 1000).unwrap();
        assert_eq!(r.cadence_rpm, Some(60.));
        assert_eq!(r.wheel_revolutions_per_s, Some(2.));
        assert_eq!(s.update(&[0; 8], 1200).unwrap().cadence_rpm, None);
    }
}
