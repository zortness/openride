fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    if !(3..=4).contains(&args.len()) {
        eprintln!("Usage: activity-core SESSION_JSONL NEW_EXPORT_DIRECTORY [PACKAGE_MANIFEST]");
        std::process::exit(2);
    }
    match activity_core::export_with_manifest(
        std::path::Path::new(&args[1]),
        std::path::Path::new(&args[2]),
        args.get(3).map(std::path::Path::new),
    ) {
        Ok(report) => println!("{}", serde_json::to_string_pretty(&report).unwrap()),
        Err(error) => {
            eprintln!("Export failed: {error}");
            std::process::exit(1);
        }
    }
}
