//! Minimal FIT activity encoder, original MIT code; field numbers/scales from
//! Garmin's published FIT profile. No SDK code or generated profile is bundled.
use crate::{Activity, Sample};
use std::collections::BTreeMap;
const FIT_EPOCH_UNIX_S: u64 = 631065600;
fn timestamp(a: &Activity, ms: u64) -> Result<u32, String> {
    let seconds = (a.unix_ms(ms)? / 1000)
        .checked_sub(FIT_EPOCH_UNIX_S)
        .ok_or("Timestamp predates FIT epoch")?;
    u32::try_from(seconds)
        .ok()
        .filter(|v| *v != u32::MAX)
        .ok_or("Timestamp exceeds FIT range".into())
}
fn duration(ms: u64) -> Result<u32, String> {
    u32::try_from(ms)
        .ok()
        .filter(|v| *v != u32::MAX)
        .ok_or("Duration exceeds FIT range".into())
}
pub(crate) fn crc(bytes: &[u8]) -> u16 {
    let mut crc = 0u16;
    for &b in bytes {
        crc ^= u16::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xa001
            } else {
                crc >> 1
            };
        }
    }
    crc
}

struct Writer(Vec<u8>);
impl Writer {
    fn definition(&mut self, local: u8, global: u16, fields: &[(u8, u8, u8)]) {
        self.0.extend([0x40 | local, 0, 0]);
        self.u16(global);
        self.u8(fields.len() as u8);
        for &(number, size, kind) in fields {
            self.0.extend([number, size, kind]);
        }
    }
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend(v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend(v.to_le_bytes());
    }
    fn event(&mut self, stamp: u32, kind: u8) {
        self.u8(1);
        self.u32(stamp);
        self.0.extend([0, kind, 0]);
    }
    fn sample(
        &mut self,
        stamp: u32,
        s: &Sample,
        position: Option<[f64; 3]>,
        losses: &mut usize,
    ) -> bool {
        let d = &s.data;
        let power = d.power_w.filter(|p| *p >= 0).map(|p| p as u16);
        let hr = d.heart_rate_bpm.filter(|p| *p != 255);
        let cadence = d
            .cadence_rpm
            .map(|v| (v * 128.0).round())
            .filter(|v| *v < 255.0 * 128.0)
            .map(|v| v as u32);
        let speed = d
            .speed_kmh
            .map(|v| (v / 3.6 * 1000.0).round())
            .filter(|v| *v < f64::from(u32::MAX))
            .map(|v| v as u32);
        *losses += usize::from(d.power_w.is_some() && power.is_none())
            + usize::from(d.heart_rate_bpm.is_some() && hr.is_none())
            + usize::from(d.cadence_rpm.is_some() && cadence.is_none())
            + usize::from(d.speed_kmh.is_some() && speed.is_none());
        if power.is_none() && hr.is_none() && cadence.is_none() && speed.is_none() {
            return false;
        }
        self.u8(2);
        self.u32(stamp);
        self.u8(hr.unwrap_or(255));
        self.u8(cadence.map_or(255, |c| (c / 128) as u8));
        self.u16(speed.filter(|v| *v < 65535).map_or(65535, |v| v as u16));
        self.u16(power.unwrap_or(65535));
        self.u8(cadence.map_or(255, |c| (c % 128) as u8));
        self.u32(speed.unwrap_or(u32::MAX));
        self.u32(
            s.virtual_distance_m
                .map_or(u32::MAX, |d| (d * 100.).round() as u32),
        );
        // FIT signed semicircles; absent geography uses the signed invalid sentinel.
        for axis in 0..2 {
            let coordinate = position.map(|p| (p[axis] / 180. * 2147483648.).round() as i32);
            self.u32(coordinate.unwrap_or(i32::MAX) as u32);
        }
        let altitude = position
            .map(|p| ((p[2] + 500.) * 5.).round())
            .filter(|v| *v >= 0. && *v < f64::from(u32::MAX))
            .map(|v| v as u32);
        *losses += usize::from(position.is_some() && altitude.is_none());
        self.u16(altitude.filter(|v| *v < 65535).map_or(65535, |v| v as u16));
        self.u32(altitude.unwrap_or(u32::MAX));
        true
    }
}
pub fn encode(a: &Activity) -> Result<(Vec<u8>, usize, Vec<String>), String> {
    encode_with_geography(a, None)
}
pub(crate) fn encode_with_geography(
    a: &Activity,
    geography: Option<&crate::xml::Geography>,
) -> Result<(Vec<u8>, usize, Vec<String>), String> {
    let start = timestamp(a, a.start_ms)?;
    let end = timestamp(a, a.end_ms)?;
    let elapsed = duration(a.end_ms - a.start_ms)?;
    let timer = duration(a.timer_ms())?;
    let mut w = Writer(Vec::new());
    w.definition(0, 0, &[(0, 1, 0), (1, 2, 0x84), (2, 2, 0x84), (4, 4, 0x86)]);
    w.u8(0);
    w.u8(4);
    w.u16(255);
    w.u16(0);
    w.u32(start); // development manufacturer, activity file
    w.definition(1, 21, &[(253, 4, 0x86), (0, 1, 0), (1, 1, 0), (4, 1, 2)]);
    w.definition(
        2,
        20,
        &[
            (253, 4, 0x86),
            (3, 1, 2),
            (4, 1, 2),
            (6, 2, 0x84),
            (7, 2, 0x84),
            (53, 1, 2),
            (73, 4, 0x86),
            (5, 4, 0x86),
            (0, 4, 0x85),
            (1, 4, 0x85),
            (2, 2, 0x84),
            (78, 4, 0x86),
        ],
    );
    // Stable, last notification per UTC second; no interpolation or averaging.
    let mut seconds = BTreeMap::new();
    let mut active_count = 0;
    let samples = a.fit_samples();
    for sample in &samples {
        if a.active(sample.ms) {
            seconds.insert(timestamp(a, sample.ms)?, sample);
            active_count += 1;
        }
    }
    let selected: Vec<_> = seconds.values().copied().collect();
    let mut timeline: Vec<(u64, Option<&Sample>, bool)> =
        selected.iter().map(|s| (s.ms, Some(*s), false)).collect();
    for i in &a.intervals {
        timeline.push((i.start_ms, None, true));
        timeline.push((i.end_ms, None, false));
    }
    if a.intervals.last().is_some_and(|i| i.end_ms != a.end_ms) {
        timeline.push((a.end_ms, None, false));
    }
    // At equal times: Start, sample, Stop (including a recovered terminal sample).
    timeline.sort_by_key(|(ms, sample, start)| {
        (
            *ms,
            if sample.is_some() {
                1
            } else if *start {
                0
            } else {
                2
            },
        )
    });
    let mut records = 0;
    let mut losses = 0;
    for (ms, sample, start_event) in timeline {
        let stamp = timestamp(a, ms)?;
        if let Some(s) = sample {
            let position = match (geography, s.virtual_distance_m) {
                (Some(g), Some(distance)) => Some(g.at(distance)?),
                _ => None,
            };
            records += usize::from(w.sample(stamp, s, position, &mut losses));
        } else {
            w.event(
                stamp,
                if start_event {
                    0
                } else if ms == a.end_ms {
                    4
                } else {
                    1
                },
            );
        }
    }
    if records == 0 {
        return Err("No representable running telemetry for FIT export".into());
    }
    w.definition(
        3,
        19,
        &[
            (254, 2, 0x84),
            (253, 4, 0x86),
            (0, 1, 0),
            (1, 1, 0),
            (2, 4, 0x86),
            (7, 4, 0x86),
            (8, 4, 0x86),
            (9, 4, 0x86),
            (25, 1, 0),
        ],
    );
    w.u8(3);
    w.u16(0);
    w.u32(end);
    w.0.extend([9, 1]);
    w.u32(start);
    w.u32(elapsed);
    w.u32(timer);
    w.u32(
        a.virtual_distance()
            .map_or(u32::MAX, |d| (d * 100.).round() as u32),
    );
    w.u8(2);
    w.definition(
        4,
        18,
        &[
            (254, 2, 0x84),
            (253, 4, 0x86),
            (0, 1, 0),
            (1, 1, 0),
            (2, 4, 0x86),
            (5, 1, 0),
            (6, 1, 0),
            (7, 4, 0x86),
            (8, 4, 0x86),
            (9, 4, 0x86),
            (25, 2, 0x84),
            (26, 2, 0x84),
        ],
    );
    w.u8(4);
    w.u16(0);
    w.u32(end);
    w.0.extend([8, 1]);
    w.u32(start);
    w.0.extend([2, if a.route_config.is_some() { 58 } else { 6 }]);
    w.u32(elapsed);
    w.u32(timer);
    w.u32(
        a.virtual_distance()
            .map_or(u32::MAX, |d| (d * 100.).round() as u32),
    );
    w.u16(0);
    w.u16(1);
    w.definition(
        5,
        34,
        &[
            (253, 4, 0x86),
            (0, 4, 0x86),
            (1, 2, 0x84),
            (2, 1, 0),
            (3, 1, 0),
            (4, 1, 0),
        ],
    );
    w.u8(5);
    w.u32(end);
    w.u32(timer);
    w.u16(1);
    w.0.extend([0, 26, 1]);
    let size = u32::try_from(w.0.len()).map_err(|_| "FIT exceeds size limit")?;
    let mut out = vec![14, 0x20];
    out.extend(21218u16.to_le_bytes());
    out.extend(size.to_le_bytes());
    out.extend(b".FIT");
    out.extend(crc(&out).to_le_bytes());
    out.extend(w.0);
    out.extend(crc(&out).to_le_bytes());
    let mut warnings=vec!["FIT uses the development manufacturer ID; UTC is preserved and no local timezone is invented.".into()];
    if active_count > selected.len() {
        warnings.push(format!(
            "FIT kept the last sample in each second; {} additional notifications remain in CSV.",
            active_count - selected.len()
        ));
    }
    if losses > 0 {
        warnings.push(format!("{losses} values outside FIT field ranges were encoded as missing; original values remain in CSV."));
    }
    if selected.len() > records {
        warnings.push(format!("{} FIT seconds had no representable sensor values and were omitted; notifications remain in CSV.",selected.len()-records));
    }
    Ok((out, records, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crc_matches_standard_check_vector_and_full_file_residue() {
        assert_eq!(crc(b"123456789"), 0xbb3d);
        let a = Activity {
            hr_config: None,
            hr_samples: vec![],
            route_config: None,
            virtual_states: vec![],
            utc_unix_ms: 1700000000000,
            samples: vec![crate::Sample {
                ms: 1000,
                virtual_distance_m: None,
                data: crate::Measurements {
                    power_w: Some(0),
                    ..Default::default()
                },
            }],
            intervals: vec![crate::Interval {
                start_ms: 0,
                end_ms: 2000,
            }],
            start_ms: 0,
            end_ms: 2000,
            grades: vec![],
            complete: true,
            recovered_running: false,
            discarded_partial_line: false,
            warnings: vec![],
        };
        let (mut file, _, _) = encode(&a).unwrap();
        assert_eq!(crc(&file[..14]), 0);
        assert_eq!(crc(&file), 0);
        file[30] ^= 1;
        assert_ne!(crc(&file), 0);
    }
}
