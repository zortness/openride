//! Ordinary ZIP sharing of referenced runtime assets only. Activity sources stay private.
use crate::{error, Result};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
const TOTAL: u64 = 2 * 1024 * 1024 * 1024;
const FILE_LIMIT: u64 = 16 * 1024 * 1024;
#[derive(Clone, Debug, Serialize)]
pub struct Asset {
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Debug, Serialize)]
pub struct Report {
    pub name: String,
    pub files: BTreeMap<String, Asset>,
    pub bytes: u64,
}
fn bad(message: impl Into<String>) -> crate::Error {
    error("invalid_package", message)
}
fn hash(raw: &[u8]) -> String {
    format!("{:x}", Sha256::digest(raw))
}
fn bounded(mut reader: impl Read, limit: u64) -> Result<Vec<u8>> {
    let mut raw = Vec::new();
    reader.by_ref().take(limit + 1).read_to_end(&mut raw)?;
    if raw.len() as u64 > limit {
        return Err(bad("Package asset exceeds size budget"));
    }
    Ok(raw)
}
fn local_file(root: &Path, name: &str) -> Result<Vec<u8>> {
    let path = root.join(name);
    if fs::symlink_metadata(&path)?.file_type().is_symlink() {
        return Err(bad("Package assets cannot be symbolic links"));
    }
    bounded(
        fs::File::open(path)?,
        if name == "manifest.json" {
            1024 * 1024
        } else {
            FILE_LIMIT
        },
    )
}
fn checked_with(mut read: impl FnMut(&str) -> Result<Vec<u8>>) -> Result<Report> {
    let raw = read("manifest.json")?;
    if raw.len() > 1024 * 1024 {
        return Err(bad("Manifest exceeds 1 MiB"));
    }
    let manifest: Value = serde_json::from_slice(&raw).map_err(|e| bad(e.to_string()))?;
    if manifest["kind"] != "openride.stream-preview"
        || manifest["schema_version"] != 1
        || manifest["trainer_ready"] != false
        || manifest["tile_m"] != 256
        || manifest["max_live_chunks"] != 9
        || manifest["neighborhood_radius_cells"] != 1
        || !manifest["attribution"].is_object()
        || manifest["presentation"]["terrain_corridor"] != true
    {
        return Err(bad("Unsupported runtime package or streaming settings"));
    }
    let name = manifest["name"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 1024)
        .ok_or_else(|| bad("Missing package name"))?
        .to_string();
    let mut entries = vec![("route.json".to_string(), &manifest["route"])];
    for (key, path) in [
        ("terrain_provenance", "terrain-provenance.json"),
        ("coast", "coast.json"),
        ("detail", "presentation.json"),
        ("imagery", "imagery.png"),
        ("coarse_terrain", "coarse-terrain.json"),
    ] {
        if let Some(e) = manifest.get(key) {
            entries.push((path.into(), e));
        }
    }
    let chunks = manifest["chunks"]
        .as_array()
        .filter(|v| !v.is_empty() && v.len() <= 4096)
        .ok_or_else(|| bad("Invalid chunk inventory"))?;
    for chunk in chunks {
        let x = chunk["x"]
            .as_i64()
            .filter(|v| v.unsigned_abs() <= 10000)
            .ok_or_else(|| bad("Invalid chunk coordinate"))?;
        let z = chunk["z"]
            .as_i64()
            .filter(|v| v.unsigned_abs() <= 10000)
            .ok_or_else(|| bad("Invalid chunk coordinate"))?;
        entries.push((format!("chunk_{x}_{z}.json"), chunk));
        if let Some(image) = chunk.get("imagery") {
            entries.push((format!("imagery_{x}_{z}.png"), image));
        }
    }
    let mut files = BTreeMap::from([(
        "manifest.json".into(),
        Asset {
            bytes: raw.len() as u64,
            sha256: hash(&raw),
        },
    )]);
    let mut total = raw.len() as u64;
    for (path, entry) in entries {
        let limit = if path == "imagery.png" {
            FILE_LIMIT
        } else if path.starts_with("imagery_") {
            128 * 1024
        } else {
            2 * 1024 * 1024
        };
        let size = entry["bytes"]
            .as_u64()
            .filter(|n| *n > 0 && *n <= limit)
            .ok_or_else(|| bad("Invalid payload size"))?;
        let checksum = entry["sha256"]
            .as_str()
            .filter(|s| s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit()))
            .ok_or_else(|| bad("Missing payload checksum"))?;
        if entry["path"].as_str() != Some(&path) || files.contains_key(&path) {
            return Err(bad("Unsafe or duplicate payload path"));
        }
        let raw = read(&path)?;
        if raw.len() as u64 != size || hash(&raw) != checksum {
            return Err(bad(format!("Missing or corrupt payload: {path}")));
        }
        if path.starts_with("imagery_") {
            crate::imagery::validate_tile(&raw)?;
        }
        total += size;
        if total > TOTAL {
            return Err(bad("Package exceeds 2 GiB expansion budget"));
        }
        files.insert(
            path,
            Asset {
                bytes: size,
                sha256: checksum.into(),
            },
        );
    }
    Ok(Report {
        name,
        files,
        bytes: total,
    })
}
pub fn validate(root: &Path) -> Result<Report> {
    checked_with(|name| local_file(root, name))
}
fn nonce() -> Result<String> {
    Ok(format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| bad(e.to_string()))?
            .as_nanos()
    ))
}
pub fn pack(root: &Path, destination: &Path) -> Result<Value> {
    if destination
        .extension()
        .and_then(|s| s.to_str())
        .is_none_or(|s| !s.eq_ignore_ascii_case("zip"))
    {
        return Err(bad("Use an ordinary .zip filename"));
    }
    if destination.exists() {
        return Err(error(
            "exists",
            "ZIP destination already exists; choose a new filename",
        ));
    }
    let report = validate(root)?;
    let temporary = destination.with_extension(format!("{}.pending", nonce()?));
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);
    for (name, asset) in &report.files {
        let raw = local_file(root, name)?;
        if hash(&raw) != asset.sha256 {
            return Err(bad("Package changed during export"));
        }
        writer
            .start_file(name, options)
            .map_err(|e| bad(e.to_string()))?;
        writer.write_all(&raw)?;
    }
    let file = writer.finish().map_err(|e| bad(e.to_string()))?;
    file.sync_all()?;
    drop(file);
    // Same-directory hard-link publication refuses concurrent destination creation.
    fs::hard_link(&temporary, destination)?;
    fs::remove_file(temporary)?;
    Ok(
        json!({"path":destination,"name":report.name,"files":report.files.len(),"bytes":fs::metadata(destination)?.len()}),
    )
}
fn safe_components(name: &str) -> bool {
    if name.is_empty()
        || name.len() > 240
        || name.contains('\\')
        || name.contains(':')
        || name.starts_with('/')
    {
        return false;
    }
    name.trim_end_matches('/').split('/').all(|c| {
        !c.is_empty()
            && c != "."
            && c != ".."
            && !c.ends_with([' ', '.'])
            && c.chars()
                .all(|v| !v.is_control() && !"<>:\"\\|?*".contains(v))
            && !matches!(
                c.split('.')
                    .next()
                    .unwrap_or("")
                    .to_ascii_uppercase()
                    .as_str(),
                "CON"
                    | "PRN"
                    | "AUX"
                    | "NUL"
                    | "COM1"
                    | "COM2"
                    | "COM3"
                    | "COM4"
                    | "COM5"
                    | "COM6"
                    | "COM7"
                    | "COM8"
                    | "COM9"
                    | "LPT1"
                    | "LPT2"
                    | "LPT3"
                    | "LPT4"
                    | "LPT5"
                    | "LPT6"
                    | "LPT7"
                    | "LPT8"
                    | "LPT9"
            )
    })
}
struct Archive {
    zip: zip::ZipArchive<fs::File>,
    entries: BTreeMap<String, usize>,
    identity: String,
}
impl Archive {
    fn open(path: &Path) -> Result<Self> {
        if fs::metadata(path)?.len() > TOTAL {
            return Err(bad("ZIP exceeds 2 GiB"));
        }
        let mut source = fs::File::open(path)?;
        let mut digest = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            let n = source.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            digest.update(&buffer[..n]);
        }
        let mut zip =
            zip::ZipArchive::new(fs::File::open(path)?).map_err(|e| bad(e.to_string()))?;
        if zip.len() > 12288 {
            return Err(bad("ZIP has too many entries"));
        }
        let mut names = BTreeMap::new();
        let mut seen = BTreeSet::new();
        let mut total = 0u64;
        for i in 0..zip.len() {
            let file = zip.by_index(i).map_err(|e| bad(e.to_string()))?;
            let name = file.name().map_err(|e| bad(e.to_string()))?.into_owned();
            if !safe_components(&name)
                || !seen.insert(name.trim_end_matches('/').to_ascii_lowercase())
            {
                return Err(bad("Unsafe or duplicate ZIP entry"));
            }
            let kind = file.unix_mode().unwrap_or(0) & 0o170000;
            if ![0, 0o100000, 0o040000].contains(&kind) || (kind == 0o040000 && !file.is_dir()) {
                return Err(bad("ZIP links and special files are unsupported"));
            }
            if file.is_dir() {
                continue;
            }
            if file.size() > FILE_LIMIT {
                return Err(bad("ZIP entry exceeds 16 MiB"));
            }
            total = total
                .checked_add(file.size())
                .ok_or_else(|| bad("ZIP size overflow"))?;
            if total > TOTAL {
                return Err(bad("ZIP expansion exceeds 2 GiB"));
            }
            names.insert(name, i);
        }
        let prefix = if names.contains_key("manifest.json") {
            String::new()
        } else {
            let roots: Vec<_> = names
                .keys()
                .filter(|s| s.ends_with("/manifest.json") && s.matches('/').count() == 1)
                .collect();
            if roots.len() != 1 {
                return Err(bad(
                    "ZIP must contain manifest.json at its root or inside one package folder",
                ));
            }
            roots[0].trim_end_matches("manifest.json").to_string()
        };
        let mut entries = BTreeMap::new();
        for (name, index) in names {
            let relative = name
                .strip_prefix(&prefix)
                .ok_or_else(|| bad("ZIP contains unrelated files outside package"))?;
            if relative.contains('/') {
                return Err(bad("Unsupported nested runtime asset"));
            }
            entries.insert(relative.to_string(), index);
        }
        Ok(Self {
            zip,
            entries,
            identity: format!("{:x}", digest.finalize()),
        })
    }
    fn read(&mut self, name: &str) -> Result<Vec<u8>> {
        let index = *self
            .entries
            .get(name)
            .ok_or_else(|| bad(format!("ZIP missing {name}")))?;
        let file = self.zip.by_index(index).map_err(|e| bad(e.to_string()))?;
        bounded(
            file,
            if name == "manifest.json" {
                1024 * 1024
            } else {
                FILE_LIMIT
            },
        )
    }
    fn inventory(&self, report: &Report) -> Result<()> {
        if self.entries.keys().ne(report.files.keys()) {
            return Err(bad(
                "ZIP includes unreferenced files; export only compiled runtime assets",
            ));
        }
        Ok(())
    }
}
pub fn validate_input(path: &Path) -> Result<Report> {
    if path.is_dir() {
        return validate(path);
    }
    let mut archive = Archive::open(path)?;
    let report = checked_with(|name| archive.read(name))?;
    archive.inventory(&report)?;
    Ok(report)
}
pub fn install(path: &Path, cache: &Path) -> Result<Value> {
    let mut archive = Archive::open(path)?;
    let report = checked_with(|name| archive.read(name))?;
    archive.inventory(&report)?;
    fs::create_dir_all(cache)?;
    let destination = cache.join(&archive.identity);
    if destination.exists() {
        let previous = validate(&destination)?;
        if previous
            .files
            .iter()
            .any(|(k, v)| report.files.get(k).is_none_or(|a| a.sha256 != v.sha256))
            || previous.files.len() != report.files.len()
        {
            return Err(bad("Managed cached package was modified"));
        }
        return Ok(json!({"path":destination,"name":report.name,"reused":true}));
    }
    let staging: PathBuf = cache.join(format!(".pending-{}", nonce()?));
    fs::create_dir(&staging)?;
    for (name, asset) in &report.files {
        let raw = archive.read(name)?;
        if hash(&raw) != asset.sha256 {
            return Err(bad("ZIP changed during installation"));
        }
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(staging.join(name))?;
        f.write_all(&raw)?;
        f.sync_all()?;
    }
    fs::rename(&staging, &destination)?;
    Ok(json!({"path":destination,"name":report.name,"reused":false}))
}
