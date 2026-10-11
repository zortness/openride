use crate::{control::Subscription, journal::Journal};
use serde_json::json;
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};
use trainer_core::{
    diagnostic::{resistance_request, CheckMode, SessionCheck, RESISTANCE_GRADES},
    ftms::{indoor_bike, ControlPoint, Features, Outcome, Request},
};
use windows::{
    core::{Error, Result, HRESULT},
    Devices::Bluetooth::{
        BluetoothConnectionStatus, BluetoothLEDevice,
        GenericAttributeProfile::{
            GattCharacteristic, GattClientCharacteristicConfigurationDescriptorValue as Cccd,
            GattCommunicationStatus, GattWriteOption,
        },
    },
    Storage::Streams::DataWriter,
};

fn error(message: impl ToString) -> Error {
    Error::new(HRESULT(0x80004005u32 as i32), message.to_string())
}
static USER_PAUSE: AtomicBool = AtomicBool::new(false);
pub static HEARTBEAT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub fn request_pause() {
    USER_PAUSE.store(true, Ordering::SeqCst);
}
struct Runtime<'a> {
    hr: Option<crate::ant_hr::Worker>,
    device: &'a BluetoothLEDevice,
    cp: &'a GattCharacteristic,
    machine: ControlPoint,
    policy: SessionCheck,
    rx: mpsc::Receiver<(u8, Vec<u8>, Instant)>,
    failed: Arc<AtomicBool>,
    clock: Instant,
    journal: Journal,
    speed: Option<(f64, u64)>,
    mode: CheckMode,
    ride: Option<ride_core::live::LiveRide>,
    grade: trainer_core::route_grade::RouteGrade,
    ride_record_ms: u64,
    run_ack: trainer_core::live::RunAcknowledgements,
}
impl Runtime<'_> {
    fn drain_hr(&mut self) -> Result<()> {
        loop {
            let row = self.hr.as_ref().and_then(|hr| hr.rx.try_recv().ok());
            match row {
                Some(row) => self.log(row)?,
                None => break,
            }
        }
        Ok(())
    }
    fn ms(&self) -> u64 {
        self.clock.elapsed().as_millis() as u64
    }
    fn log(&mut self, row: serde_json::Value) -> Result<()> {
        self.journal.record(self.ms(), row).map_err(error)
    }
    fn pump(&mut self) -> Result<Option<Outcome>> {
        HEARTBEAT.fetch_add(1, Ordering::Relaxed);
        self.drain_hr()?;
        self.machine.tick(self.ms()).map_err(error)?;
        if self.failed.load(Ordering::SeqCst) {
            return Err(error("Notification decode/queue failure"));
        }
        if self.device.ConnectionStatus()? != BluetoothConnectionStatus::Connected {
            return Err(error("Trainer disconnected"));
        }
        // Integrate using the previously received sample before applying new telemetry.
        let now = self.ms();
        if let Some(ride) = &mut self.ride {
            ride.tick(now);
        }
        let mut outcome = None;
        while let Ok((kind, bytes, received)) = self.rx.try_recv() {
            let ms = self.ms();
            match kind {
                0 => {
                    self.log(json!({"event":"control_indication","bytes":bytes}))?;
                    if let Some(result) = self.machine.indication(ms, &bytes).map_err(error)? {
                        outcome = Some(result);
                    }
                }
                1 => {
                    self.log(json!({"event":"machine_status","bytes":bytes}))?;
                    self.machine.machine_status(&bytes).map_err(error)?;
                    self.machine.tick(ms).map_err(error)?;
                    self.policy.status(&bytes).map_err(error)?;
                    if self.policy.interrupted() {
                        if let Some(ride) = &mut self.ride {
                            ride.pause(ms);
                        }
                    }
                }
                2 => {
                    let decoded = indoor_bike(&bytes).map_err(error)?;
                    let received_ms =
                        received.saturating_duration_since(self.clock).as_millis() as u64;
                    self.policy
                        .telemetry(received_ms, decoded.power_w.is_some());
                    if let Some(ride) = &mut self.ride {
                        ride.telemetry(received_ms, decoded.power_w.map(f64::from));
                        ride.tick(ms);
                    }
                    if let Some(speed) = decoded.speed_kmh {
                        self.speed = Some((speed, received_ms));
                    }
                    self.log(json!({"event":"indoor_bike_data","bytes":bytes,"decoded":{"Ok":decoded},"received_ms":received_ms}))?;
                }
                _ => return Err(error("Unexpected callback kind")),
            }
        }
        if self.ride.is_some() && self.ms().saturating_sub(self.ride_record_ms) >= 200 {
            self.record_ride()?;
        }
        Ok(outcome)
    }
    fn record_ride(&mut self) -> Result<()> {
        if let Some(ride) = &self.ride {
            let mut row = ride.snapshot();
            row["acknowledged_grade_percent"] = json!(f64::from(self.grade.acknowledged) / 100.);
            self.log(row)?;
            self.ride_record_ms = self.ms();
        }
        Ok(())
    }
    fn accepted(&mut self, outcome: Outcome, request: Request) -> Result<()> {
        match outcome {
            Outcome::Accepted(actual) if actual == request => {
                self.run_ack.accepted(request);
                if let Request::Simulation {
                    grade_hundredths, ..
                } = request
                {
                    self.grade.acknowledge(self.ms(), grade_hundredths);
                }
                self.log(json!({"event":"command_acknowledged","opcode":request.encode()[0],"bytes":request.encode()}))
            }
            Outcome::Rejected { code, .. } => {
                self.log(
                    json!({"event":"command_rejected","opcode":request.encode()[0],"result":code}),
                )?;
                Err(error(format!("FTMS rejected request: {code}")))
            }
            _ => Err(error("Unexpected command acknowledgement")),
        }
    }
    fn procedure(&mut self, request: Request) -> Result<()> {
        let positive_grade = matches!(
            request,
            Request::Simulation {
                grade_hundredths: 1..,
                ..
            }
        );
        let requires_fresh = request == Request::Start || positive_grade;
        if requires_fresh
            && (!self.policy.fresh(self.ms())
                || self.policy.interrupted()
                || self
                    .ride
                    .as_ref()
                    .is_some_and(|r| r.fresh_power(self.ms()).is_none()))
        {
            return Err(error(
                "Start/positive grade requires fresh power telemetry and no reported stop",
            ));
        }
        let (id, bytes) = self.machine.begin(self.ms(), request).map_err(error)?;
        // A journal failure returns before the hardware write is made.
        self.log(json!({"event":"command_requested","opcode":bytes[0],"bytes":bytes}))?;
        self.machine.tick(self.ms()).map_err(error)?;
        let now = self.ms();
        if let Some(ride) = &mut self.ride {
            ride.tick(now);
            if positive_grade && ride.status() != "running" {
                return Err(error("Route simulation interrupted before grade write"));
            }
        }
        if requires_fresh
            && (!self.policy.fresh(self.ms())
                || self
                    .ride
                    .as_ref()
                    .is_some_and(|r| r.fresh_power(self.ms()).is_none())
                || (positive_grade && !self.motion_allowed()))
        {
            return Err(error(
                "Fresh/stationary telemetry gate failed after recording intent; write not sent",
            ));
        }
        if self.ride.is_some()
            && (positive_grade || request == Request::Start)
            && crate::ui_control::INBOX.lock().unwrap().stop_pending()
        {
            return Err(error(
                "Stop requested before route control write; closing connection",
            ));
        }
        let writer = DataWriter::new()?;
        writer.WriteBytes(&bytes)?;
        let op = self.cp.WriteValueWithOptionAsync(
            &writer.DetachBuffer()?,
            GattWriteOption::WriteWithResponse,
        )?;
        let result = (|| {
            let mut written = false;
            loop {
                if let Some(outcome) = self.pump()? {
                    return self.accepted(outcome, request);
                }
                if !written && op.Status()?.0 != 0 {
                    let success = op.GetResults()? == GattCommunicationStatus::Success;
                    self.log(json!({"event":"command_write_complete","opcode":request.encode()[0],"success":success}))?;
                    written = true;
                    if let Some(outcome) = self
                        .machine
                        .write_complete(self.ms(), id, success)
                        .map_err(error)?
                    {
                        return self.accepted(outcome, request);
                    }
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        })();
        if result.is_err() {
            let _ = op.Cancel();
            self.machine.invalidate();
        }
        result
    }
    fn motion_allowed(&self) -> bool {
        self.mode.motion_allowed(self.ms(), self.speed)
    }
    fn live_state(
        &mut self,
        lifecycle: &trainer_core::live::Lifecycle,
        reason: &str,
    ) -> Result<()> {
        use trainer_core::live::State;
        self.log(json!({"event":"session_state", "state": match lifecycle.state { State::Running => "running", State::Paused => "paused", State::Finished => "finished" }, "resume_epoch":lifecycle.epoch, "start_acknowledged":self.run_ack.ever_started, "pause_acknowledged":self.run_ack.pause_acknowledged, "can_resume":lifecycle.can_resume(lifecycle.epoch, self.policy.fresh(self.ms()) && self.ride.as_ref().is_none_or(|r|r.fresh_power(self.ms()).is_some() && r.status()!="finished")), "reason":reason}))
    }
    fn flat_pause(&mut self) -> Result<()> {
        let now = self.ms();
        if let Some(ride) = &mut self.ride {
            ride.pause(now);
        }
        self.record_ride()?;
        if self.ride.is_some() {
            for request in self.run_ack.settle_requests() {
                self.procedure(request)?;
                if matches!(request, Request::Simulation { .. }) {
                    self.log(json!({"event":"flat_grade_restored","acknowledged":true}))?;
                }
            }
            if !self.run_ack.ever_started {
                self.log(json!({"event":"ready_without_start","pause_sent":false}))?;
            }
            Ok(())
        } else {
            self.procedure(resistance_request(0).map_err(error)?)?;
            self.log(json!({"event":"flat_grade_restored","acknowledged":true}))?;
            self.procedure(Request::Pause)
        }
    }
    fn live(&mut self) -> Result<()> {
        use crate::ui_control::{Command, INBOX};
        use trainer_core::live::{Lifecycle, State};
        self.procedure(Request::Acquire)?;
        let wait = self.ms();
        while !self.policy.fresh(self.ms()) {
            self.pump()?;
            if INBOX.lock().unwrap().finish_reason().is_some() {
                break;
            }
            if self.ms() - wait > 5000 {
                return Err(error("No fresh power telemetry; Start not sent"));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let mut lifecycle = Lifecycle::default();
        let initial_command = INBOX.lock().unwrap().pop();
        if self.ride.is_none()
            && INBOX.lock().unwrap().finish_reason().is_none()
            && initial_command.is_none()
        {
            self.procedure(resistance_request(0).map_err(error)?)?;
            if INBOX.lock().unwrap().stop_pending() {
                self.flat_pause()?;
                lifecycle.paused();
            } else {
                self.procedure(Request::Start)?;
                lifecycle.running();
            }
        } else {
            self.flat_pause()?;
            lifecycle.paused();
        }
        self.live_state(&lifecycle, "initial_state")?;
        self.record_ride()?;
        let mut step = 0;
        let mut step_since = self.ms();
        let mut state_since = self.ms();
        loop {
            self.pump()?;
            let finish = INBOX.lock().unwrap().finish_reason().or_else(|| {
                (self.ms() >= 40 * 60 * 1000)
                    .then_some("session_limit_40_minutes")
                    .or_else(|| {
                        self.ride
                            .as_ref()
                            .is_some_and(|r| r.status() == "finished")
                            .then_some("route_complete")
                    })
            });
            if let Some(reason) = finish {
                self.log(json!({"event":"finish_requested","reason":reason}))?;
                self.flat_pause()?;
                lifecycle.finish();
                self.live_state(&lifecycle, reason)?;
                let since = self.ms();
                while self.ms() - since < 5000 {
                    self.pump()?;
                    std::thread::sleep(Duration::from_millis(10));
                }
                return self.log(json!({"event":"session_check_complete","pause_acknowledged":self.run_ack.pause_acknowledged,"start_acknowledged":self.run_ack.ever_started,"grade_sent":true,"reason":reason}));
            }
            // A safety interruption wins over any queued Resume from the previous state.
            if lifecycle.state == State::Running {
                if let Some(reason) = self.policy.interruption_reason(self.ms()).or_else(|| {
                    self.ride
                        .as_ref()
                        .filter(|r| r.status() != "running")
                        .map(|_| "simulation_interrupted")
                }) {
                    self.flat_pause()?;
                    lifecycle.paused();
                    self.live_state(&lifecycle, reason)?;
                }
            }
            let command = INBOX.lock().unwrap().pop();
            match command {
                Some(Command::Pause) => {
                    self.log(json!({"event":"ui_command","command":"pause"}))?;
                    self.flat_pause()?;
                    lifecycle.paused();
                    self.live_state(&lifecycle, "user_pause")?;
                }
                Some(Command::Resume(epoch)) => {
                    self.log(
                        json!({"event":"ui_command","command":"resume","resume_epoch":epoch}),
                    )?;
                    if lifecycle.can_resume(epoch, self.policy.fresh(self.ms()))
                        && self.ride.as_ref().is_none_or(|r| {
                            r.fresh_power(self.ms()).is_some() && r.status() != "finished"
                        })
                    {
                        self.policy.explicit_resume(self.ms()).map_err(error)?;
                        self.procedure(resistance_request(0).map_err(error)?)?;
                        // A Finish arriving during the flat acknowledgement must prevent Start.
                        if INBOX.lock().unwrap().stop_pending() {
                            continue;
                        }
                        self.procedure(Request::Start)?;
                        let now = self.ms();
                        if let Some(ride) = &mut self.ride {
                            if !ride.resume(now) {
                                self.flat_pause()?;
                                lifecycle.paused();
                                self.live_state(&lifecycle, "power_lost_during_start")?;
                                continue;
                            }
                        }
                        lifecycle.running();
                        step = 0;
                        step_since = self.ms();
                        self.live_state(&lifecycle, "explicit_resume")?;
                    } else {
                        self.log(json!({"event":"resume_rejected","reason":"Resume requires the current paused state and fresh power"}))?;
                        self.live_state(&lifecycle, "resume_rejected")?;
                    }
                }
                None => (),
            }
            if self.ride.is_some()
                && lifecycle.state == State::Running
                && !INBOX.lock().unwrap().stop_pending()
            {
                let course = self.ride.as_ref().unwrap().grade_percent();
                if let Some(grade) = self.grade.target(self.ms(), course) {
                    self.log(json!({"event":"route_grade_target","course_grade_percent":course,"commanded_grade_percent":f64::from(grade)/100.}))?;
                    if !INBOX.lock().unwrap().stop_pending() {
                        self.procedure(
                            trainer_core::route_grade::RouteGrade::request(grade).map_err(error)?,
                        )?;
                    }
                }
            }
            if self.ride.is_none()
                && lifecycle.state == State::Running
                && !INBOX.lock().unwrap().stop_pending()
                && step < RESISTANCE_GRADES.len() - 1
                && self.ms() - step_since >= 10000
            {
                step += 1;
                self.procedure(resistance_request(RESISTANCE_GRADES[step]).map_err(error)?)?;
                step_since = self.ms();
                self.log(json!({"event":"grade_step_acknowledged","grade_percent":f64::from(RESISTANCE_GRADES[step])/100.0,"hold_seconds":10}))?;
            }
            if self.ms() - state_since >= 1000 {
                self.live_state(&lifecycle, "heartbeat")?;
                state_since = self.ms();
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn resistance_profile(&mut self) -> Result<&'static str> {
        for (index, grade) in RESISTANCE_GRADES.iter().enumerate() {
            if USER_PAUSE.load(Ordering::SeqCst) {
                return Ok("user_pause");
            }
            if index != 0 {
                self.pump()?;
                if let Some(reason) = self.policy.interruption_reason(self.ms()) {
                    return Ok(reason);
                }
                if !self.motion_allowed() {
                    return Ok("stationary_check_failed");
                }
                self.procedure(resistance_request(*grade).map_err(error)?)?;
            }
            self.log(
                json!({"event":"grade_step_acknowledged","grade_percent":f64::from(*grade)/100.0,"hold_seconds":self.mode.hold_ms()/1000}),
            )?;
            let since = self.ms();
            while self.ms() - since < self.mode.hold_ms() {
                self.pump()?;
                if USER_PAUSE.load(Ordering::SeqCst) {
                    return Ok("user_pause");
                }
                if let Some(reason) = self.policy.interruption_reason(self.ms()) {
                    return Ok(reason);
                }
                if !self.motion_allowed() {
                    return Ok("stationary_check_failed");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        Ok("scheduled_pause")
    }
    fn check(&mut self) -> Result<()> {
        if USER_PAUSE.load(Ordering::SeqCst) {
            return Err(error("Cancelled before control acquisition"));
        }
        let resistance = self.mode.resistance();
        self.procedure(Request::Acquire)?;
        let wait_since = self.ms();
        loop {
            self.pump()?;
            if self.policy.interrupted() {
                return Err(error("Device reported stop before Start"));
            }
            if self.policy.fresh(self.ms()) {
                break;
            }
            if self.ms() - wait_since >= 5000 {
                return Err(error("No fresh power telemetry; Start not sent"));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        if resistance {
            if !self.motion_allowed() {
                return Err(error(
                    "Stationary check requires fresh trainer speed <= 0.5 km/h; grade not sent",
                ));
            }
            self.procedure(resistance_request(0).map_err(error)?)?;
        }
        if USER_PAUSE.load(Ordering::SeqCst) {
            self.procedure(Request::Pause)?;
            return self.log(json!({"event":"session_check_complete","pause_acknowledged":true,"reason":"cancelled_before_start","grade_sent":resistance}));
        }
        self.procedure(Request::Start)?;
        self.policy.start_acknowledged(self.ms());
        let reason = if resistance {
            self.resistance_profile()?
        } else {
            loop {
                self.pump()?;
                if let Some(reason) = self.policy.pause_reason(self.ms()) {
                    break reason;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        };
        if resistance {
            self.procedure(resistance_request(0).map_err(error)?)?;
            self.log(json!({"event":"flat_grade_restored","acknowledged":true}))?;
        }
        self.log(json!({"event":"pause_reason","reason":reason}))?;
        self.procedure(Request::Pause)?;
        let pause_ms = self.ms();
        while self.ms() - pause_ms < 5000 {
            self.pump()?;
            std::thread::sleep(Duration::from_millis(10));
        }
        self.log(json!({"event":"session_check_complete","pause_acknowledged":true,"reason":reason,"grade_sent":resistance}))?;
        if reason != "scheduled_pause" && reason != "user_pause" {
            return Err(error(format!(
                "Check interrupted ({reason}); Pause was acknowledged"
            )));
        }
        Ok(())
    }
}

pub fn run(
    device: &BluetoothLEDevice,
    cp: &GattCharacteristic,
    status: &GattCharacteristic,
    bike: &GattCharacteristic,
    features: &Features,
    journal_path: &Path,
    mode: CheckMode,
) -> Result<()> {
    if mode.resistance() && !features.simulation {
        return Err(error("Trainer does not advertise FTMS simulation support"));
    }
    let route_path =
        std::env::args().find_map(|a| a.strip_prefix("--route-profile=").map(str::to_owned));
    let ride = if let Some(path) = route_path {
        use std::io::{Read, Write};
        if mode != CheckMode::Live {
            return Err(error("Route sessions require live control mode"));
        }
        let mut bytes = Vec::new();
        std::fs::File::open(&path)
            .map_err(error)?
            .take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(error)?;
        let ride = ride_core::live::LiveRide::parse(&bytes).map_err(error)?;
        let expected = std::env::args()
            .find_map(|a| {
                a.strip_prefix("--expected-route-sha256=")
                    .map(str::to_owned)
            })
            .ok_or_else(|| error("Expected route identity missing"))?;
        if ride.metadata()["route_sha256"] != expected {
            return Err(error("Route changed since preview was loaded"));
        }
        let directory = journal_path
            .parent()
            .ok_or_else(|| error("Session directory missing"))?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join("route-profile.json"))
            .map_err(error)?;
        file.write_all(&bytes).map_err(error)?;
        file.sync_all().map_err(error)?;
        // Preserve the exact origin/provenance for offline geographic exports.
        use sha2::{Digest, Sha256};
        let expected_package = std::env::args()
            .find_map(|a| a.strip_prefix("--package-sha256=").map(str::to_owned))
            .ok_or_else(|| error("Package identity missing"))?;
        let mut manifest = Vec::new();
        std::fs::File::open(std::path::Path::new(&path).with_file_name("manifest.json"))
            .map_err(error)?
            .take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut manifest)
            .map_err(error)?;
        if manifest.len() > 8 * 1024 * 1024
            || format!("{:x}", Sha256::digest(&manifest)) != expected_package
        {
            return Err(error("Package changed since preview was loaded"));
        }
        let mut saved = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join("package-manifest.json"))
            .map_err(error)?;
        saved.write_all(&manifest).map_err(error)?;
        saved.sync_all().map_err(error)?;
        Some(ride)
    } else {
        None
    };
    let clock = Instant::now();
    let journal = Journal::create(journal_path).map_err(error)?;
    let (tx, rx) = mpsc::sync_channel(128);
    let failed = Arc::new(AtomicBool::new(false));
    let mut runtime = Runtime {
        hr: None,
        device,
        cp,
        machine: ControlPoint::new(features),
        policy: SessionCheck::default(),
        rx,
        failed: failed.clone(),
        clock,
        journal,
        speed: None,
        mode,
        ride,
        grade: Default::default(),
        ride_record_ms: 0,
        run_ack: Default::default(),
    };
    let result = (|| {
        if let Some(raw) =
            std::env::args().find_map(|a| a.strip_prefix("--ant-hr-config=").map(str::to_owned))
        {
            let config: serde_json::Value = serde_json::from_str(&raw).map_err(error)?;
            let (number, transmission) = crate::ant_hr::identity(&config).map_err(error)?;
            runtime.log(json!({"event":"ant_hr_config","schema":1,"device_number":number,"device_type":120,"transmission_type":transmission,"freshness_ms":3000}))?;
            runtime.hr = Some(crate::ant_hr::Worker::start(config, clock));
        }
        if let Some(ride) = &runtime.ride {
            let mut metadata = ride.metadata();
            let package = std::env::args()
                .find_map(|a| a.strip_prefix("--package-sha256=").map(str::to_owned))
                .ok_or_else(|| error("Package identity missing"))?;
            if package.len() != 64 || !package.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(error("Invalid package identity"));
            }
            metadata["package_manifest_sha256"] = json!(package);
            runtime.log(metadata)?;
        }
        let _indications = Subscription::new(cp, Cccd::Indicate, 0, tx.clone(), failed.clone())?;
        let _status = Subscription::new(status, Cccd::Notify, 1, tx.clone(), failed.clone())?;
        let _bike = Subscription::new(bike, Cccd::Notify, 2, tx, failed)?;
        runtime.log(
            json!({"event":"session_check_ready","mode":mode.name(),"planned_paused_seconds":5}),
        )?;
        if mode == CheckMode::Live {
            runtime.live()
        } else {
            runtime.check()
        }
    })();
    if let Err(ref e) = result {
        runtime.machine.invalidate();
        let _ = runtime.log(json!({"event":"session_check_failed","message":e.to_string(),"action":"close_connection","trainer_state":"unconfirmed"}));
    }
    if let Some(hr) = &runtime.hr {
        if !hr.stop() {
            runtime.log(json!({"event":"ant_hr_stop_timeout","radio_reset_confirmed":false}))?;
        }
    }
    runtime.drain_hr()?;
    result
}
