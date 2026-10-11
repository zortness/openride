//! Process boundary: BLE and disk synchronization never run on Godot's thread.
use godot::{classes::RefCounted, prelude::*};
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
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
}
impl Drop for TrainerSession {
    fn drop(&mut self) {
        self.commands.take(); // Closing the pipe requests flat/Pause in the child.
    }
}
#[godot_api]
impl TrainerSession {
    #[func]
    fn configure_heart_rate(&mut self, config: GString) {
        if !self.active.load(Ordering::SeqCst) {
            self.hr_config = config.to_string();
        }
    }
    #[func]
    fn start_session(&mut self, executable: GString, directory: GString) -> bool {
        self.start_worker(executable, directory, None)
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
        )
    }
    #[func]
    fn pause_session(&self) {
        if let Some(tx) = &self.commands {
            let _ = tx.send("pause".into());
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
                command
                    .arg("SUITO")
                    .arg(format!(
                        "--live-session={}",
                        directory.join("session.jsonl").display()
                    ))
                    .arg("--ui-control")
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(errors);
                if let Some((path, expected, package)) = route {
                    command.arg(format!("--route-profile={path}"));
                    command.arg(format!("--expected-route-sha256={expected}"));
                    command.arg(format!("--package-sha256={package}"));
                }
                if !hr_config.is_empty() {
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
