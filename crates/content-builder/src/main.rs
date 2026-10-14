use std::path::Path;

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() == 3 && args[0] == "--review-audit" {
        let review = ride_core::source_review::open(Path::new(&args[1]))?;
        let profile = ride_core::full_route::Profile::from_review(&review)?;
        let trials = [150., 200., 250.]
            .into_iter()
            .map(|power| ride_core::route_audit::rehearse(&profile, power, 86400))
            .collect::<Result<Vec<_>, _>>()?;
        let report = serde_json::json!({
            "kind":"openride.route-audit", "schema_version":1,
            "review_revision":review.revision,"geometry_source":review.document.geometry_source,
            "length_m":profile.length_m(),"point_count":profile.points.len(),
            "assumptions":{"total_mass_kg":85,"physics":"Simulation default configuration; constant power; 20 ms physics ticks", "time_limit_seconds":86400},
            "limitations":"Software profile rehearsal only; no scenery, hardware, endurance or steep-grade acceptance. Context grades do not replace raw grades.",
            "steep_findings":ride_core::route_audit::steep_findings(&profile),"rehearsals":trials
        });
        ride_core::save_new(
            &args[2],
            &serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?,
        )?;
        for trial in trials {
            println!(
                "{} W: {} at {:.3} km after {:.1} simulated minutes",
                trial.power_w,
                trial.status,
                trial.distance_m / 1000.,
                trial.simulated_seconds / 60.
            );
        }
        return Ok(());
    }
    if args.len() == 3 && args[0] == "--repair-review" {
        let directory = Path::new(&args[1]);
        let review = ride_core::source_review::open(directory)?;
        let saved = ride_core::source_review::save(directory, &args[2], review.document)?;
        println!("Saved automatic repair review revision {}", saved.revision);
        return Ok(());
    }
    if args.len() == 3 && args[0] == "--review-repairs" {
        let review = ride_core::source_review::open(Path::new(&args[1]))?;
        let repairs = ride_core::route_repairs::comparisons(&review)?;
        ride_core::save_new(
            &args[2],
            &serde_json::to_string_pretty(&serde_json::json!({
                "review_revision":review.revision,"repairs":repairs
            }))
            .map_err(|e| e.to_string())?,
        )?;
        println!(
            "Exported {} repair comparisons; saved review unchanged",
            repairs.len()
        );
        return Ok(());
    }
    if args.len() == 3 && args[0] == "--review-profile" {
        let review = ride_core::source_review::open(Path::new(&args[1]))?;
        let profile = ride_core::full_route::Profile::from_review(&review)?;
        let points:Vec<_>=profile.points.iter().map(|p|serde_json::json!({"source_sample":p.sample,"distance_m":p.distance,"latitude":p.lat,"longitude":p.lon,"elevation_m":p.height,"break_before":p.break_before})).collect();
        let output = serde_json::json!({"kind":"openride.reviewed-profile","schema_version":1,"trainer_ready":false,"review_revision":review.revision,"sources":review.document.sources,"geometry_source":review.document.geometry_source,"elevation_source":review.document.elevation_source,"name":review.activities[0].name,"points":points,"issues":profile.issues,"bridged_pauses":profile.bridged_pauses,"unresolved_breaks":profile.unresolved_breaks});
        ride_core::save_new(
            &args[2],
            &serde_json::to_string(&output).map_err(|e| e.to_string())?,
        )?;
        println!(
            "Reviewed profile: {:.3} km; {} retained breaks",
            profile.length_m() / 1000.,
            profile.unresolved_breaks
        );
        return Ok(());
    }
    if args.len() == 3 && args[0] == "--import-route" {
        let activity = ride_core::import_file(Path::new(&args[1]))?;
        let route = ride_core::extract_route(
            &activity,
            0,
            activity
                .samples
                .len()
                .checked_sub(1)
                .ok_or("Empty activity")?,
        )?;
        ride_core::save_new(
            &args[2],
            &serde_json::to_string_pretty(&route).map_err(|e| e.to_string())?,
        )?;
        println!(
            "Imported {} route points from {}",
            route.points.len(),
            activity.format
        );
        return Ok(());
    }
    if args.len() == 3 && args[0] == "--activity-evidence" {
        use std::io::Write;
        let activity = ride_core::import_file(Path::new(&args[1]))?;
        let samples: Vec<_> = activity
            .samples
            .iter()
            .enumerate()
            .map(|(index, s)| {
                serde_json::json!({
                    "source_sample":index,"time_ms":s.time_ms,"speed_mps":s.speed_mps,
                    "latitude":s.latitude,"longitude":s.longitude,"break_before":s.break_before
                })
            })
            .collect();
        let evidence = serde_json::json!({"kind":"openride.activity-evidence","schema_version":1,
            "source_activity_sha256":activity.source_sha256,"samples":samples,
            "speed_provenance":"Recorded field; sensor identity unknown. Null means absent, not zero."});
        let bytes = serde_json::to_vec(&evidence).map_err(|e| e.to_string())?;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&args[2])
            .map_err(|e| e.to_string())?
            .write_all(&bytes)
            .map_err(|e| e.to_string())?;
        println!(
            "Exported {} source-bound timing/movement samples",
            samples.len()
        );
        return Ok(());
    }
    if args.len() == 4 && args[0] == "--terrain" {
        let report = content_builder::terrain::attach(
            Path::new(&args[1]),
            Path::new(&args[2]),
            Path::new(&args[3]),
        )?;
        println!(
            "Terrain preparation: {} x {} pixels; {} missing pixels.\n{}",
            report["terrain"]["width"],
            report["terrain"]["height"],
            report["terrain"]["nodata_pixels"],
            args[3]
        );
        return Ok(());
    }
    if args.len() != 4 {
        return Err(
            "Usage: content-builder SOURCE.orroute NEW_DIRECTORY START_METRES END_METRES\n       content-builder --terrain PREPARATION.json SOURCES_DIRECTORY NEW_DIRECTORY".into(),
        );
    }
    let start = args[2].parse().map_err(|_| "Invalid start distance")?;
    let end = args[3].parse().map_err(|_| "Invalid end distance")?;
    let report =
        content_builder::write_bundle(Path::new(&args[0]), Path::new(&args[1]), start, end)?;
    println!(
        "Prepared {} points; {} findings. Preview only, trainer_ready=false.\n{}",
        report["points"].as_array().unwrap().len(),
        report["findings"].as_array().unwrap().len(),
        args[1]
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
