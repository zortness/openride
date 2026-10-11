//! Fixed 20 ms forward-only cycling model, independent of rendering and hardware.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub total_mass_kg: f64,
    pub rolling_coefficient: f64,
    pub drag_area_m2: f64,
    pub air_density: f64,
    pub drivetrain_efficiency: f64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            total_mass_kg: 85.,
            rolling_coefficient: 0.004,
            drag_area_m2: 0.32,
            air_density: 1.225,
            drivetrain_efficiency: 0.97,
        }
    }
}
pub struct Simulation {
    profile: Vec<(f64, f64)>,
    config: Config,
    distance: f64,
    speed: f64,
    pending_ms: u64,
    running: bool,
    status: &'static str,
}
impl Simulation {
    pub fn new(profile: Vec<(f64, f64)>, config: Config) -> Result<Self, String> {
        if profile.len() < 2
            || profile.len() > 10000
            || profile[0].0 != 0.
            || profile.iter().any(|p| !p.0.is_finite() || !p.1.is_finite())
            || profile
                .windows(2)
                .any(|p| p[1].0 <= p[0].0 || ((p[1].1 - p[0].1) / (p[1].0 - p[0].0)).abs() > 1.)
        {
            return Err("Invalid continuous simulation profile".into());
        }
        if !config.total_mass_kg.is_finite()
            || !(30. ..=250.).contains(&config.total_mass_kg)
            || !config.rolling_coefficient.is_finite()
            || !(0. ..=0.05).contains(&config.rolling_coefficient)
            || !config.drag_area_m2.is_finite()
            || !(0.05..=2.).contains(&config.drag_area_m2)
            || !config.air_density.is_finite()
            || !(0.5..=2.).contains(&config.air_density)
            || !config.drivetrain_efficiency.is_finite()
            || !(0.5..=1.).contains(&config.drivetrain_efficiency)
        {
            return Err("Invalid simulation parameters".into());
        }
        Ok(Self {
            profile,
            config,
            distance: 0.,
            speed: 0.,
            pending_ms: 0,
            running: false,
            status: "paused",
        })
    }
    pub fn distance_m(&self) -> f64 {
        self.distance
    }
    pub fn speed_mps(&self) -> f64 {
        self.speed
    }
    pub fn status(&self) -> &str {
        self.status
    }
    pub fn resume(&mut self) {
        if self.distance < self.profile.last().unwrap().0 {
            self.running = true;
            self.status = "running";
        }
    }
    pub fn pause(&mut self) {
        self.running = false;
        self.pending_ms = 0;
        self.status = "paused";
    }
    pub fn reset(&mut self) {
        self.distance = 0.;
        self.speed = 0.;
        self.pause();
    }
    pub fn grade_percent(&self) -> f64 {
        let i = self
            .profile
            .partition_point(|p| p.0 < self.distance)
            .clamp(1, self.profile.len() - 1);
        let (a, b) = (self.profile[i - 1], self.profile[i]);
        100. * (b.1 - a.1) / (b.0 - a.0)
    }
    pub fn advance(&mut self, elapsed_ms: u64, power_w: Option<f64>) -> Result<(), String> {
        if let Some(power) = power_w {
            if !power.is_finite() || !(0. ..=3000.).contains(&power) {
                self.pause();
                return Err("Power must be finite and between 0 and 3000 W".into());
            }
        }
        if !self.running {
            return Ok(());
        }
        if elapsed_ms > 250 {
            self.pause();
            self.status = "timing_gap";
            return Ok(());
        }
        let Some(power) = power_w else {
            self.pause();
            self.status = "power_missing";
            return Ok(());
        };
        self.pending_ms += elapsed_ms;
        while self.pending_ms >= 20 && self.running {
            self.pending_ms -= 20;
            let slope = (self.grade_percent() / 100.).atan();
            let weight = self.config.total_mass_kg * 9.80665;
            let resistance = weight * (slope.sin() + self.config.rolling_coefficient * slope.cos())
                + 0.5
                    * self.config.air_density
                    * self.config.drag_area_m2
                    * self.speed
                    * self.speed;
            // Regularize power/velocity at launch; cap forward acceleration at 3 m/s^2.
            let thrust = power * self.config.drivetrain_efficiency / self.speed.max(1.);
            let acceleration = ((thrust - resistance) / self.config.total_mass_kg).min(3.);
            let next = (self.speed + acceleration * 0.02).clamp(0., 50.);
            self.distance += (self.speed + next) * 0.5 * 0.02 * slope.cos();
            self.speed = next;
            if self.distance >= self.profile.last().unwrap().0 {
                self.distance = self.profile.last().unwrap().0;
                self.speed = 0.;
                self.running = false;
                self.pending_ms = 0;
                self.status = "finished";
            }
        }
        Ok(())
    }
}
