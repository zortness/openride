//! Timing policy for an explicitly requested, bounded start/pause hardware check.
use crate::ftms::Request;
pub const RESISTANCE_GRADES: [i16; 5] = [0, 100, 200, 100, 0];
pub const RESISTANCE_HOLD_MS: u64 = 2000;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckMode {
    StartPause,
    Stationary,
    Pedaling,
    Live,
}
impl CheckMode {
    pub fn resistance(self) -> bool {
        self != Self::StartPause
    }
    pub fn hold_ms(self) -> u64 {
        if matches!(self, Self::Pedaling | Self::Live) {
            10000
        } else {
            RESISTANCE_HOLD_MS
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::StartPause => "start_pause",
            Self::Stationary => "stationary_resistance",
            Self::Pedaling => "pedaling_resistance",
            Self::Live => "persistent_session",
        }
    }
    pub fn motion_allowed(self, now: u64, speed: Option<(f64, u64)>) -> bool {
        self != Self::Stationary
            || speed.is_some_and(|(v, t)| v <= 0.5 && now >= t && now - t <= 3000)
    }
}
pub fn resistance_request(grade: i16) -> Result<Request, String> {
    if ![0, 100, 200].contains(&grade) {
        return Err("Target outside diagnostic profile".into());
    }
    Ok(Request::Simulation {
        wind_mm_s: 0,
        grade_hundredths: grade,
        crr: 40,
        cw: 51,
    })
}
#[derive(Debug, Default)]
pub struct SessionCheck {
    last_power_ms: Option<u64>,
    running_since: Option<u64>,
    interrupted: bool,
}
impl SessionCheck {
    pub fn explicit_resume(&mut self, now_ms: u64) -> Result<(), String> {
        if !self.fresh(now_ms) {
            return Err("Resume requires fresh power telemetry".into());
        }
        self.interrupted = false;
        self.running_since = None;
        Ok(())
    }
    pub fn telemetry(&mut self, received_ms: u64, power_present: bool) {
        if power_present {
            self.last_power_ms = Some(received_ms);
        }
    }
    pub fn fresh(&self, now_ms: u64) -> bool {
        self.last_power_ms
            .is_some_and(|t| now_ms >= t && now_ms - t <= 3000)
    }
    pub fn start_acknowledged(&mut self, now_ms: u64) {
        self.running_since = Some(now_ms);
    }
    pub fn status(&mut self, bytes: &[u8]) -> Result<(), String> {
        match bytes {
            [0x02, 1 | 2] | [0x03] => self.interrupted = true,
            [0x02, ..] | [0x03, ..] => return Err("Malformed stopped/paused status".into()),
            // 0x04 is a device report, never permission to start/resume the app.
            _ => (),
        }
        Ok(())
    }
    pub fn interrupted(&self) -> bool {
        self.interrupted
    }
    pub fn pause_reason(&self, now_ms: u64) -> Option<&'static str> {
        let start = self.running_since?;
        if let Some(reason) = self.interruption_reason(now_ms) {
            return Some(reason);
        }
        if now_ms.saturating_sub(start) >= 15000 {
            Some("scheduled_pause")
        } else {
            None
        }
    }
    pub fn interruption_reason(&self, now_ms: u64) -> Option<&'static str> {
        if self.interrupted {
            Some("device_reported_stop")
        } else if !self.fresh(now_ms) {
            Some("telemetry_stale")
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_resume_alone_clears_a_stop_with_fresh_power() {
        let mut d = SessionCheck::default();
        d.status(&[2, 1]).unwrap();
        assert!(d.explicit_resume(4000).is_err());
        assert!(d.interrupted());
        d.telemetry(4000, true);
        assert!(d.interrupted());
        d.explicit_resume(4001).unwrap();
        assert!(!d.interrupted());
        d.status(&[3]).unwrap();
        assert!(d.interrupted());
    }
    #[test]
    fn pedaling_allows_motion_without_bypassing_freshness_or_stops() {
        assert!(CheckMode::Pedaling.motion_allowed(2000, Some((30.0, 2000))));
        assert!(!CheckMode::Stationary.motion_allowed(2000, Some((30.0, 2000))));
        let mut d = SessionCheck::default();
        d.start_acknowledged(0);
        d.telemetry(20000, true);
        assert_eq!(d.interruption_reason(20000), None);
        assert_eq!(d.interruption_reason(23001), Some("telemetry_stale"));
        d.telemetry(24000, true);
        d.status(&[2, 1]).unwrap();
        assert_eq!(d.interruption_reason(24000), Some("device_reported_stop"));
        assert_eq!(
            CheckMode::Pedaling.hold_ms() * RESISTANCE_GRADES.len() as u64,
            50000
        );
    }
    #[test]
    fn resistance_profile_is_bounded_and_returns_flat() {
        assert_eq!(RESISTANCE_GRADES[0], 0);
        assert_eq!(RESISTANCE_GRADES[4], 0);
        for p in RESISTANCE_GRADES.windows(2) {
            assert!((p[1] - p[0]).abs() <= 100);
        }
        assert_eq!(
            resistance_request(200).unwrap().encode(),
            [0x11, 0, 0, 200, 0, 40, 51]
        );
        assert_eq!(
            resistance_request(0).unwrap().encode(),
            [0x11, 0, 0, 0, 0, 40, 51]
        );
        assert!(resistance_request(300).is_err());
    }
    #[test]
    fn zero_power_is_fresh_but_missing_power_does_not_refresh() {
        let mut d = SessionCheck::default();
        d.telemetry(0, true);
        d.start_acknowledged(0);
        d.telemetry(3000, false);
        assert_eq!(d.pause_reason(3001), Some("telemetry_stale"));
    }
    #[test]
    fn device_started_status_never_starts_the_diagnostic() {
        let mut d = SessionCheck::default();
        d.status(&[4]).unwrap();
        assert_eq!(d.pause_reason(20000), None);
        d.telemetry(20000, true);
        d.start_acknowledged(20000);
        d.telemetry(35000, true);
        assert_eq!(d.pause_reason(35000), Some("scheduled_pause"));
    }
    #[test]
    fn stop_latches_and_fresh_data_does_not_resume() {
        let mut d = SessionCheck::default();
        d.start_acknowledged(0);
        d.status(&[2, 2]).unwrap();
        d.telemetry(1000, true);
        assert!(d.interrupted());
        assert_eq!(d.pause_reason(1000), Some("device_reported_stop"));
        assert!(d.status(&[2]).is_err());
    }
}
