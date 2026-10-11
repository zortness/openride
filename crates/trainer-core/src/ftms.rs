//! FTMS wire types. Units and opcodes follow Bluetooth FTMS 1.0.
use serde::Serialize;

#[derive(Debug, Serialize, PartialEq)]
pub struct Features {
    pub machine_bits: u32,
    pub target_bits: u32,
    pub cadence: bool,
    pub power: bool,
    pub simulation: bool,
    pub target_power: bool,
}
pub fn features(data: &[u8]) -> Result<Features, String> {
    if data.len() != 8 {
        return Err("FTMS features require exactly 8 bytes".into());
    }
    let machine_bits = u32::from_le_bytes(data[..4].try_into().unwrap());
    let target_bits = u32::from_le_bytes(data[4..].try_into().unwrap());
    Ok(Features {
        machine_bits,
        target_bits,
        cadence: machine_bits & 2 != 0,
        power: machine_bits & (1 << 14) != 0,
        simulation: target_bits & (1 << 13) != 0,
        target_power: target_bits & 8 != 0,
    })
}

#[derive(Debug, Default, Serialize, PartialEq)]
pub struct BikeData {
    pub more_data: bool,
    pub speed_kmh: Option<f64>,
    pub cadence_rpm: Option<f64>,
    pub distance_m: Option<u32>,
    pub power_w: Option<i16>,
    pub heart_rate_bpm: Option<u8>,
    pub elapsed_s: Option<u16>,
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}
impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], String> {
        let end = self.pos + n;
        let result = self
            .data
            .get(self.pos..end)
            .ok_or("Truncated Indoor Bike Data")?;
        self.pos = end;
        Ok(result)
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
}

/// One notification, not an assembled multipart record. Absent values stay absent.
pub fn indoor_bike(data: &[u8]) -> Result<BikeData, String> {
    let mut r = Reader { data, pos: 0 };
    let flags = r.u16()?;
    if flags & 0xe000 != 0 {
        return Err("Unknown Indoor Bike Data flags".into());
    }
    // Resistance field widths differ in deployed implementations. Do not guess
    // offsets until a device-specific layout is explicitly verified.
    if flags & (1 << 5) != 0 {
        return Err("Resistance-field layout not yet supported; raw packet retained".into());
    }
    let mut out = BikeData {
        more_data: flags & 1 != 0,
        ..Default::default()
    };
    if !out.more_data {
        out.speed_kmh = Some(f64::from(r.u16()?) / 100.0);
    }
    if flags & 2 != 0 {
        r.take(2)?;
    }
    if flags & 4 != 0 {
        out.cadence_rpm = Some(f64::from(r.u16()?) / 2.0);
    }
    if flags & 8 != 0 {
        r.take(2)?;
    }
    if flags & 16 != 0 {
        let d = r.take(3)?;
        out.distance_m = Some(u32::from_le_bytes([d[0], d[1], d[2], 0]));
    }
    if flags & 64 != 0 {
        out.power_w = Some(r.u16()? as i16);
    }
    if flags & 128 != 0 {
        r.take(2)?;
    }
    if flags & 256 != 0 {
        r.take(5)?;
    }
    if flags & 512 != 0 {
        out.heart_rate_bpm = Some(r.take(1)?[0]);
    }
    if flags & 1024 != 0 {
        r.take(1)?;
    }
    if flags & 2048 != 0 {
        out.elapsed_s = Some(r.u16()?);
    }
    if flags & 4096 != 0 {
        r.take(2)?;
    }
    if r.pos != data.len() {
        return Err("Unexpected trailing Indoor Bike Data bytes".into());
    }
    Ok(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    Acquire,
    Start,
    Pause,
    /// Wire units: mm/s, hundredths of percent, 0.0001 Crr, 0.01 kg/m Cw.
    Simulation {
        wind_mm_s: i16,
        grade_hundredths: i16,
        crr: u8,
        cw: u8,
    },
}
impl Request {
    pub fn encode(self) -> Vec<u8> {
        match self {
            Self::Acquire => vec![0],
            Self::Start => vec![7],
            Self::Pause => vec![8, 2],
            Self::Simulation {
                wind_mm_s,
                grade_hundredths,
                crr,
                cw,
            } => {
                let mut bytes = vec![0x11];
                bytes.extend(wind_mm_s.to_le_bytes());
                bytes.extend(grade_hundredths.to_le_bytes());
                bytes.extend([crr, cw]);
                bytes
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Accepted(Request),
    Rejected { request: Request, code: u8 },
}
struct Pending {
    id: u64,
    request: Request,
    deadline: u64,
    written: bool,
    response: Option<u8>,
}

/// One instance per physical BLE connection. Not Clone: the owning transport
/// serializes all access. A timeout/transport fault requires a fresh connection;
/// FTMS has no transaction IDs to disambiguate a delayed response after retry.
pub struct ControlPoint {
    pending: Option<Pending>,
    next_id: u64,
    now: u64,
    faulted: bool,
    control: bool,
    simulation: bool,
}
impl ControlPoint {
    pub fn new(features: &Features) -> Self {
        Self {
            pending: None,
            next_id: 0,
            now: 0,
            faulted: false,
            control: false,
            simulation: features.simulation,
        }
    }
    pub fn has_control(&self) -> bool {
        self.control
    }
    pub fn invalidate(&mut self) {
        self.pending = None;
        self.control = false;
        self.faulted = true;
    }
    /// Fitness Machine Status (0x2ADA). Control loss/reset makes every pending
    /// procedure unusable. Other status payloads remain for the session layer.
    pub fn machine_status(&mut self, data: &[u8]) -> Result<(), String> {
        let opcode = *data.first().ok_or("Empty Fitness Machine Status")?;
        if matches!(opcode, 0x01 | 0xff) {
            self.invalidate();
            if data.len() != 1 {
                return Err("Malformed reset/control-loss status".into());
            }
        }
        Ok(())
    }
    pub fn tick(&mut self, ms: u64) -> Result<(), String> {
        if ms < self.now {
            self.invalidate();
            return Err("Control clock moved backwards".into());
        }
        self.now = ms;
        if self.pending.as_ref().is_some_and(|p| ms >= p.deadline) {
            self.invalidate();
            return Err("FTMS response timeout; reconnect required".into());
        }
        if self.faulted {
            return Err("Control connection invalid; reconnect required".into());
        }
        Ok(())
    }
    /// Call only after control-point indications are enabled successfully.
    pub fn begin(&mut self, ms: u64, request: Request) -> Result<(u64, Vec<u8>), String> {
        self.tick(ms)?;
        if self.pending.is_some() {
            return Err("FTMS procedure already pending".into());
        }
        if request != Request::Acquire && !self.control {
            return Err("FTMS control not acquired".into());
        }
        if matches!(request, Request::Simulation { .. }) && !self.simulation {
            return Err("Simulation unsupported".into());
        }
        let deadline = ms.checked_add(5000).ok_or("Clock overflow")?;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or("Transaction counter overflow")?;
        self.pending = Some(Pending {
            id: self.next_id,
            request,
            deadline,
            written: false,
            response: None,
        });
        Ok((self.next_id, request.encode()))
    }
    pub fn write_complete(
        &mut self,
        ms: u64,
        id: u64,
        success: bool,
    ) -> Result<Option<Outcome>, String> {
        self.tick(ms)?;
        if self.pending.as_ref().is_none_or(|p| p.id != id) {
            self.invalidate();
            return Err("Unexpected write completion".into());
        }
        if !success {
            self.invalidate();
            return Err("GATT write failed; reconnect required".into());
        }
        self.pending.as_mut().unwrap().written = true;
        Ok(self.finish())
    }
    pub fn indication(&mut self, ms: u64, data: &[u8]) -> Result<Option<Outcome>, String> {
        self.tick(ms)?;
        let valid = data.len() == 3
            && data[0] == 0x80
            && (1..=5).contains(&data[2])
            && self
                .pending
                .as_ref()
                .is_some_and(|p| p.request.encode()[0] == data[1] && p.response.is_none());
        if !valid {
            self.invalidate();
            return Err("Malformed, duplicate or unmatched FTMS response".into());
        }
        self.pending.as_mut().unwrap().response = Some(data[2]);
        Ok(self.finish())
    }
    fn finish(&mut self) -> Option<Outcome> {
        if !self
            .pending
            .as_ref()
            .is_some_and(|p| p.written && p.response.is_some())
        {
            return None;
        }
        let p = self.pending.take().unwrap();
        let code = p.response.unwrap();
        if code == 1 {
            if p.request == Request::Acquire {
                self.control = true;
            }
            Some(Outcome::Accepted(p.request))
        } else {
            // Conservative policy: explicit re-acquisition after any rejection.
            self.control = false;
            Some(Outcome::Rejected {
                request: p.request,
                code,
            })
        }
    }
}
