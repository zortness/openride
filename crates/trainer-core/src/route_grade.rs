use crate::ftms::Request;
/// Initial supervised envelope: only the previously tested 0..2% range.
#[derive(Default)]
pub struct RouteGrade {
    pub acknowledged: i16,
    last_ack_ms: u64,
    gear_offset: i8,
}
impl RouteGrade {
    pub fn gear(&self) -> i8 {
        self.gear_offset + 5
    }
    /// Prototype load steps, not native transmission ratios. Neutral preserves
    /// existing terrain behavior; clipping and acknowledgement pacing still apply.
    pub fn shift(&mut self, direction: i8) -> bool {
        if !matches!(direction, -1 | 1) {
            return false;
        }
        let next = (self.gear_offset + direction).clamp(-4, 4);
        let changed = next != self.gear_offset;
        self.gear_offset = next;
        changed
    }
    pub fn load_percent(&self, course: f64) -> f64 {
        (course.clamp(0., 2.) + f64::from(self.gear_offset) * 0.25).clamp(0., 2.)
    }
    pub fn target(&self, now: u64, course: f64) -> Option<i16> {
        if !course.is_finite() || now.saturating_sub(self.last_ack_ms) < 1000 {
            return None;
        }
        let target = ((self.load_percent(course) * 100.).round() as i16)
            .clamp(self.acknowledged - 100, self.acknowledged + 100);
        (target != self.acknowledged).then_some(target)
    }
    pub fn acknowledge(&mut self, now: u64, grade: i16) {
        self.acknowledged = grade;
        self.last_ack_ms = now;
    }
    pub fn request(grade: i16) -> Result<Request, String> {
        if !(0..=200).contains(&grade) {
            return Err("Grade outside initial route envelope".into());
        }
        Ok(Request::Simulation {
            wind_mm_s: 0,
            grade_hundredths: grade,
            crr: 40,
            cw: 51,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shifting_adjusts_bounded_load_without_bypassing_ack_pacing() {
        let mut g = RouteGrade::default();
        assert_eq!(g.gear(), 5);
        assert!(g.shift(1));
        assert_eq!(g.gear(), 6);
        assert_eq!(g.target(1000, 0.5), Some(75));
        g.acknowledge(1000, 75);
        assert_eq!(g.target(1999, 0.5), None);
        for _ in 0..20 {
            g.shift(1);
        }
        assert_eq!(g.gear(), 9);
        assert!(!g.shift(1));
        assert!(!g.shift(2));
        assert_eq!(g.target(2000, 20.), Some(175));
        for _ in 0..20 {
            g.shift(-1);
        }
        assert_eq!(g.gear(), 1);
        assert_eq!(g.target(2000, -20.), Some(0));
    }
    #[test]
    fn course_grade_is_bounded_and_changes_wait_for_acknowledgement() {
        let mut g = RouteGrade::default();
        assert_eq!(g.target(999, 12.), None);
        assert_eq!(g.target(1000, 12.), Some(100));
        g.acknowledge(1100, 100);
        assert_eq!(g.target(2000, 12.), None);
        assert_eq!(g.target(2100, 12.), Some(200));
        g.acknowledge(2200, 200);
        assert_eq!(g.target(3200, -8.), Some(100));
        assert!(RouteGrade::request(-1).is_err());
        assert!(RouteGrade::request(201).is_err());
        assert_eq!(
            RouteGrade::request(123).unwrap().encode(),
            [17, 0, 0, 123, 0, 40, 51]
        );
    }
}
