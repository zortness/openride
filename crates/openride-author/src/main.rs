use openride_author::{Error, Snapshot};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

fn emit(value: serde_json::Value) {
    println!("{value}");
}
fn run(args: &[String]) -> Result<Snapshot, Error> {
    let usage = || {
        Error {code:"usage",message:"Usage: openride-author project ACTION FOLDER [OPTIONS]. Run openride-author --help for commands and options.".into()}
    };
    if args.len() < 3 || args[0] != "project" {
        return Err(usage());
    }
    let action = args[1].as_str();
    if ![
        "create",
        "open",
        "status",
        "rename",
        "import",
        "select-sources",
        "repair",
        "plan-maps",
        "acquire",
        "build-course",
        "infer-roads",
        "reset-roads",
        "choose-road",
        "edit-course",
        "archive-edits",
        "build-package",
        "export-package",
        "edit-scenery",
    ]
    .contains(&action)
    {
        return Err(usage());
    }
    let mut options = BTreeMap::new();
    let mut sources = Vec::new();
    for pair in args[3..].chunks(2) {
        if pair.len() == 2 && pair[0] == "--source" && action == "import" {
            sources.push(PathBuf::from(&pair[1]));
            continue;
        }
        if pair.len() != 2
            || ![
                "--name",
                "--expected-revision",
                "--cancel-file",
                "--geometry",
                "--elevation",
                "--repair",
                "--status",
                "--layer",
                "--max-tiles",
                "--sample",
                "--mode",
                "--latitude",
                "--longitude",
                "--way",
                "--scope",
                "--accept-steep",
                "--feature",
                "--feature-request",
                "--id",
            ]
            .contains(&pair[0].as_str())
            || options.insert(pair[0].as_str(), pair[1].as_str()).is_some()
        {
            return Err(usage());
        }
    }
    if options.get("--scope").is_some_and(|v| {
        action != "choose-road" || *v != "nearby" || options.get("--mode") == Some(&"undo")
    }) || options.contains_key("--name") && !["create", "rename"].contains(&action)
        || options.contains_key("--expected-revision")
            && ![
                "rename",
                "import",
                "select-sources",
                "repair",
                "plan-maps",
                "acquire",
                "build-course",
                "infer-roads",
                "reset-roads",
                "choose-road",
                "edit-course",
                "archive-edits",
                "build-package",
                "export-package",
                "edit-scenery",
            ]
            .contains(&action)
        || (options.contains_key("--geometry") || options.contains_key("--elevation"))
            && action != "select-sources"
        || (options.contains_key("--repair") || options.contains_key("--status"))
            && action != "repair"
        || (options.contains_key("--layer") || options.contains_key("--max-tiles"))
            && action != "acquire"
        || ["--sample", "--mode", "--latitude", "--longitude", "--way"]
            .iter()
            .any(|key| options.contains_key(key))
            && !["edit-course", "edit-scenery", "choose-road"].contains(&action)
        || ["--feature", "--feature-request", "--id"]
            .iter()
            .any(|key| options.contains_key(key))
            && action != "edit-scenery"
    {
        return Err(usage());
    }
    if options
        .get("--accept-steep")
        .is_some_and(|v| action != "build-package" || !["true", "false"].contains(v))
    {
        return Err(usage());
    }
    let root = Path::new(&args[2]);
    let cancel = options.get("--cancel-file").map(PathBuf::from);
    if cancel.as_ref().is_some_and(|p| p.exists()) {
        return Err(Error {
            code: "cancelled",
            message: "Operation cancelled before starting".into(),
        });
    }
    emit(json!({"protocol_version":1,"event":"progress","action":action,"phase":"validating"}));
    match action {
        "edit-scenery" => {
            let request = if let Some(expected) = options.get("--feature-request") {
                use std::io::Read;
                let mut raw = vec![];
                std::fs::File::open(root.join("jobs/scenery-edit.json"))?
                    .take(16 * 1024 * 1024 + 1)
                    .read_to_end(&mut raw)?;
                if raw.len() > 16 * 1024 * 1024
                    || format!("{:x}", Sha256::digest(&raw)) != *expected
                    || options.contains_key("--feature")
                {
                    return Err(Error {
                        code: "invalid_scenery_request",
                        message: "Scenery draft changed before saving; retry the edit".into(),
                    });
                }
                Some(serde_json::from_slice(&raw).map_err(|_| usage())?)
            } else {
                options
                    .get("--feature")
                    .map(|raw| serde_json::from_str(raw))
                    .transpose()
                    .map_err(|_| usage())?
            };
            openride_author::scenery::edit(
                root,
                options.get("--expected-revision").ok_or_else(usage)?,
                options.get("--mode").ok_or_else(usage)?,
                options.get("--id").ok_or_else(usage)?,
                request,
                cancel.as_deref(),
            )
        }
        "export-package" => openride_author::assembly::export(
            root,
            options.get("--expected-revision").ok_or_else(usage)?,
            cancel.as_deref(),
        ),
        "build-package" => openride_author::assembly::build(
            root,
            options.get("--expected-revision").ok_or_else(usage)?,
            options.get("--accept-steep").is_some_and(|v| *v == "true"),
            cancel.as_deref(),
        ),
        "archive-edits" => openride_author::corrections::archive(
            root,
            options.get("--expected-revision").ok_or_else(usage)?,
            cancel.as_deref(),
        ),
        "edit-course" => {
            let latitude = options
                .get("--latitude")
                .map(|s| s.parse::<f64>())
                .transpose()
                .map_err(|_| usage())?;
            let longitude = options
                .get("--longitude")
                .map(|s| s.parse::<f64>())
                .transpose()
                .map_err(|_| usage())?;
            if latitude.is_some() != longitude.is_some() {
                return Err(usage());
            }
            openride_author::corrections::decide(
                root,
                options.get("--expected-revision").ok_or_else(usage)?,
                openride_author::corrections::Edit {
                    source_sample: options
                        .get("--sample")
                        .ok_or_else(usage)?
                        .parse()
                        .map_err(|_| usage())?,
                    mode: options.get("--mode").ok_or_else(usage)?.to_string(),
                    position: latitude.zip(longitude).map(|(a, b)| [a, b]),
                    way: options
                        .get("--way")
                        .map(|s| s.parse())
                        .transpose()
                        .map_err(|_| usage())?,
                },
                cancel.as_deref(),
            )
        }
        "reset-roads" => openride_author::road_course::reset(
            root,
            options.get("--expected-revision").ok_or_else(usage)?,
            cancel.as_deref(),
        ),
        "infer-roads" => openride_author::road_course::build(
            root,
            options.get("--expected-revision").ok_or_else(usage)?,
            None,
            cancel.as_deref(),
        ),
        "choose-road" => {
            let sample = options
                .get("--sample")
                .ok_or_else(usage)?
                .parse()
                .map_err(|_| usage())?;
            if let Some(mode) = options
                .get("--mode")
                .filter(|m| ["connect", "undo-connection"].contains(m))
            {
                return openride_author::road_course::connect(
                    root,
                    options.get("--expected-revision").ok_or_else(usage)?,
                    sample,
                    *mode == "undo-connection",
                    cancel.as_deref(),
                );
            }
            let way = if options.get("--mode") == Some(&"undo") {
                None
            } else {
                Some(
                    options
                        .get("--way")
                        .ok_or_else(usage)?
                        .parse()
                        .map_err(|_| usage())?,
                )
            };
            if options.get("--scope") == Some(&"nearby") {
                return openride_author::road_course::choose_nearby(
                    root,
                    options.get("--expected-revision").ok_or_else(usage)?,
                    sample,
                    way.ok_or_else(usage)?,
                    cancel.as_deref(),
                );
            }
            openride_author::road_course::build(
                root,
                options.get("--expected-revision").ok_or_else(usage)?,
                Some((sample, way)),
                cancel.as_deref(),
            )
        }
        "build-course" => openride_author::course::build(
            root,
            options.get("--expected-revision").ok_or_else(usage)?,
            cancel.as_deref(),
        ),
        "plan-maps" => openride_author::maps::save_plan(
            root,
            options.get("--expected-revision").ok_or_else(usage)?,
            cancel.as_deref(),
        ),
        "acquire" => openride_author::maps::acquire(
            root,
            options.get("--expected-revision").ok_or_else(usage)?,
            options.get("--layer").ok_or_else(usage)?,
            options
                .get("--max-tiles")
                .map(|s| s.parse::<usize>())
                .transpose()
                .map_err(|_| usage())?
                .unwrap_or(1024),
            cancel.as_deref(),
        ),
        "repair" => openride_author::repairs::decide(
            root,
            options.get("--expected-revision").ok_or_else(usage)?,
            options.get("--repair").ok_or_else(usage)?,
            options.get("--status").ok_or_else(usage)?,
            cancel.as_deref(),
        ),
        "import" => openride_author::import(
            root,
            options.get("--expected-revision").ok_or_else(usage)?,
            &sources,
            cancel.as_deref(),
        ),
        "select-sources" => openride_author::select_sources(
            root,
            options.get("--expected-revision").ok_or_else(usage)?,
            options.get("--geometry").ok_or_else(usage)?,
            options.get("--elevation").ok_or_else(usage)?,
            cancel.as_deref(),
        ),
        "create" => openride_author::create(
            root,
            options.get("--name").ok_or_else(usage)?,
            cancel.as_deref(),
        ),
        "rename" => openride_author::rename(
            root,
            options.get("--expected-revision").ok_or_else(usage)?,
            options.get("--name").ok_or_else(usage)?,
            cancel.as_deref(),
        ),
        _ => openride_author::open(root),
    }
}
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("package") {
        let result=match args.get(1).map(String::as_str) {
            Some("validate") if args.len()==3 => openride_author::package::validate_input(Path::new(&args[2])).map(|v|serde_json::to_value(v).unwrap()),
            Some("pack") if args.len()==4 => openride_author::package::pack(Path::new(&args[2]),Path::new(&args[3])),
            Some("install") if args.len()==4 => openride_author::package::install(Path::new(&args[2]),Path::new(&args[3])),
            _=>Err(Error{code:"usage",message:"Usage: openride-author package validate FOLDER_OR_ZIP | pack FOLDER OUTPUT.zip | install INPUT.zip CACHE_FOLDER".into()}),
        };
        match result {
            Ok(package) => emit(
                json!({"protocol_version":1,"event":"result","status":"complete","package":package}),
            ),
            Err(error) => {
                emit(
                    json!({"protocol_version":1,"event":"result","status":"failed","error":error}),
                );
                std::process::exit(2);
            }
        }
        return;
    }
    if args == ["--help"] {
        println!("openride-author package validate FOLDER_OR_ZIP | pack FOLDER OUTPUT.zip | install INPUT.zip CACHE_FOLDER");
        println!("openride-author project ACTION FOLDER [OPTIONS]\nActions: create, open, status, rename, import, select-sources, repair, plan-maps, acquire, infer-roads, choose-road, reset-roads, build-course, edit-course, archive-edits, edit-scenery, build-package, export-package\nOptions: --name NAME; --expected-revision REVISION; --source FILE (repeatable); --geometry SHA256 --elevation SHA256; --repair ID --status accepted|rejected|pending; --layer terrain|streets|imagery|buildings|vegetation|water --max-tiles N; --sample N --mode trace|point|way|join|undo [--latitude LAT --longitude LON] [--way OSM_ID]; --accept-steep true|false; --cancel-file PATH\nScenery: --id FEATURE_ID --mode save|hide|undo|reset [--feature JSON | --feature-request SHA256_OF_jobs/scenery-edit.json]\nJSON-lines protocol v1; exit 0 success, 2 failure, 3 cancelled. Quote paths with spaces.");
        return;
    }
    match run(&args) {
        Ok(result) => emit(
            json!({"protocol_version":1,"event":"result","status":"complete","snapshot":result}),
        ),
        Err(error) => {
            let exit = if error.code == "cancelled" { 3 } else { 2 };
            emit(
                json!({"protocol_version":1,"event":"result","status":if exit==3 {"cancelled"} else {"failed"},"error":error}),
            );
            std::process::exit(exit);
        }
    }
}
