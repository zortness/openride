//! Standard HRS, CPS and CSC notification decoding; no device I/O.
//! References: Bluetooth SIG GATT Specification Supplement, HRS 1.0, CSCS 1.0.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    HeartRate,
    CyclingPower,
    Csc,
}
impl Kind {
    pub fn service(self) -> u32 {
        match self {
            Self::HeartRate => 0x180d,
            Self::CyclingPower => 0x1818,
            Self::Csc => 0x1816,
        }
    }
    pub fn characteristic(self) -> u32 {
        match self {
            Self::HeartRate => 0x2a37,
            Self::CyclingPower => 0x2a63,
            Self::Csc => 0x2a5b,
        }
    }
}
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Values {
    pub power_w: Option<i16>,
    pub cadence_rpm: Option<f64>,
    pub heart_rate_bpm: Option<u8>,
}
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}
impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let value = self
            .bytes
            .get(self.pos..self.pos + n)
            .ok_or("Truncated Bluetooth sensor packet")?;
        self.pos += n;
        Ok(value)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn end(&self) -> Result<(), String> {
        if self.pos == self.bytes.len() {
            Ok(())
        } else {
            Err("Unexpected sensor packet bytes".into())
        }
    }
}

#[derive(Default)]
pub struct Decoder {
    crank: Counter,
}
#[derive(Default)]
struct Counter {
    previous: Option<(u16, u16, u64)>,
    changed: u64,
    rpm: Option<f64>,
}
impl Counter {
    fn update(&mut self, count: u16, ticks: u16, now: u64) -> Option<f64> {
        let old = self.previous.replace((count, ticks, now));
        let Some((old_count, old_ticks, seen)) = old else {
            self.changed = now;
            return None;
        };
        if now < seen || now - seen >= 3000 {
            self.changed = now;
            self.rpm = None;
            return None;
        }
        if count == old_count && ticks == old_ticks {
            if now - self.changed >= 3000 {
                self.rpm = Some(0.);
            }
        } else {
            let revs = count.wrapping_sub(old_count);
            let elapsed = ticks.wrapping_sub(old_ticks);
            self.changed = now;
            self.rpm = if revs > 0 && elapsed > 0 {
                Some(f64::from(revs) * 60. * 1024. / f64::from(elapsed)).filter(|r| *r <= 300.)
            } else {
                None
            };
        }
        self.rpm
    }
}
impl Decoder {
    pub fn decode(&mut self, kind: Kind, bytes: &[u8], now: u64) -> Result<Values, String> {
        let mut r = Reader { bytes, pos: 0 };
        let mut values = Values::default();
        let mut crank = None;
        match kind {
            Kind::HeartRate => {
                let flags = r.u8()?;
                if flags & 0xe0 != 0 {
                    return Err("Unknown HR flags".into());
                }
                let bpm = if flags & 1 != 0 {
                    r.u16()?
                } else {
                    u16::from(r.u8()?)
                };
                if flags & 8 != 0 {
                    r.take(2)?;
                }
                if flags & 16 != 0 {
                    if r.pos == bytes.len() || (bytes.len() - r.pos) % 2 != 0 {
                        return Err("Invalid HR RR intervals".into());
                    }
                    r.take(bytes.len() - r.pos)?;
                }
                if bpm > 254 {
                    return Err("HR exceeds supported activity-export range (254 bpm)".into());
                }
                values.heart_rate_bpm = if flags & 4 != 0 && flags & 2 == 0 {
                    None
                } else {
                    Some(bpm as u8)
                };
            }
            Kind::Csc => {
                let flags = r.u8()?;
                if flags & !3 != 0 {
                    return Err("Unknown CSC flags".into());
                }
                if flags & 1 != 0 {
                    r.take(6)?;
                } // Wheel counts do not replace route speed.
                if flags & 2 != 0 {
                    crank = Some((r.u16()?, r.u16()?));
                }
            }
            Kind::CyclingPower => {
                let flags = r.u16()?;
                if flags & 0xe000 != 0 {
                    return Err("Unknown cycling-power flags".into());
                }
                values.power_w = Some(r.u16()? as i16);
                if flags & 1 != 0 {
                    r.take(1)?;
                }
                if flags & 4 != 0 {
                    r.take(2)?;
                }
                if flags & 16 != 0 {
                    r.take(6)?;
                }
                if flags & 32 != 0 {
                    crank = Some((r.u16()?, r.u16()?));
                }
                for (bit, size) in [(6, 4), (7, 4), (8, 3), (9, 2), (10, 2), (11, 2)] {
                    if flags & (1 << bit) != 0 {
                        r.take(size)?;
                    }
                }
            }
        }
        r.end()?; // A malformed packet must not advance the cadence counter.
        if let Some((count, ticks)) = crank {
            values.cadence_rpm = self.crank.update(count, ticks, now);
        }
        Ok(values)
    }
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct Sample {
    #[serde(flatten)]
    pub values: Values,
    pub power_received_ms: Option<u64>,
    pub cadence_received_ms: Option<u64>,
    pub hr_received_ms: Option<u64>,
}
#[derive(Default)]
struct Timed<T> {
    value: Option<T>,
    ms: Option<u64>,
}
impl<T: Copy> Timed<T> {
    fn set(&mut self, value: Option<T>, ms: u64) {
        if self.ms.is_none_or(|t| ms >= t) {
            self.value = value;
            self.ms = Some(ms);
        }
    }
    fn fresh(&self, now: u64) -> Option<T> {
        self.ms
            .filter(|t| now >= *t && now - *t < 3000)
            .and(self.value)
    }
}
pub struct Selected {
    pairing: crate::pairing::Pairing,
    power: Timed<i16>,
    cadence: Timed<f64>,
    hr: Timed<u8>,
}
impl Selected {
    pub fn new(pairing: crate::pairing::Pairing) -> Self {
        Self {
            pairing,
            power: Timed::default(),
            cadence: Timed::default(),
            hr: Timed::default(),
        }
    }
    pub fn trainer(&mut self, data: &crate::ftms::BikeData, ms: u64) {
        if self.pairing.power.protocol.is_none() {
            self.power.set(data.power_w, ms);
        }
        if self
            .pairing
            .cadence
            .as_ref()
            .is_some_and(|d| d.protocol.is_none())
        {
            self.cadence.set(data.cadence_rpm, ms);
        }
    }
    pub fn sensor(&mut self, id: &str, kind: Kind, data: Values, ms: u64) {
        let matches = |d: &crate::pairing::Device| d.id == id && d.protocol == Some(kind);
        if matches(&self.pairing.power) {
            self.power.set(data.power_w, ms);
        }
        if self.pairing.cadence.as_ref().is_some_and(matches) {
            self.cadence.set(data.cadence_rpm, ms);
        }
        if self.pairing.heart_rate.as_ref().is_some_and(matches) {
            self.hr.set(data.heart_rate_bpm, ms);
        }
    }
    pub fn sample(&self, now: u64) -> Sample {
        Sample {
            values: Values {
                power_w: self.power.fresh(now),
                cadence_rpm: self.cadence.fresh(now),
                heart_rate_bpm: self.hr.fresh(now),
            },
            power_received_ms: self.power.ms,
            cadence_received_ms: self.cadence.ms,
            hr_received_ms: self.hr.ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hr_width_contact_and_optional_fields() {
        let mut d = Decoder::default();
        assert_eq!(
            d.decode(Kind::HeartRate, &[0, 0], 0)
                .unwrap()
                .heart_rate_bpm,
            Some(0)
        );
        assert_eq!(
            d.decode(Kind::HeartRate, &[0x19, 200, 0, 1, 0, 0, 4], 0)
                .unwrap()
                .heart_rate_bpm,
            Some(200)
        );
        assert_eq!(
            d.decode(Kind::HeartRate, &[4, 100], 0)
                .unwrap()
                .heart_rate_bpm,
            None
        );
        for bytes in [&[1, 44, 1][..], &[16, 100, 1], &[8, 100], &[0, 90, 0]] {
            assert!(d.decode(Kind::HeartRate, bytes, 0).is_err());
        }
    }
    #[test]
    fn power_signed_zero_and_optional_offsets() {
        let mut d = Decoder::default();
        assert_eq!(
            d.decode(Kind::CyclingPower, &[0, 0, 0, 0], 0)
                .unwrap()
                .power_w,
            Some(0)
        );
        assert_eq!(
            d.decode(Kind::CyclingPower, &[0, 0, 255, 255], 0)
                .unwrap()
                .power_w,
            Some(-1)
        );
        let mut b = vec![0xf5, 0x0f, 200, 0];
        b.extend([0; 30]);
        assert!(d.decode(Kind::CyclingPower, &b, 0).is_ok());
        for size in 0..b.len() {
            assert!(d.decode(Kind::CyclingPower, &b[..size], 0).is_err());
        }
    }
    #[test]
    fn cadence_rollover_duplicate_stopped_reset_and_gap() {
        let mut d = Decoder::default();
        let p = |count: u16, time: u16| {
            let mut b = vec![2];
            b.extend(count.to_le_bytes());
            b.extend(time.to_le_bytes());
            b
        };
        assert_eq!(
            d.decode(Kind::Csc, &p(65535, 65000), 0)
                .unwrap()
                .cadence_rpm,
            None
        );
        assert_eq!(
            d.decode(Kind::Csc, &p(0, 488), 1000).unwrap().cadence_rpm,
            Some(60.)
        );
        for t in [2000, 3000] {
            assert_eq!(
                d.decode(Kind::Csc, &p(0, 488), t).unwrap().cadence_rpm,
                Some(60.)
            );
        }
        assert_eq!(
            d.decode(Kind::Csc, &p(0, 488), 4000).unwrap().cadence_rpm,
            Some(0.)
        );
        assert_eq!(
            d.decode(Kind::Csc, &p(2, 2536), 8000).unwrap().cadence_rpm,
            None
        );
        assert_eq!(
            d.decode(Kind::Csc, &p(0, 0), 8100).unwrap().cadence_rpm,
            None
        );
        assert!(d.decode(Kind::Csc, &[3, 0], 8200).is_err());
    }
    #[test]
    fn selected_identity_no_fallback_and_independent_freshness() {
        let p=crate::pairing::Pairing::parse(r#"{"schema":2,"resistance":{"id":"trainer","name":"T"},"power":{"id":"meter","name":"P","protocol":"cycling_power"},"cadence":{"id":"cad","name":"C","protocol":"csc"},"heart_rate":{"id":"hr","name":"H","protocol":"heart_rate"}}"#).unwrap();
        let mut s = Selected::new(p);
        s.trainer(
            &crate::ftms::BikeData {
                power_w: Some(900),
                cadence_rpm: Some(80.),
                ..Default::default()
            },
            0,
        );
        s.sensor(
            "wrong",
            Kind::CyclingPower,
            Values {
                power_w: Some(300),
                ..Default::default()
            },
            0,
        );
        assert_eq!(s.sample(0).values, Values::default());
        s.sensor(
            "meter",
            Kind::CyclingPower,
            Values {
                power_w: Some(0),
                ..Default::default()
            },
            100,
        );
        s.sensor(
            "cad",
            Kind::Csc,
            Values {
                cadence_rpm: Some(90.),
                ..Default::default()
            },
            2000,
        );
        assert_eq!(s.sample(3099).values.power_w, Some(0));
        assert_eq!(s.sample(3100).values.power_w, None);
        assert_eq!(s.sample(3100).values.cadence_rpm, Some(90.));
        s.sensor("meter", Kind::CyclingPower, Values::default(), 3200);
        assert_eq!(s.sample(3200).values.power_w, None);
    }
}
