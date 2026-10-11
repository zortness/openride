//! Process boundary: BLE and disk synchronization never run on Godot's thread.
use godot::{classes::RefCounted, prelude::*};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

static SESSION_OWNER: AtomicBool = AtomicBool::new(false);

#[derive(Default)]
struct Feed {
    rows: VecDeque<Value>,
    overflow: bool,
}
impl Feed {
    fn push(&mut self, row: Value) {
        if self.rows.len() == 512 {
            self.rows.pop_front();
            self.overflow = true;
        }
        self.rows.push_back(row);
    }
}

#[derive(GodotClass)]
#[class(base=RefCounted, init)]
struct TrainerSession {
    base: Base<RefCounted>,
    feed: Arc<Mutex<Feed>>,
    active: Arc<AtomicBool>,
    commands: Option<mpsc::Sender<String>>,
    hr_config: String,
    pairing_file: String,
}
impl Drop for TrainerSession {
    fn drop(&mut self) {
        self.commands.take(); // Closing the pipe requests flat/Pause in the child.
    }
}
#[godot_api]
impl TrainerSession {
    #[func]
    fn configure_pairing_path(&mut self, path: GString) {
        if !self.active.load(Ordering::SeqCst) {
            self.pairing_file = path.to_string();
        }
    }
    #[func]
    fn pairing_selection(&self) -> GString {
        let value = match trainer_core::pairing::load(std::path::Path::new(&self.pairing_file)) {
            Ok(pairing) => json!({"selection":pairing}),
            Err(error) => json!({"error":error}),
        };
        value.to_string().as_str().into()
    }
    #[func]
    fn save_pairing(&self, selection: GString) -> GString {
        if SESSION_OWNER.load(Ordering::SeqCst) {
            return "Finish the current ride or diagnostic before changing sources".into();
        }
        let result =
            trainer_core::pairing::Pairing::parse(&selection.to_string()).and_then(|pairing| {
                trainer_core::pairing::save(std::path::Path::new(&self.pairing_file), &pairing)
            });
        result.err().unwrap_or_default().as_str().into()
    }
    #[func]
    fn configure_heart_rate(&mut self, config: GString) {
        if !self.active.load(Ordering::SeqCst) {
            self.hr_config = config.to_string();
        }
    }
    #[func]
    fn start_session(&mut self, executable: GString, directory: GString) -> bool {
        self.start_worker(executable, directory, None, None)
    }
    /// Empty identity enumerates Windows-paired BLE devices. A selected identity
    /// inspects capabilities only, with no subscriptions or trainer commands.
    #[func]
    fn start_discovery(
        &mut self,
        executable: GString,
        directory: GString,
        identity: GString,
    ) -> bool {
        self.start_worker(executable, directory, None, Some(identity.to_string()))
    }
    #[func]
    fn start_route_session(
        &mut self,
        executable: GString,
        directory: GString,
        route: GString,
        expected_sha256: GString,
        package_sha256: GString,
    ) -> bool {
        self.start_worker(
            executable,
            directory,
            Some((
                route.to_string(),
                expected_sha256.to_string(),
                package_sha256.to_string(),
            )),
            None,
        )
    }
    #[func]
    fn pause_session(&self) {
        if let Some(tx) = &self.commands {
            let _ = tx.send("pause".into());
        }
    }
    #[func]
    fn shift_session(&self, epoch: i64, direction: i64) {
        if epoch < 0 || !matches!(direction, -1 | 1) { return; }
        if let Some(tx) = &self.commands {
            let _ = tx.send(format!("shift {epoch} {direction}"));
        }
    }
    #[func]
    fn resume_session(&self, epoch: i64) {
        if epoch < 0 {
            return;
        }
        if let Some(tx) = &self.commands {
            let _ = tx.send(format!("resume {epoch}"));
        }
    }
    #[func]
    fn finish_session(&self) {
        if let Some(tx) = &self.commands {
            let _ = tx.send("finish".into());
        }
    }
    #[func]
    fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }
    #[func]
    fn poll_session(&self) -> GString {
        let mut feed = self.feed.lock().unwrap();
        let mut rows: Vec<Value> = feed.rows.drain(..).collect();
        if std::mem::take(&mut feed.overflow) {
            rows.push(json!({"event":"display_overflow","message":"Display updates were lost; inspect the session journal"}));
        }
        serde_json::to_string(&rows).unwrap().as_str().into()
    }
}

impl TrainerSession {
    fn start_worker(
        &mut self,
        executable: GString,
        directory: GString,
        route: Option<(String, String, String)>,
        discovery: Option<String>,
    ) -> bool {
        if self.active.load(Ordering::SeqCst)
            || SESSION_OWNER
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
        {
            return false;
        }
        self.active.store(true, Ordering::SeqCst);
        *self.feed.lock().unwrap() = Feed::default();
        // Read once on the main thread. Every worker receives an immutable snapshot;
        // malformed/missing preferences never fall back to a device name.
        let pairing = if discovery.is_none() {
            Some(trainer_core::pairing::load(std::path::Path::new(
                &self.pairing_file,
            )))
        } else {
            None
        };
        let (tx, rx) = mpsc::channel();
        self.commands = Some(tx);
        let feed = self.feed.clone();
        let active = self.active.clone();
        let hr_config = self.hr_config.clone();
        let executable = executable.to_string();
        let directory = directory.to_string();
        std::thread::spawn(move || {
            let emit = |row| feed.lock().unwrap().push(row);
            let result = (|| -> Result<(), String> {
                let pairing = pairing.transpose()?;
                if pairing.as_ref().is_some_and(|p| p.heart_rate.is_some()) && !hr_config.is_empty() {
                    return Err("Choose Bluetooth OR ANT heart rate in ride setup, not both".into());
                }
                let directory = std::path::PathBuf::from(directory);
                std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
                let errors = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(directory.join("ble-errors.txt"))
                    .map_err(|e| e.to_string())?;
                let mut capture = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(directory.join("ble.jsonl"))
                    .map_err(|e| e.to_string())?;
                let mut command = Command::new(executable);
                if let Some(identity) = &discovery {
                    if identity.is_empty() {
                        command.arg("--discover");
                    } else {
                        command.arg("--inspect-id").arg(identity);
                    }
                } else {
                    let pairing = pairing.as_ref().ok_or("Trainer pairing missing")?;
                    command
                        .arg("--device-id")
                        .arg(&pairing.resistance.id)
                        .arg(format!(
                            "--pairing={}",
                            serde_json::to_string(pairing).map_err(|e| e.to_string())?
                        ))
                        .arg(format!(
                            "--live-session={}",
                            directory.join("session.jsonl").display()
                        ))
                        .arg("--ui-control");
                }
                command
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(errors);
                if let Some((path, expected, package)) = route {
                    command.arg(format!("--route-profile={path}"));
                    command.arg(format!("--expected-route-sha256={expected}"));
                    command.arg(format!("--package-sha256={package}"));
                }
                if discovery.is_none() && !hr_config.is_empty() {
                    command.arg(format!("--ant-hr-config={hr_config}"));
                }
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    command.creation_flags(0x08000000); // CREATE_NO_WINDOW
                }
                let mut child = command.spawn().map_err(|e| e.to_string())?;
                let mut input = child.stdin.take().ok_or("Missing control pipe")?;
                std::thread::spawn(move || {
                    // Keep the pipe open across Pause/Resume. EOF finishes the session.
                    while let Ok(command) = rx.recv() {
                        if writeln!(input, "{command}").is_err() || command == "finish" {
                            break;
                        }
                    }
                });
                emit(json!({"event":"worker_started"}));
                let stdout = child.stdout.take().ok_or("Missing output pipe")?;
                for line in BufReader::new(stdout).lines() {
                    let line = match line {
                        Ok(line) => line,
                        Err(e) => {
                            emit(json!({"event":"worker_error","message":e.to_string()}));
                            break;
                        }
                    };
                    // The controller owns the durable session journal. This is a diagnostic mirror.
                    if let Err(e) = writeln!(capture, "{line}") {
                        emit(json!({"event":"capture_error","message":e.to_string()}));
                    }
                    if let Ok(row) = serde_json::from_str(&line) {
                        emit(row);
                    }
                }
                let status = child.wait().map_err(|e| e.to_string())?;
                emit(
                    json!({"event":"worker_exited","success":status.success(),"code":status.code()}),
                );
                Ok(())
            })();
            if let Err(message) = result {
                emit(json!({"event":"worker_error","message":message}));
            }
            SESSION_OWNER.store(false, Ordering::SeqCst);
            active.store(false, Ordering::SeqCst);
        });
        true
    }
}
