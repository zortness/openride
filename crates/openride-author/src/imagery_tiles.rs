//! Chunk-local 2 m imagery with a shared global pixel lattice and one-pixel gutter.
use crate::{error, imagery, maps, Result};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, VecDeque},
    path::{Path, PathBuf},
};
pub const WIDTH: usize = 130;
pub struct Tiles {
    root: PathBuf,
    tiles: Vec<maps::Tile>,
    cache: VecDeque<(usize, Vec<u8>)>,
    pub sources: BTreeMap<String, String>,
    pub peak_decoded: usize,
}
impl Tiles {
    pub fn new(root: &Path, plan: &maps::Plan) -> Result<Self> {
        let root = root.join("maps").join(&plan.key);
        let mut tiles = vec![];
        let mut sources = BTreeMap::new();
        for tile in &plan.tiles {
            let dir = root.join(&tile.id);
            if !dir.join("imagery.receipt.json").exists() {
                continue;
            }
            if !maps::verified(&dir, "imagery") {
                return Err(error(
                    "invalid_imagery",
                    format!("Reacquire corrupt imagery tile {}", tile.id),
                ));
            }
            sources.insert(
                tile.id.clone(),
                format!(
                    "{:x}",
                    Sha256::digest(maps::read(&dir.join("imagery.receipt.json"))?)
                ),
            );
            tiles.push(tile.clone());
        }
        Ok(Self {
            root,
            tiles,
            cache: VecDeque::new(),
            sources,
            peak_decoded: 0,
        })
    }
    fn pixels(&mut self, index: usize) -> Result<&[u8]> {
        if let Some(position) = self.cache.iter().position(|v| v.0 == index) {
            let value = self.cache.remove(position).unwrap();
            self.cache.push_back(value);
        } else {
            if self.cache.len() == 8 {
                self.cache.pop_front();
            }
            let raw = maps::read(&self.root.join(&self.tiles[index].id).join("imagery.png"))?;
            self.cache.push_back((index, imagery::decode(&raw)?));
            self.peak_decoded = self.peak_decoded.max(self.cache.len());
        }
        Ok(&self.cache.back().unwrap().1)
    }
    pub fn bake(
        &mut self,
        cx: i32,
        cz: i32,
        geographic: impl Fn(f64, f64) -> [f64; 2],
    ) -> Result<(Vec<u8>, usize)> {
        let x0 = cx as f64 * 256. - 2.;
        let z0 = cz as f64 * 256. - 2.;
        let corners = [
            (x0, z0),
            (x0 + 260., z0),
            (x0, z0 + 260.),
            (x0 + 260., z0 + 260.),
        ]
        .map(|(x, z)| geographic(x, z));
        let min_lat = corners.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
        let max_lat = corners
            .iter()
            .map(|p| p[0])
            .fold(f64::NEG_INFINITY, f64::max);
        let min_lon = corners.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
        let max_lon = corners
            .iter()
            .map(|p| p[1])
            .fold(f64::NEG_INFINITY, f64::max);
        let candidates: Vec<_> = self
            .tiles
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                t.bounds[0] <= max_lon
                    && t.bounds[2] >= min_lon
                    && t.bounds[1] <= max_lat
                    && t.bounds[3] >= min_lat
            })
            .map(|(i, t)| (i, t.bounds))
            .collect();
        let mut pixels = vec![0; WIDTH * WIDTH * 4];
        let mut missing = 0;
        for y in 0..WIDTH {
            for x in 0..WIDTH {
                let [lat, lon] = geographic(x0 + (x as f64 + 0.5) * 2., z0 + (y as f64 + 0.5) * 2.);
                let mut sum = [0.; 4];
                let mut weight = 0.;
                for &(index, [w, s, e, n]) in &candidates {
                    if lat < s || lat > n || lon < w || lon > e {
                        continue;
                    }
                    let u = ((lon - w) / (e - w) * 1024. - 0.5).clamp(0., 1023.);
                    let v = ((n - lat) / (n - s) * 1024. - 0.5).clamp(0., 1023.);
                    let edge = (u.min(1023. - u).min(v.min(1023. - v)) / 8.).clamp(0.001, 1.);
                    let data = self.pixels(index)?;
                    let ix = u.floor() as usize;
                    let iy = v.floor() as usize;
                    for (px, py, a) in [
                        (ix, iy, (1. - u.fract()) * (1. - v.fract())),
                        ((ix + 1).min(1023), iy, u.fract() * (1. - v.fract())),
                        (ix, (iy + 1).min(1023), (1. - u.fract()) * v.fract()),
                        (
                            (ix + 1).min(1023),
                            (iy + 1).min(1023),
                            u.fract() * v.fract(),
                        ),
                    ] {
                        let p = &data[(py * 1024 + px) * 4..][..4];
                        let factor = a * edge * p[3] as f64 / 255.;
                        for c in 0..3 {
                            sum[c] += p[c] as f64 * factor;
                        }
                        weight += factor;
                    }
                }
                let out = &mut pixels[(y * WIDTH + x) * 4..][..4];
                if weight > 0. {
                    for c in 0..3 {
                        out[c] = (sum[c] / weight).round() as u8;
                    }
                    out[3] = 255;
                } else {
                    missing += 1;
                }
            }
        }
        if missing == WIDTH * WIDTH {
            return Ok((vec![], missing));
        }
        let mut raw = vec![];
        {
            let mut encoder = png::Encoder::new(&mut raw, WIDTH as u32, WIDTH as u32);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .map_err(|e| error("imagery_encode", e.to_string()))?
                .write_image_data(&pixels)
                .map_err(|e| error("imagery_encode", e.to_string()))?;
        }
        Ok((raw, missing))
    }
}
