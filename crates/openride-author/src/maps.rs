//! Bounded corridor acquisition; verified tile receipts survive cancellation.
use crate::{cancelled, checked, error, lock, open, Result, Snapshot};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
const USGS: &str =
    "https://elevation.nationalmap.gov/arcgis/rest/services/3DEPElevation/ImageServer";
const OVERPASS: &str = "https://overpass-api.de/api/interpreter";
const LIMIT: u64 = 64 * 1024 * 1024;
pub(crate) fn atomic_file(path: &Path, raw: &[u8]) -> Result<()> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| error("clock", e.to_string()))?
        .as_nanos();
    let temporary = path.with_extension(format!("{}-{stamp}.pending", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(raw)?;
    file.sync_all()?;
    drop(file);
    fs::rename(temporary, path)?;
    Ok(())
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Tile {
    pub id: String,
    pub bounds: [f64; 4],
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Plan {
    pub schema_version: u32,
    pub key: String,
    pub origin: [f64; 3],
    pub tiles: Vec<Tile>,
    pub terrain_ready: usize,
    pub streets_ready: usize,
    #[serde(default)]
    pub street_ways: usize,
    #[serde(default)]
    pub imagery_ready: usize,
    #[serde(default)]
    pub scenery_ready: BTreeMap<String, usize>,
    #[serde(default)]
    pub tile_layers: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub estimated_imagery_bytes: usize,
    pub estimated_terrain_bytes: usize,
    pub resolution_m: f64,
    pub attribution: BTreeMap<String, String>,
}
fn hash(raw: &[u8]) -> String {
    format!("{:x}", Sha256::digest(raw))
}
pub fn plan(snapshot: &Snapshot) -> Result<Plan> {
    let preview = snapshot.repair_preview.as_ref().ok_or_else(|| {
        error(
            "source_required",
            "Select a geometry source before planning maps",
        )
    })?;
    if preview.length_m > 200000. {
        return Err(error(
            "coverage_limit",
            "Current full-corridor acquisition supports up to 200 km",
        ));
    }
    let located: Vec<_> = preview
        .samples
        .iter()
        .filter_map(|s| Some([s.latitude?, s.longitude?]))
        .collect();
    if located.len() < 2
        || located
            .iter()
            .any(|p| p[0].abs() >= 80. || p[1].abs() > 180.)
    {
        return Err(error(
            "coverage_limit",
            "Need located samples within supported latitude/longitude bounds",
        ));
    }
    let reference = snapshot
        .review
        .as_ref()
        .and_then(|r| {
            r.activities
                .iter()
                .find(|a| Some(&a.source_sha256) == r.document.geometry_source.as_ref())
        })
        .ok_or_else(|| error("source_required", "Geometry source missing"))?;
    let anchors: Vec<_> = reference
        .samples
        .iter()
        .filter_map(|s| Some([s.latitude?, s.longitude?]))
        .collect();
    let lat = anchors.iter().map(|p| p[0]).sum::<f64>() / anchors.len() as f64;
    let lon = anchors.iter().map(|p| p[1]).sum::<f64>() / anchors.len() as f64;
    let scale = 111195. * lat.to_radians().cos();
    let mut cells = BTreeSet::new();
    let mut add = |p: [f64; 2]| -> Result<()> {
        let x = (p[1] - lon) * scale;
        let z = (p[0] - lat) * 111195.;
        if x.hypot(z) > 99000. {
            return Err(error(
                "coverage_limit",
                "Route exceeds 99 km local acquisition radius",
            ));
        }
        for ix in ((x - 256.) / 2000.).floor() as i32..=((x + 256.) / 2000.).floor() as i32 {
            for iz in ((z - 256.) / 2000.).floor() as i32..=((z + 256.) / 2000.).floor() as i32 {
                cells.insert((ix, iz));
            }
        }
        if cells.len() > 1024 {
            return Err(error(
                "coverage_limit",
                "Corridor exceeds 1024 source tiles",
            ));
        }
        Ok(())
    };
    for p in &located {
        add(*p)?;
    }
    for w in preview.samples.windows(2) {
        if w[1].break_before {
            continue;
        }
        if let (Some(a), Some(b), Some(c), Some(d)) =
            (w[0].latitude, w[0].longitude, w[1].latitude, w[1].longitude)
        {
            let n = (ride_core::haversine(a, b, c, d) / 250.).ceil().max(1.) as usize;
            if n > 800 {
                return Err(error("coverage_limit", "Unbounded source segment"));
            }
            for i in 1..n {
                let t = i as f64 / n as f64;
                add([a + (c - a) * t, b + (d - b) * t])?;
            }
        }
    }
    let tiles: Vec<_> = cells
        .into_iter()
        .map(|(x, z)| Tile {
            id: format!("tile_{x}_{z}"),
            bounds: [
                lon + (x as f64 * 2000. - 16.) / scale,
                lat + (z as f64 * 2000. - 16.) / 111195.,
                lon + ((x + 1) as f64 * 2000. + 16.) / scale,
                lat + ((z + 1) as f64 * 2000. + 16.) / 111195.,
            ],
        })
        .collect();
    let key = hash(&serde_json::to_vec(&tiles).unwrap());
    let count = tiles.len();
    Ok(Plan {
        schema_version: 1,
        key,
        origin: [lat, lon, 0.],
        tiles,
        terrain_ready: 0,
        streets_ready: 0,
        street_ways: 0,
        imagery_ready: 0,
        scenery_ready: BTreeMap::new(),
        tile_layers: BTreeMap::new(),
        estimated_imagery_bytes: count * 1024 * 1024 * 4,
        estimated_terrain_bytes: count * 256 * 256 * 4,
        resolution_m: 2032. / 256.,
        attribution: BTreeMap::from([
            (
                "terrain".into(),
                "USGS 3DEP elevation; source metadata retained with each tile".into(),
            ),
            (
                "streets".into(),
                "© OpenStreetMap contributors, ODbL 1.0; https://www.openstreetmap.org/copyright"
                    .into(),
            ),
            ("imagery".into(),"USGS National Map / NAIP public-domain aerial imagery; mosaic photography dates unresolved".into()),
        ]),
    })
}
pub(crate) fn read(path: &Path) -> Result<Vec<u8>> {
    let mut raw = Vec::new();
    fs::File::open(path)?
        .take(LIMIT + 1)
        .read_to_end(&mut raw)?;
    if raw.len() as u64 > LIMIT {
        return Err(error("source_limit", "Map source exceeds 64 MiB"));
    }
    Ok(raw)
}
pub(crate) fn verified(dir: &Path, layer: &str) -> bool {
    let Ok(raw) = read(&dir.join(format!("{layer}.receipt.json"))) else {
        return false;
    };
    let Ok(files) = serde_json::from_slice::<BTreeMap<String, String>>(&raw) else {
        return false;
    };
    let required = if layer == "terrain" {
        vec!["terrain.tif", "export.json", "service.json", "catalog.json"]
    } else if layer == "imagery" {
        vec![
            "imagery.png",
            "imagery-export.json",
            "imagery-service.json",
            "imagery.json",
        ]
    } else if layer == "streets" {
        vec!["streets.osm"]
    } else {
        vec!["scenery.osm"]
    };
    files.len() == required.len()
        && required.iter().all(|name| {
            files.get(*name).is_some_and(|expected| {
                read(&dir.join(name)).is_ok_and(|raw| hash(&raw) == *expected)
            })
        })
}
pub fn load(root: &Path) -> Result<Option<Plan>> {
    let path = root.join("maps/index.json");
    if !path.exists() {
        return Ok(None);
    }
    let raw = read(&path)?;
    if raw.len() > 2 * 1024 * 1024 {
        return Err(error("invalid_maps", "Map index exceeds 2 MiB"));
    }
    let plan: Plan =
        serde_json::from_slice(&raw).map_err(|e| error("invalid_maps", e.to_string()))?;
    if plan.schema_version != 1 || plan.tiles.len() > 1024 {
        return Err(error("invalid_maps", "Unsupported map index"));
    }
    Ok(Some(plan))
}
pub fn save_plan(root: &Path, revision: &str, cancel: Option<&Path>) -> Result<Snapshot> {
    cancelled(cancel)?;
    let _guard = lock(root)?;
    let current = checked(root, revision)?;
    let mut plan = plan(&current)?;
    refresh(root, &mut plan);
    fs::create_dir_all(root.join("maps"))?;
    atomic_file(
        &root.join("maps/index.json"),
        &serde_json::to_vec_pretty(&plan).unwrap(),
    )?;
    open(root)
}
fn refresh(root: &Path, plan: &mut Plan) {
    plan.terrain_ready = 0;
    plan.streets_ready = 0;
    plan.imagery_ready = 0;
    plan.scenery_ready.clear();
    plan.tile_layers.clear();
    for tile in &plan.tiles {
        let dir = root.join("maps").join(&plan.key).join(&tile.id);
        if verified(&dir, "terrain") {
            plan.terrain_ready += 1;
            plan.tile_layers
                .entry(tile.id.clone())
                .or_default()
                .push("terrain".into());
        }
        if verified(&dir, "streets") {
            plan.streets_ready += 1;
            plan.tile_layers
                .entry(tile.id.clone())
                .or_default()
                .push("streets".into());
        }
        if verified(&dir, "imagery") {
            plan.imagery_ready += 1;
            plan.tile_layers
                .entry(tile.id.clone())
                .or_default()
                .push("imagery".into());
        }
        for layer in ["buildings", "vegetation", "water"] {
            if verified(&dir.join(layer), layer) {
                *plan.scenery_ready.entry(layer.into()).or_default() += 1;
                plan.tile_layers
                    .entry(tile.id.clone())
                    .or_default()
                    .push(layer.into());
            }
        }
    }
}
pub fn download(
    client: &reqwest::blocking::Client,
    url: &str,
    form: Option<&str>,
    cancel: Option<&Path>,
) -> Result<Vec<u8>> {
    for attempt in 0..3 {
        let mut backoff_ticks = 20 << attempt;
        cancelled(cancel)?;
        let request = if let Some(body) = form {
            client.post(url).form(&[("data", body)])
        } else {
            client.get(url)
        };
        let outcome = request.send();
        match outcome {
            Ok(response) if response.status().is_success() => {
                let mut raw = Vec::new();
                match response.take(LIMIT + 1).read_to_end(&mut raw) {
                    Ok(_) if raw.len() as u64 <= LIMIT => return Ok(raw),
                    Ok(_) => return Err(error("source_limit", "Download exceeds 64 MiB")),
                    Err(e) if attempt == 2 => return Err(error("download", e.to_string())),
                    _ => {}
                }
            }
            Ok(response) => {
                let code = response.status().as_u16();
                if code == 429 {
                    backoff_ticks = response
                        .headers()
                        .get("retry-after")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse::<u64>().ok())
                        .unwrap_or(60)
                        .clamp(5, 120)
                        * 10;
                }
                if attempt == 2 || ![408, 429, 500, 502, 503, 504].contains(&code) {
                    return Err(error(
                        "download",
                        format!("Map service returned HTTP {code}; retry this stage later"),
                    ));
                }
            }
            Err(e) if attempt == 2 => return Err(error("download", e.to_string())),
            Err(_) => {}
        }
        for _ in 0..backoff_ticks {
            cancelled(cancel)?;
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    Err(error("download", "Request retries exhausted"))
}
fn json_response(raw: &[u8]) -> Result<Value> {
    let value: Value =
        serde_json::from_slice(raw).map_err(|e| error("map_response", e.to_string()))?;
    if !value.is_object() || value.get("error").is_some() {
        return Err(error("map_response", "Map service returned an error"));
    }
    Ok(value)
}
pub fn acquire(
    root: &Path,
    revision: &str,
    layer: &str,
    max_tiles: usize,
    cancel: Option<&Path>,
) -> Result<Snapshot> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(60))
        .connect_timeout(Duration::from_secs(15))
        .user_agent("OpenRide-authoring/0.1")
        .build()
        .map_err(|e| error("network", e.to_string()))?;
    let mut last_streets = None::<std::time::Instant>;
    acquire_with(root, revision, layer, max_tiles, cancel, |url, body| {
        if url == OVERPASS {
            while last_streets.is_some_and(|last| last.elapsed() < Duration::from_secs(5)) {
                cancelled(cancel)?;
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        let result = download(&client, url, body, cancel);
        if url == OVERPASS {
            last_streets = Some(std::time::Instant::now());
        }
        result
    })
}
pub fn street_graph(
    root: &Path,
    plan: &Plan,
    cancel: Option<&Path>,
) -> Result<crate::streets::Graph> {
    let mut graph = crate::streets::Graph::default();
    for tile in &plan.tiles {
        cancelled(cancel)?;
        let dir = root.join("maps").join(&plan.key).join(&tile.id);
        if verified(&dir, "streets") {
            let raw = read(&dir.join("streets.osm"))?;
            let mut part = crate::streets::parse(&raw)?;
            part.sources.insert(tile.id.clone(), hash(&raw));
            graph.merge(part)?;
        }
    }
    graph.validate()?;
    Ok(graph)
}
pub fn street_sources(root: &Path, plan: &Plan) -> BTreeMap<String, String> {
    plan.tiles
        .iter()
        .filter_map(|tile| {
            let dir = root.join("maps").join(&plan.key).join(&tile.id);
            if !verified(&dir, "streets") {
                return None;
            }
            read(&dir.join("streets.osm"))
                .ok()
                .map(|raw| (tile.id.clone(), hash(&raw)))
        })
        .collect()
}
pub fn acquire_with<F>(
    root: &Path,
    revision: &str,
    layer: &str,
    max_tiles: usize,
    cancel: Option<&Path>,
    mut fetch: F,
) -> Result<Snapshot>
where
    F: FnMut(&str, Option<&str>) -> Result<Vec<u8>>,
{
    if ![
        "terrain",
        "streets",
        "imagery",
        "buildings",
        "vegetation",
        "water",
    ]
    .contains(&layer)
        || max_tiles == 0
    {
        return Err(error(
            "usage",
            "Select terrain, streets, imagery, buildings, vegetation or water and a positive tile budget",
        ));
    }
    cancelled(cancel)?;
    let _guard = lock(root)?;
    let current = checked(root, revision)?;
    let mut plan = plan(&current)?;
    fs::create_dir_all(root.join("maps"))?;
    refresh(root, &mut plan);
    let mut downloaded = 0;
    let scenery_layer = ["buildings", "vegetation", "water"].contains(&layer);
    for tile in &plan.tiles.clone() {
        cancelled(cancel)?;
        let mut dir = root.join("maps").join(&plan.key).join(&tile.id);
        if scenery_layer {
            dir = dir.join(layer);
        }
        if verified(&dir, layer) {
            continue;
        }
        if downloaded >= max_tiles {
            break;
        }
        fs::create_dir_all(&dir)?;
        let mut files = BTreeMap::new();
        let mut save = |name: &str, raw: Vec<u8>| -> Result<()> {
            atomic_file(&dir.join(name), &raw)?;
            files.insert(name.to_string(), hash(&raw));
            Ok(())
        };
        let [w, s, e, n] = tile.bounds;
        if layer == "terrain" {
            let service = fetch(&format!("{USGS}?f=json"), None)?;
            json_response(&service)?;
            save("service.json", service)?;
            let catalog=fetch(&format!("{USGS}/query?f=json&geometry={w},{s},{e},{n}&geometryType=esriGeometryEnvelope&inSR=4326&spatialRel=esriSpatialRelIntersects&outFields=*&returnGeometry=false"),None)?;
            json_response(&catalog)?;
            save("catalog.json", catalog)?;
            let url=format!("{USGS}/exportImage?f=json&bbox={w},{s},{e},{n}&bboxSR=4326&imageSR=4326&size=256,256&format=tiff&pixelType=F32&compression=LZ77&adjustAspectRatio=false&interpolation=RSP_BilinearInterpolation&renderingRule=%7B%22rasterFunction%22%3A%22None%22%7D");
            let raw = fetch(&url, None)?;
            let export = json_response(&raw)?;
            let actual = ["xmin", "ymin", "xmax", "ymax"].map(|k| export["extent"][k].as_f64());
            if actual
                .iter()
                .zip(tile.bounds)
                .any(|(a, b)| a.is_none_or(|v| (v - b).abs() > 1e-7))
            {
                return Err(error(
                    "map_response",
                    "DEM extent differs from planned tile",
                ));
            }
            let href = export["href"]
                .as_str()
                .ok_or_else(|| error("map_response", "Missing DEM download URL"))?;
            let parsed =
                reqwest::Url::parse(href).map_err(|e| error("map_response", e.to_string()))?;
            if parsed.scheme() != "https"
                || parsed.host_str() != Some("elevation.nationalmap.gov")
                || !parsed.username().is_empty()
                || parsed.password().is_some()
            {
                return Err(error("map_response", "Unexpected DEM download host"));
            }
            let bytes = fetch(href, None)?;
            let terrain = content_builder::terrain::build(
                &bytes,
                &export,
                &json!({"origin_lat_lon_elevation":plan.origin,"points":[]}),
            )
            .map_err(|e| error("invalid_terrain", e))?;
            if terrain["nodata_pixels"].as_u64() != Some(0) {
                return Err(error(
                    "missing_terrain",
                    "DEM has missing pixels; coverage needs review",
                ));
            }
            save("terrain.tif", bytes)?;
            save("export.json", raw)?;
        } else if layer == "imagery" {
            let service = crate::imagery::SERVICE;
            let metadata = fetch(&format!("{service}?f=json"), None)?;
            json_response(&metadata)?;
            save("imagery-service.json", metadata)?;
            let raw=fetch(&format!("{service}/exportImage?f=json&bbox={w},{s},{e},{n}&bboxSR=4326&imageSR=4326&size=1024,1024&format=png&adjustAspectRatio=false&interpolation=RSP_BilinearInterpolation"),None)?;
            let export = json_response(&raw)?;
            let extent = &export["extent"];
            let sr = &extent["spatialReference"];
            if export["width"] != 1024
                || export["height"] != 1024
                || sr.get("latestWkid").unwrap_or(&sr["wkid"]) != 4326
                || ["xmin", "ymin", "xmax", "ymax"]
                    .iter()
                    .zip(tile.bounds)
                    .any(|(key, expected)| {
                        extent[key]
                            .as_f64()
                            .is_none_or(|v| (v - expected).abs() > 1e-7)
                    })
            {
                return Err(error(
                    "invalid_imagery",
                    "Image dimensions, CRS or extent differ from planned tile",
                ));
            }
            let href = export["href"]
                .as_str()
                .ok_or_else(|| error("invalid_imagery", "Missing imagery URL"))?;
            let parsed =
                reqwest::Url::parse(href).map_err(|e| error("invalid_imagery", e.to_string()))?;
            if parsed.scheme() != "https"
                || parsed.host_str() != Some("imagery.nationalmap.gov")
                || !parsed.username().is_empty()
                || parsed.password().is_some()
            {
                return Err(error("invalid_imagery", "Unexpected imagery download host"));
            }
            let image = fetch(href, None)?;
            let transparent = crate::imagery::validate(&image)?;
            save("imagery.png", image)?;
            save("imagery-export.json", raw)?;
            save("imagery.json",serde_json::to_vec(&json!({"schema_version":1,"bounds":tile.bounds,"resolution_m":2032./1024.,"transparent_pixels":transparent,"source":service,"attribution":plan.attribution["imagery"],"photography_date":"Mosaic dates unresolved"})).unwrap())?;
        } else if scenery_layer {
            let selectors: &[&str] = match layer {
                "buildings" => &["way[building]", "relation[building]"],
                "vegetation" => &[
                    "way[natural~\"^(wood|scrub|heath)$\"]",
                    "relation[natural~\"^(wood|scrub|heath)$\"]",
                    "way[landuse=forest]",
                    "relation[landuse=forest]",
                ],
                _ => &[
                    "way[natural=water]",
                    "relation[natural=water]",
                    "way[waterway=riverbank]",
                    "relation[waterway=riverbank]",
                    "way[natural=coastline]",
                ],
            };
            let clauses = selectors
                .iter()
                .map(|slt| format!("{slt}({s},{w},{n},{e});"))
                .collect::<String>();
            let query =
                format!("[out:xml][timeout:25][maxsize:33554432];({clauses});(._;>>;);out meta;");
            let raw = fetch(OVERPASS, Some(&query))?;
            crate::streets::parse(&raw)?;
            save("scenery.osm", raw)?;
        } else {
            // Street matching needs complete ways and their nodes. Regional scenery
            // relations belong to separate scenery layers, not every street tile.
            let query=format!("[out:xml][timeout:25][maxsize:33554432];way[highway]({s},{w},{n},{e});(._;>;);out meta;");
            let raw = fetch(OVERPASS, Some(&query))?;
            crate::streets::parse(&raw)?;
            save("streets.osm", raw)?;
        }
        cancelled(cancel)?;
        atomic_file(
            &dir.join(format!("{layer}.receipt.json")),
            &serde_json::to_vec(&files).unwrap(),
        )?;
        downloaded += 1;
        if layer == "terrain" {
            plan.terrain_ready += 1;
        } else if layer == "imagery" {
            plan.imagery_ready += 1;
        } else if scenery_layer {
            *plan.scenery_ready.entry(layer.into()).or_default() += 1;
        } else {
            plan.streets_ready += 1;
        }
        atomic_file(
            &root.join("maps/index.json"),
            &serde_json::to_vec_pretty(&plan).unwrap(),
        )?;
        println!(
            "{}",
            json!({"protocol_version":1,"event":"progress","action":"acquire","phase":layer,"tile":tile.id,"terrain_ready":plan.terrain_ready,"streets_ready":plan.streets_ready,"imagery_ready":plan.imagery_ready,"total":plan.tiles.len()})
        );
    }
    refresh(root, &mut plan);
    if scenery_layer {
        crate::scenery::refresh_catalog(root, &plan, cancel)?;
    }
    if layer == "streets" {
        let graph = street_graph(root, &plan, cancel)?;
        let roads = graph.roads();
        plan.street_ways = roads.len();
        let dir = root.join("maps").join(&plan.key);
        fs::create_dir_all(&dir)?;
        atomic_file(
            &dir.join("streets.json"),
            &serde_json::to_vec(&graph).unwrap(),
        )?;
        // A bounded display copy; full identity/topology remains in streets.json.
        let mut count = 0;
        let preview: Vec<_> = roads
            .iter()
            .take_while(|road| {
                count += road.len();
                count <= 100_000
            })
            .collect();
        atomic_file(
            &dir.join("streets-preview.json"),
            &serde_json::to_vec(
                &json!({"roads":preview,"total_ways":roads.len(),"shown_ways":preview.len()}),
            )
            .unwrap(),
        )?;
    }
    atomic_file(
        &root.join("maps/index.json"),
        &serde_json::to_vec_pretty(&plan).unwrap(),
    )?;
    open(root)
}
