use content_builder::terrain;
use serde_json::json;
use std::io::Cursor;
use tiff::encoder::{colortype::Gray32Float, Compression, TiffEncoder};

fn raster(values: &[f32]) -> Vec<u8> {
    raster_compressed(values, Compression::Uncompressed)
}

fn raster_compressed(values: &[f32], compression: Compression) -> Vec<u8> {
    let mut output = Cursor::new(Vec::new());
    TiffEncoder::new(&mut output)
        .unwrap()
        .with_compression(compression)
        .write_image::<Gray32Float>(3, 3, values)
        .unwrap();
    output.into_inner()
}

#[test]
fn builds_north_up_terrain_and_compares_elevation_at_pixel_centers() {
    let bytes = raster(&[10., 20., 30., 40., 50., 60., 70., 80., 90.]);
    let export = json!({"width":3,"height":3,"extent":{"xmin":-118.403,"xmax":-118.4,"ymin":33.8,"ymax":33.803,"spatialReference":{"wkid":4326}}});
    let preparation = json!({"origin_lat_lon_elevation":[33.8015,-118.4015,45.],
        "points":[{"latitude":33.8015,"longitude":-118.4015,"elevation_m":55.,"source_sample":7}]});
    let result = terrain::build(&bytes, &export, &preparation).unwrap();
    let lzw = raster_compressed(
        &[10., 20., 30., 40., 50., 60., 70., 80., 90.],
        Compression::Lzw,
    );
    assert_eq!(terrain::build(&lzw, &export, &preparation).unwrap(), result);
    let vertices = result["mesh"]["vertices"].as_array().unwrap();
    assert_eq!(vertices.len(), 9);
    assert_eq!(vertices[0][1], -35.);
    assert!(vertices[0][2].as_f64().unwrap() < 0.); // Row zero is north.
    assert_eq!(result["mesh"]["indices"].as_array().unwrap().len(), 24);
    assert!(
        (result["comparison"][0]["terrain_elevation_m"]
            .as_f64()
            .unwrap()
            - 50.)
            .abs()
            < 1e-6
    );
    assert!(
        (result["comparison"][0]["activity_minus_terrain_m"]
            .as_f64()
            .unwrap()
            - 5.)
            .abs()
            < 1e-6
    );
}

#[test]
fn preserves_nodata_holes_and_rejects_wrong_crs_or_dimensions() {
    let bytes = raster(&[f32::NAN, 20., 30., 40., 50., 60., 70., 80., 90.]);
    let mut export = json!({"width":3,"height":3,"extent":{"xmin":-118.403,"xmax":-118.4,"ymin":33.8,"ymax":33.803,"spatialReference":{"wkid":4326}}});
    let preparation = json!({"origin_lat_lon_elevation":[33.8015,-118.4015,45.],
        "points":[{"latitude":33.80225,"longitude":-118.40225,"elevation_m":55.,"source_sample":7}]});
    let result = terrain::build(&bytes, &export, &preparation).unwrap();
    assert_eq!(result["nodata_pixels"], 1);
    assert_eq!(result["mesh"]["indices"].as_array().unwrap().len(), 21);
    assert!(result["comparison"][0]["terrain_elevation_m"].is_null());
    export["width"] = json!(4);
    assert!(terrain::build(&bytes, &export, &preparation).is_err());
    export["width"] = json!(3);
    export["extent"]["spatialReference"]["wkid"] = json!(3857);
    assert!(terrain::build(&bytes, &export, &preparation).is_err());
}

#[test]
fn attaches_only_matching_sources_and_preserves_original_preparation() {
    use sha2::{Digest, Sha256};
    use std::fs;
    let hash = |bytes: &[u8]| format!("{:x}", Sha256::digest(bytes));
    let root = std::env::temp_dir().join(format!(
        "openride-terrain-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let sources = root.join("sources");
    fs::create_dir(&sources).unwrap();
    let mut files = Vec::new();
    for name in ["road-preview.obj", "bounds.geojson", "osm-query.overpass"] {
        fs::write(root.join(name), b"synthetic test asset").unwrap();
        files.push(json!({"path":name,"sha256":hash(b"synthetic test asset")}));
    }
    let prep = json!({"schema_version":1,"kind":"openride.scenery-preparation","trainer_ready":false,
        "source_route_sha256":"test","files":files,"origin_lat_lon_elevation":[33.8015,-118.4015,45.],
        "points":[{"latitude":33.8015,"longitude":-118.4015,"elevation_m":55.,"source_sample":7}]}).to_string();
    let preparation_path = root.join("preparation.json");
    fs::write(&preparation_path, &prep).unwrap();
    let export = json!({"width":3,"height":3,"extent":{"xmin":-118.403,"xmax":-118.4,"ymin":33.8,"ymax":33.803,"spatialReference":{"wkid":4326}}}).to_string();
    let bytes = raster(&[10., 20., 30., 40., 50., 60., 70., 80., 90.]);
    fs::write(sources.join("terrain.tif"), &bytes).unwrap();
    fs::write(sources.join("usgs-export.json"), &export).unwrap();
    let manifest = json!({"schema_version":1,"kind":"openride.geographic-sources",
        "requested_bounds":{"west":-115.27586366430859_f64},
        "preparation_sha256":hash(prep.as_bytes()),"source_route_sha256":"test",
        "requests":{"terrain_export":{"pixelType":"F32","imageSR":"4326","renderingRule":"{\"rasterFunction\":\"None\"}"}},
        "files":{"terrain.tif":{"sha256":hash(&bytes)},"usgs-export.json":{"sha256":hash(export.as_bytes())}}});
    fs::write(sources.join("sources.json"), manifest.to_string()).unwrap();
    let destination = root.join("result");
    let output = terrain::attach(&preparation_path, &sources, &destination).unwrap();
    // Python source manifests must survive Rust parsing without a one-ULP drift.
    // OSM context verifies the embedded snapshot against the original manifest.
    assert_eq!(output["terrain"]["source_snapshot"], manifest);
    assert_eq!(output["points"][0]["elevation_m"], 55.);
    assert_eq!(output["trainer_ready"], false);
    assert_eq!(fs::read_to_string(&preparation_path).unwrap(), prep);
    assert!(terrain::attach(&preparation_path, &sources, &destination).is_err());
    fs::write(sources.join("terrain.tif"), b"tampered").unwrap();
    assert!(
        terrain::attach(&preparation_path, &sources, &root.join("bad"))
            .unwrap_err()
            .contains("checksum")
    );
    assert!(!root.join("bad").exists());
    fs::remove_dir_all(root).unwrap();
}
