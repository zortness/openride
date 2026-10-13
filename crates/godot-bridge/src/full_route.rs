use godot::{classes::RefCounted, prelude::*};
use ride_core::full_route::Profile;
use serde_json::json;
use std::sync::mpsc;
type Loaded = (Profile, Vec<ride_core::route_repairs::Comparison>);

#[derive(GodotClass)]
#[class(base=RefCounted,init)]
struct FullRoutePreview {
    base: Base<RefCounted>,
    pending: Option<mpsc::Receiver<Result<Loaded, String>>>,
    profile: Option<Profile>,
}
#[godot_api]
impl FullRoutePreview {
    #[func]
    fn load_review(&mut self, directory: GString, revision: GString) -> bool {
        if self.pending.is_some() {
            return false;
        }
        let path = directory.to_string();
        let revision = revision.to_string();
        let (tx, rx) = mpsc::channel();
        self.pending = Some(rx);
        self.profile = None;
        std::thread::spawn(move || {
            let result =
                ride_core::source_review::open(std::path::Path::new(&path)).and_then(|review| {
                    if review.revision != revision {
                        return Err("Review changed on disk; reopen it before previewing".into());
                    }
                    Ok((
                        Profile::from_review(&review)?,
                        ride_core::route_repairs::comparisons(&review)?,
                    ))
                });
            let _ = tx.send(result);
        });
        true
    }
    #[func]
    fn decide_repair(
        &mut self,
        directory: GString,
        revision: GString,
        id: GString,
        status: GString,
    ) -> bool {
        if self.pending.is_some() {
            return false;
        }
        let (tx, rx) = mpsc::channel();
        self.pending = Some(rx);
        self.profile = None;
        let (path, revision, id, status) = (
            directory.to_string(),
            revision.to_string(),
            id.to_string(),
            status.to_string(),
        );
        std::thread::spawn(move || {
            let result = ride_core::route_repairs::decide(
                std::path::Path::new(&path),
                &revision,
                &id,
                &status,
            )
            .and_then(|review| {
                Ok((
                    Profile::from_review(&review)?,
                    ride_core::route_repairs::comparisons(&review)?,
                ))
            });
            let _ = tx.send(result);
        });
        true
    }
    #[func]
    fn poll(&mut self) -> GString {
        let Some(rx) = &self.pending else {
            return GString::new();
        };
        let result = match rx.try_recv() {
            Ok(v) => v,
            Err(mpsc::TryRecvError::Empty) => return GString::new(),
            Err(_) => Err("Profile worker stopped".into()),
        };
        self.pending = None;
        let value = match result {
            Ok((p, repairs)) => {
                let simulation_error = p.simulation().err();
                let stride = p.points.len().div_ceil(512);
                let mut overview = vec![];
                let mut broken = false;
                for (i, point) in p.points.iter().enumerate() {
                    broken |= point.break_before;
                    if i % stride == 0 || i + 1 == p.points.len() {
                        overview.push(json!({"route_distance_m":point.distance,"elevation_m":point.height,"break_before":broken}));
                        broken = false;
                    }
                }
                let v = json!({"ok":true,"length_m":p.length_m(),"point_count":p.points.len(),"bridged_pauses":p.bridged_pauses,"unresolved_breaks":p.unresolved_breaks,"issues":p.issues,"revision":p.revision,"simulation_error":simulation_error});
                let mut v = v;
                v["overview"] = json!(overview);
                v["repairs"] = json!(repairs);
                self.profile = Some(p);
                v
            }
            Err(e) => json!({"ok":false,"message":e}),
        };
        value.to_string().as_str().into()
    }
    #[func]
    fn window(&self, distance: f64) -> GString {
        let result = self
            .profile
            .as_ref()
            .ok_or("No profile loaded".to_string())
            .and_then(|p| p.window(distance));
        let value = match result {
            Ok(v) => json!({"ok":true,"window":v}),
            Err(e) => json!({"ok":false,"message":e}),
        };
        value.to_string().as_str().into()
    }
    #[func]
    fn advance(&self, from: f64, travel: f64) -> GString {
        let result = self
            .profile
            .as_ref()
            .ok_or("No profile loaded".to_string())
            .and_then(|p| p.advance(from, travel));
        let value = match result {
            Ok((d, stop)) => json!({"distance_m":d,"stopped":stop}),
            Err(e) => json!({"error":e}),
        };
        value.to_string().as_str().into()
    }
}
