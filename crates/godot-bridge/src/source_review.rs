use godot::{classes::RefCounted, prelude::*};
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
};

#[derive(GodotClass)]
#[class(base=RefCounted, init)]
struct FullRideReviewer {
    base: Base<RefCounted>,
    pending: Option<mpsc::Receiver<serde_json::Value>>,
}
#[godot_api]
impl FullRideReviewer {
    #[func]
    fn start_review(&mut self, request: GString) -> bool {
        if self.pending.is_some() {
            return false;
        }
        let raw = request.to_string();
        let (tx, rx) = mpsc::channel();
        self.pending = Some(rx);
        std::thread::spawn(move || {
            let result = (|| -> Result<ride_core::source_review::Review, String> {
                if raw.len() > 2 * 1024 * 1024 {
                    return Err("Review request exceeds size budget".into());
                }
                let request: serde_json::Value =
                    serde_json::from_str(&raw).map_err(|e| e.to_string())?;
                let directory = Path::new(
                    request["directory"]
                        .as_str()
                        .ok_or("Missing review directory")?,
                );
                match request["action"].as_str() {
                    Some("create") => {
                        let paths: Vec<PathBuf> = serde_json::from_value(request["paths"].clone())
                            .map_err(|e| e.to_string())?;
                        ride_core::source_review::create(directory, &paths)
                    }
                    Some("open") => ride_core::source_review::open(directory),
                    Some("save") => {
                        let mut document: ride_core::source_review::Document =
                            serde_json::from_value(request["document"].clone())
                                .map_err(|e| e.to_string())?;
                        // Gap/source editing cannot modify repairs. Preserve native f64 values;
                        // a Godot JSON round trip may round a proposal's exact elevations.
                        document.repairs =
                            ride_core::source_review::open(directory)?.document.repairs;
                        ride_core::source_review::save(
                            directory,
                            request["revision"].as_str().ok_or("Missing revision")?,
                            document,
                        )
                    }
                    _ => Err("Unsupported review action".into()),
                }
            })();
            let value = match result {
                Ok(review) => serde_json::json!({"ok":true,"review":review}),
                Err(message) => serde_json::json!({"ok":false,"message":message}),
            };
            let _ = tx.send(value);
        });
        true
    }
    #[func]
    fn poll_review(&mut self) -> GString {
        let Some(rx) = &self.pending else {
            return GString::new();
        };
        let value = match rx.try_recv() {
            Ok(value) => value,
            Err(mpsc::TryRecvError::Empty) => return GString::new(),
            Err(mpsc::TryRecvError::Disconnected) => {
                serde_json::json!({"ok":false,"message":"Review worker stopped unexpectedly"})
            }
        };
        self.pending = None;
        value.to_string().as_str().into()
    }
}
