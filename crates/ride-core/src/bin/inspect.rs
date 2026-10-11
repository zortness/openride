use ride_core::{extract_route, import_file, read_route, route_gpx, save_new, Library};

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("import") => {
            let lib=Library::import_paths(&args[1..]);
            println!("{}",serde_json::to_string(&lib).map_err(|e|e.to_string())?);
            if !lib.errors.is_empty() {return Err("Some files could not be imported (see JSON errors)".into());}
        }
        Some("export") if args.len()==5 => {
            let activity=import_file(&args[1])?;
            let start=args[2].parse().map_err(|_|"Invalid start index")?;
            let end=args[3].parse().map_err(|_|"Invalid end index")?;
            let route=extract_route(&activity,start,end)?;
            let data=if args[4].ends_with(".gpx") {route_gpx(&route)} else {serde_json::to_string_pretty(&route).map_err(|e|e.to_string())?};
            save_new(&args[4],&data)?;
            println!("Exported {} route points",route.points.len());
        }
        Some("validate-route") if args.len()==2 => {
            let json=std::fs::read_to_string(&args[1]).map_err(|e|e.to_string())?;
            let route=read_route(&json)?;
            println!("Route schema {}: {} points, {:.3} km",route.schema_version,route.points.len(),route.points.last().unwrap().distance_m/1000.0);
        }
        _=>return Err("Usage: inspect import FILE... | export FILE START END OUTPUT.{orroute,gpx} | validate-route FILE.orroute".into()),
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
