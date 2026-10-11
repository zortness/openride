use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::Cursor;
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};
use tiff::{
    decoder::{Decoder, DecodingResult, Limits},
    tags::Tag,
    ColorType,
};

fn read(path: &Path) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 64 * 1024 * 1024 {
        return Err("Input exceeds 64 MiB".into());
    }
    Ok(bytes)
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Attach a verified source snapshot to an existing preparation in a new directory.
pub fn attach(
    preparation_path: &Path,
    sources: &Path,
    destination: &Path,
) -> Result<Value, String> {
    let preparation_bytes = read(preparation_path)?;
    let mut preparation: Value =
        serde_json::from_slice(&preparation_bytes).map_err(|e| e.to_string())?;
    let manifest: Value =
        serde_json::from_slice(&read(&sources.join("sources.json"))?).map_err(|e| e.to_string())?;
    if preparation["kind"] != "openride.scenery-preparation"
        || preparation["schema_version"] != 1
        || preparation["trainer_ready"] != false
        || manifest["kind"] != "openride.geographic-sources"
        || manifest["schema_version"] != 1
        || manifest["preparation_sha256"] != hash(&preparation_bytes)
        || manifest["source_route_sha256"] != preparation["source_route_sha256"]
    {
        return Err("Source/preparation identity mismatch or unsupported schema".into());
    }
    let request = &manifest["requests"]["terrain_export"];
    if request["pixelType"] != "F32"
        || request["renderingRule"] != "{\"rasterFunction\":\"None\"}"
        || request["imageSR"] != "4326"
    {
        return Err("Expected raw EPSG:4326 USGS elevation export request".into());
    }
    let verified = |name: &str| -> Result<Vec<u8>, String> {
        let bytes = read(&sources.join(name))?;
        if manifest["files"][name]["sha256"] != hash(&bytes) {
            return Err(format!("Source checksum mismatch: {name}"));
        }
        Ok(bytes)
    };
    let tiff = verified("terrain.tif")?;
    let export: Value =
        serde_json::from_slice(&verified("usgs-export.json")?).map_err(|e| e.to_string())?;
    let mut terrain = build(&tiff, &export, &preparation)?;
    terrain["source_snapshot"] = manifest;
    terrain["source_usage"] =
        json!("Only USGS elevation consumed; OSM snapshot reserved for later scenery work");
    preparation["terrain"] = terrain;
    preparation["warnings"] = json!(["Terrain context from a saved USGS service export; activity elevations are unchanged.",
        "Vertical references are not reconciled. Terrain is bare earth, not necessarily the road surface.",
        "OSM geometry, road corrections and streaming are not implemented. Preview only; no trainer control."]);
    let mut files = Vec::new();
    for name in ["road-preview.obj", "bounds.geojson", "osm-query.overpass"] {
        let bytes = read(
            &preparation_path
                .parent()
                .ok_or("Missing preparation directory")?
                .join(name),
        )?;
        let entry = preparation["files"]
            .as_array()
            .ok_or("Missing asset checksums")?
            .iter()
            .find(|p| p["path"] == name)
            .ok_or("Missing asset checksum")?;
        if entry["sha256"] != hash(&bytes) {
            return Err(format!("Preparation asset checksum mismatch: {name}"));
        }
        files.push((name, bytes));
    }
    // Compact numeric geometry keeps the preview within the viewer's 16 MiB limit.
    let output = serde_json::to_vec(&preparation).map_err(|e| e.to_string())?;
    if output.len() > 16 * 1024 * 1024 {
        return Err("Combined preparation exceeds viewer's 16 MiB limit".into());
    }
    fs::create_dir(destination).map_err(|e| e.to_string())?;
    files.push(("preparation.json", output));
    for (name, bytes) in files {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination.join(name))
            .map_err(|e| e.to_string())?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|e| e.to_string())?;
    }
    Ok(preparation)
}

fn number(value: &Value) -> Result<f64, String> {
    value
        .as_f64()
        .filter(|v| v.is_finite())
        .ok_or("Missing or non-finite coordinate".into())
}

pub fn build(tiff: &[u8], export: &Value, preparation: &Value) -> Result<Value, String> {
    let extent = &export["extent"];
    let sr = &extent["spatialReference"];
    if sr["latestWkid"].as_u64().or(sr["wkid"].as_u64()) != Some(4326) {
        return Err("Terrain requires EPSG:4326 export metadata".into());
    }
    let (west, south, east, north) = (
        number(&extent["xmin"])?,
        number(&extent["ymin"])?,
        number(&extent["xmax"])?,
        number(&extent["ymax"])?,
    );
    if !(-180. ..=180.).contains(&west)
        || east > 180.
        || south <= -80.
        || north >= 80.
        || east <= west
        || north <= south
        || east - west > 0.15
        || north - south > 0.1
    {
        return Err("Terrain extent must be a small local geographic section".into());
    }
    let mut limits = Limits::default();
    limits.decoding_buffer_size = 16 * 1024 * 1024;
    limits.intermediate_buffer_size = 16 * 1024 * 1024;
    let mut decoder = Decoder::new(Cursor::new(tiff))
        .map_err(|e| e.to_string())?
        .with_limits(limits);
    let (width, height) = decoder.dimensions().map_err(|e| e.to_string())?;
    if !(2..=256).contains(&width)
        || !(2..=256).contains(&height)
        || export["width"].as_u64() != Some(width.into())
        || export["height"].as_u64() != Some(height.into())
        || decoder.colortype().map_err(|e| e.to_string())? != ColorType::Gray(32)
    {
        return Err(
            "Expected a 2..256 pixel single-band F32 elevation raster matching export dimensions"
                .into(),
        );
    }
    if decoder.get_tag_u32(Tag::Orientation).unwrap_or(1) != 1 {
        return Err("Only north-up TIFF orientation is supported".into());
    }
    let nodata = decoder
        .get_tag_ascii_string(Tag::GdalNodata)
        .ok()
        .and_then(|s| s.trim_matches('\0').parse::<f32>().ok());
    let DecodingResult::F32(samples) = decoder.read_image().map_err(|e| e.to_string())? else {
        return Err("Terrain pixels must be floating-point elevations, not imagery".into());
    };
    let width = width as usize;
    let height = height as usize;
    let valid = |i: usize| {
        samples[i].is_finite()
            && (-12000. ..=12000.).contains(&samples[i])
            && nodata != Some(samples[i])
    };
    let lat0 = number(&preparation["origin_lat_lon_elevation"][0])?;
    let lon0 = number(&preparation["origin_lat_lon_elevation"][1])?;
    let elevation0 = number(&preparation["origin_lat_lon_elevation"][2])?;
    let dx = (east - west) / width as f64;
    let dy = (north - south) / height as f64;
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for row in 0..height {
        for col in 0..width {
            let i = row * width + col;
            let lat = north - (row as f64 + 0.5) * dy;
            let lon = west + (col as f64 + 0.5) * dx;
            vertices.push([
                super::EARTH_M * (lon - lon0).to_radians() * lat0.to_radians().cos(),
                if valid(i) {
                    samples[i] as f64 - elevation0
                } else {
                    0.
                },
                -super::EARTH_M * (lat - lat0).to_radians(),
            ]);
            if row + 1 < height && col + 1 < width {
                for triangle in [[i, i + 1, i + width], [i + 1, i + width + 1, i + width]] {
                    if triangle.iter().all(|&p| valid(p)) {
                        indices.extend(triangle);
                    }
                }
            }
        }
    }
    if indices.is_empty() {
        return Err("No valid terrain triangles".into());
    }
    let points = preparation["points"]
        .as_array()
        .ok_or("Missing route points")?;
    let mut comparison = Vec::new();
    for point in points {
        let col = (number(&point["longitude"])? - west) / dx - 0.5;
        let row = (north - number(&point["latitude"])?) / dy - 0.5;
        let sample =
            if col >= 0. && row >= 0. && col <= (width - 1) as f64 && row <= (height - 1) as f64 {
                let x = (col.floor() as usize).min(width - 2);
                let y = (row.floor() as usize).min(height - 2);
                let i = y * width + x;
                if [i, i + 1, i + width, i + width + 1]
                    .iter()
                    .all(|&p| valid(p))
                {
                    let fx = col - x as f64;
                    let fy = row - y as f64;
                    Some(
                        (samples[i] as f64 * (1. - fx) + samples[i + 1] as f64 * fx) * (1. - fy)
                            + (samples[i + width] as f64 * (1. - fx)
                                + samples[i + width + 1] as f64 * fx)
                                * fy,
                    )
                } else {
                    None
                }
            } else {
                None
            };
        comparison.push(json!({"source_sample":point["source_sample"],"terrain_elevation_m":sample,
            "activity_minus_terrain_m":sample.zip(point["elevation_m"].as_f64()).map(|(terrain,activity)|activity-terrain)}));
    }
    Ok(
        json!({"mesh":{"vertices":vertices,"indices":indices},"comparison":comparison,
        "width":width,"height":height,"nodata_pixels":(0..samples.len()).filter(|&i|!valid(i)).count(),
        "extent":extent,"sampling":"Pixel centres, north-up; bilinear route comparison; holes preserved",
        "vertical_reference":"Unverified; differences include activity datum/bias and ground-versus-road effects"}),
    )
}
