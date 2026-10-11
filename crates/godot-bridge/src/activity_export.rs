use godot::{classes::RefCounted, prelude::*};
use std::sync::mpsc;

#[derive(GodotClass)]
#[class(base=RefCounted, init)]
struct ActivityExporter {
    base: Base<RefCounted>,
    pending: Option<mpsc::Receiver<serde_json::Value>>,
}
#[godot_api]
impl ActivityExporter {
    #[func]
    fn start_export(&mut self, source: GString, destination: GString, manifest: GString) -> bool {
        if self.pending.is_some() {
            return false;
        }
        let (tx, rx) = mpsc::channel();
        self.pending = Some(rx);
        let source = source.to_string();
        let destination = destination.to_string();
        let manifest = manifest.to_string();
        std::thread::spawn(move || {
            let result = match activity_core::export_with_manifest(
                std::path::Path::new(&source),
                std::path::Path::new(&destination),
                (!manifest.is_empty()).then(|| std::path::Path::new(&manifest)),
            ) {
                Ok(report) => serde_json::json!({"ok":true,"report":report}),
                Err(message) => serde_json::json!({"ok":false,"message":message}),
            };
            let _ = tx.send(result);
        });
        true
    }
    #[func]
    fn poll_export(&mut self) -> GString {
        let Some(rx) = &self.pending else {
            return GString::new();
        };
        let result = match rx.try_recv() {
            Ok(value) => value,
            Err(mpsc::TryRecvError::Empty) => return GString::new(),
            Err(mpsc::TryRecvError::Disconnected) => {
                serde_json::json!({"ok":false,"message":"Export worker stopped unexpectedly"})
            }
        };
        self.pending = None;
        result.to_string().as_str().into()
    }
}
