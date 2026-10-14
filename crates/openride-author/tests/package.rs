use openride_author::package;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Work(PathBuf);
impl Drop for Work {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn fixture() -> Work {
    let p = std::env::temp_dir().join(format!(
        "or-zip-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&p).unwrap();
    let source = p.join("Unpacked ride");
    fs::create_dir(&source).unwrap();
    let route = b"{\"points\":[]}";
    let chunk = b"{\"terrain\":{},\"road\":{}}";
    let entry = |name: &str, raw: &[u8]| json!({"path":name,"bytes":raw.len(),"sha256":format!("{:x}",Sha256::digest(raw))});
    let mut tile = entry("chunk_0_0.json", chunk);
    tile["x"] = json!(0);
    tile["z"] = json!(0);
    let manifest = json!({"kind":"openride.stream-preview","schema_version":1,"trainer_ready":false,"name":"Synthetic ZIP ride","tile_m":256,"max_live_chunks":9,"neighborhood_radius_cells":1,"attribution":{"terrain":"Synthetic"},"presentation":{"terrain_corridor":true},"route":entry("route.json",route),"chunks":[tile]});
    fs::write(
        source.join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    fs::write(source.join("route.json"), route).unwrap();
    fs::write(source.join("chunk_0_0.json"), chunk).unwrap();
    fs::write(source.join("private-source.gpx"), b"do not share").unwrap();
    Work(p)
}
#[test]
fn plain_zip_and_unpacked_folder_roundtrip_share_only_referenced_assets() {
    let w = fixture();
    let source = w.0.join("Unpacked ride");
    let archive = w.0.join("Shared ride.zip");
    package::pack(&source, &archive).unwrap();
    let report = package::validate_input(&archive).unwrap();
    assert_eq!(report.files.len(), 3);
    assert!(!report.files.contains_key("private-source.gpx"));
    let cache = w.0.join("cache");
    let installed = package::install(&archive, &cache).unwrap();
    let path = PathBuf::from(installed["path"].as_str().unwrap());
    assert_eq!(
        fs::read(path.join("route.json")).unwrap(),
        fs::read(source.join("route.json")).unwrap()
    );
    assert!(!path.join("private-source.gpx").exists());
    assert_eq!(package::install(&archive, &cache).unwrap()["reused"], true);
    assert_eq!(package::pack(&source, &archive).unwrap_err().code, "exists");
    fs::write(path.join("route.json"), b"corrupt").unwrap();
    assert!(package::install(&archive, &cache).is_err());
    fs::write(source.join("chunk_0_0.json"), b"bad").unwrap();
    assert!(package::validate_input(&source).is_err());
}
fn zip_files(path: &std::path::Path, files: &[(String, Vec<u8>)]) {
    let mut writer = zip::ZipWriter::new(fs::File::create(path).unwrap());
    for (name, raw) in files {
        writer
            .start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .unwrap();
        writer.write_all(raw).unwrap();
    }
    writer.finish().unwrap();
}
#[test]
fn supports_one_outer_folder_but_rejects_traversal_duplicates_extras_and_expansion() {
    let w = fixture();
    let source = w.0.join("Unpacked ride");
    let files: Vec<_> = ["manifest.json", "route.json", "chunk_0_0.json"]
        .iter()
        .map(|n| (format!("My Ride/{n}"), fs::read(source.join(n)).unwrap()))
        .collect();
    let archive = w.0.join("wrapped.zip");
    zip_files(&archive, &files);
    assert!(package::validate_input(&archive).is_ok());
    for (index, extra) in [
        ("../escape", vec![0]),
        ("My Ride/Manifest.json", vec![0]),
        ("My Ride/secrets.txt", vec![0]),
        ("My Ride/large", vec![0; 16 * 1024 * 1024 + 1]),
    ]
    .into_iter()
    .enumerate()
    {
        let mut bad = files.clone();
        bad.push((extra.0.into(), extra.1));
        let path = w.0.join(format!("bad-{index}.zip"));
        zip_files(&path, &bad);
        assert!(package::install(&path, &w.0.join("cache")).is_err());
        assert!(!w.0.join("escape").exists());
    }
    let path = w.0.join("link.zip");
    let mut writer = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    writer
        .add_symlink(
            "manifest.json",
            "../../escape",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
    writer.finish().unwrap();
    assert!(package::validate_input(&path).is_err());
}
