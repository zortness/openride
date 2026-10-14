//! Thin Godot adapter. All route logic lives in ride-core.
use godot::classes::RefCounted;
use godot::prelude::*;
use ride_core::{extract_route, read_route, route_gpx, save_new, Library};
mod activity_export;
mod full_route;
mod source_review;
mod trainer;

struct OpenRideExtension;
#[gdextension]
unsafe impl ExtensionLibrary for OpenRideExtension {}

#[derive(GodotClass)]
#[class(base=RefCounted, init)]
struct RideInspector {
    base: Base<RefCounted>,
    pending: Option<std::sync::mpsc::Receiver<Library>>,
    library: Library,
}

#[godot_api]
impl RideInspector {
    #[func]
    fn start_import(&mut self, paths_json: GString) -> bool {
        if self.pending.is_some() {
            return false;
        }
        // Convert all engine values on the main thread. The worker owns plain Rust data.
        let paths = paths_json.to_string();
        let (send, receive) = std::sync::mpsc::channel();
        self.pending = Some(receive);
        std::thread::spawn(move || {
            let library = match serde_json::from_str::<Vec<String>>(&paths) {
                Ok(paths) => Library::import_paths(&paths),
                Err(e) => Library {
                    errors: vec![format!("Invalid file list: {e}")],
                    ..Default::default()
                },
            };
            let _ = send.send(library);
        });
        true
    }

    #[func]
    fn poll_import(&mut self) -> GString {
        let Some(receive) = &self.pending else {
            return GString::new();
        };
        self.library = match receive.try_recv() {
            Ok(library) => library,
            Err(std::sync::mpsc::TryRecvError::Empty) => return GString::new(),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Library {
                errors: vec!["Import worker ended without a result".into()],
                ..Default::default()
            },
        };
        self.pending = None;
        serde_json::to_string(&self.library)
            .expect("Library is serializable")
            .as_str()
            .into()
    }

    #[func]
    fn inspect_files(&self, paths_json: GString) -> GString {
        let result: Result<Vec<String>, _> = serde_json::from_str(&paths_json.to_string());
        let library = match result {
            Ok(paths) => Library::import_paths(&paths),
            Err(e) => Library {
                errors: vec![format!("Invalid file list: {e}")],
                ..Default::default()
            },
        };
        serde_json::to_string(&library)
            .unwrap_or_else(|e| format!("{{\"errors\":[\"{e}\"]}}"))
            .as_str()
            .into()
    }

    #[func]
    fn export_route(&self, source_hash: GString, start: i64, end: i64, path: GString) -> GString {
        let result = (|| -> Result<String, String> {
            // Keep original typed values; Godot JSON numbers lose integer types.
            let hash = source_hash.to_string();
            let a = self
                .library
                .groups
                .iter()
                .flat_map(|g| &g.sources)
                .find(|a| a.source_sha256 == hash)
                .ok_or("Source is not in the native library")?;
            let route = extract_route(
                a,
                usize::try_from(start).map_err(|_| "Invalid start")?,
                usize::try_from(end).map_err(|_| "Invalid end")?,
            )?;
            let path = path.to_string();
            let content = if path.to_lowercase().ends_with(".gpx") {
                route_gpx(&route)
            } else {
                serde_json::to_string_pretty(&route).map_err(|e| e.to_string())?
            };
            save_new(&path, &content)?;
            Ok(format!(
                "Saved {} route points. Original activity unchanged.",
                route.points.len()
            ))
        })();
        serde_json::json!({"ok":result.is_ok(),"message":result.unwrap_or_else(|e|e)})
            .to_string()
            .as_str()
            .into()
    }

    #[func]
    fn open_route(&self, path: GString) -> GString {
        let result = (|| -> Result<ride_core::RouteDocument, String> {
            let path = path.to_string();
            if std::fs::metadata(&path).map_err(|e| e.to_string())?.len() > 64 * 1024 * 1024 {
                return Err("Route exceeds 64 MiB limit".into());
            }
            read_route(&std::fs::read_to_string(path).map_err(|e| e.to_string())?)
        })();
        match result {
            Ok(route) => serde_json::json!({"ok":true,"route":route})
                .to_string()
                .as_str()
                .into(),
            Err(e) => serde_json::json!({"ok":false,"message":e})
                .to_string()
                .as_str()
                .into(),
        }
    }
}
mod simulation;
