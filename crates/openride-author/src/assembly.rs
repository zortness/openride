//! Continuous native terrain/road assembly on one shared metric lattice.
use crate::{cancelled, checked, error, lock, maps, open, package, Result, Snapshot};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    path::Path,
};
const R: f64 = 6_371_000.;
const GRID: f64 = 8.;
const CELL: f64 = 256.;
const BUFFER: f64 = 150.;
fn bad(message: impl Into<String>) -> crate::Error {
    error("build_course", message)
}
fn hash(raw: &[u8]) -> String {
    format!("{:x}", Sha256::digest(raw))
}
#[derive(Clone, Copy)]
struct Frame {
    origin: [f64; 3],
}
impl Frame {
    fn project(&self, p: [f64; 2]) -> Result<[f64; 2]> {
        let lat = p[0].to_radians();
        let base = self.origin[0].to_radians();
        let dl = (p[1] - self.origin[1]).to_radians();
        let x = R * lat.cos() * dl.sin();
        let z = -R * (base.cos() * lat.sin() - base.sin() * lat.cos() * dl.cos());
        if !x.is_finite() || !z.is_finite() || x.hypot(z) > 99000. {
            return Err(bad("Course exceeds 99 km projection radius"));
        }
        Ok([x, z])
    }
    fn geographic(&self, x: f64, z: f64) -> [f64; 2] {
        let radius = x.hypot(z);
        if radius == 0. {
            return [self.origin[0], self.origin[1]];
        }
        let c = (radius / R).asin();
        let lat = self.origin[0].to_radians();
        [
            (c.cos() * lat.sin() - z * c.sin() * lat.cos() / radius)
                .asin()
                .to_degrees(),
            self.origin[1]
                + (x * c.sin())
                    .atan2(radius * lat.cos() * c.cos() + z * lat.sin() * c.sin())
                    .to_degrees(),
        ]
    }
}
#[derive(Clone, Serialize)]
struct Point {
    distance_m: f64,
    latitude: f64,
    longitude: f64,
    elevation_m: f64,
    position: [f64; 3],
    source_sample: usize,
    break_before: bool,
    grade_percent: Option<f64>,
    #[serde(default)]
    elevated: bool,
}
#[derive(Clone, Serialize, Default)]
struct Mesh {
    vertices: Vec<[f64; 3]>,
    indices: Vec<usize>,
    normals: Vec<[f64; 3]>,
}
fn normal(v: [f64; 3]) -> [f64; 3] {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if n == 0. {
        [0., 1., 0.]
    } else {
        v.map(|x| x / n)
    }
}
fn length(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}
fn profile(snapshot: &Snapshot, frame: Frame, accept_steep: bool) -> Result<Vec<Point>> {
    let course = snapshot
        .course
        .as_ref()
        .ok_or_else(|| bad("Build/rebuild the course before compiling scenery"))?;
    if course.unresolved > 0 {
        return Err(error(
            "review_required",
            format!(
                "Review {} unresolved course occurrences before building the package",
                course.unresolved
            ),
        ));
    }
    let review = snapshot.review.as_ref().unwrap();
    if review.document.geometry_source != review.document.elevation_source {
        return Err(error("elevation_alignment","Package construction currently requires the same geometry/elevation source; cross-source alignment needs review"));
    }
    let mut points: Vec<Point> = Vec::new();
    let mut previous_height = None;
    for section in &course.sections {
        if section.break_before || section.geometry.is_empty() {
            return Err(bad("Course still has a gap or missing geometry"));
        }
        let end_height = section
            .recorded_elevation_m
            .filter(|h| h.is_finite())
            .ok_or_else(|| bad("Course has missing recorded elevation"))?;
        let start_height = previous_height.unwrap_or(end_height);
        let mut station = 0.;
        for (i, &p) in section.geometry.iter().enumerate() {
            if i > 0 {
                let a = section.geometry[i - 1];
                station += ride_core::haversine(a[0], a[1], p[0], p[1]);
            }
            let t = if section.length_m > 0. {
                (station / section.length_m).clamp(0., 1.)
            } else {
                1.
            };
            let height = start_height + (end_height - start_height) * t;
            let projected = frame.project(p)?;
            let mut point = Point {
                distance_m: 0.,
                latitude: p[0],
                longitude: p[1],
                elevation_m: height,
                position: [projected[0], height - frame.origin[2], projected[1]],
                source_sample: section.source_sample,
                break_before: points.is_empty(),
                grade_percent: None,
                elevated: false,
            };
            if let Some(previous) = points.last() {
                let step = ride_core::haversine(previous.latitude, previous.longitude, p[0], p[1]);
                if step < 0.01 {
                    if (height - previous.elevation_m).abs() > 0.01 {
                        return Err(bad(
                            "Elevation changes without horizontal travel; review the profile",
                        ));
                    }
                    continue;
                }
                let grade = 100. * (height - previous.elevation_m) / step;
                if grade.abs() > 100. {
                    return Err(bad(
                        "Unresolved grade exceeds 100%; repair or review the source profile",
                    ));
                }
                if grade.abs() > 15. && !accept_steep {
                    return Err(error("steep_review","Recorded grade exceeds 15%. Inspect it, then explicitly retain steep grades for an unvalidated preview"));
                }
                point.distance_m = previous.distance_m + step;
                point.grade_percent = Some(grade);
            }
            points.push(point);
            if points.len() > 10000 {
                return Err(bad(
                    "Canonical course exceeds the 10,000-point runtime budget",
                ));
            }
        }
        previous_height = Some(end_height);
    }
    if points.len() < 2 || points.last().unwrap().distance_m > 200000. {
        return Err(bad("Package requires 2-10,000 points and at most 200 km"));
    }
    Ok(points)
}
fn road_profile(
    root: &Path,
    snapshot: &Snapshot,
    plan: &maps::Plan,
    accept_steep: bool,
    cancel: Option<&Path>,
) -> Result<(Frame, Vec<Point>)> {
    let course = snapshot.course.as_ref().unwrap();
    if course.unresolved > 0 {
        return Err(error(
            "review_required",
            format!(
                "Review {} uncertain road stretches before building",
                course.unresolved
            ),
        ));
    }
    let metadata = course.roads.as_ref().unwrap();
    let frame = Frame {
        origin: [plan.origin[0], plan.origin[1], 0.],
    };
    let mut terrain = Terrain::new(root, plan, frame)?;
    let graph = maps::street_graph(root, plan, cancel)?;
    let mut points: Vec<Point> = vec![];
    let mut structures = vec![];
    for section in &course.sections {
        cancelled(cancel)?;
        if section.break_before || section.geometry.is_empty() {
            return Err(bad("Road sequence has a disconnected stretch"));
        }
        let ways = metadata.section_ways.get(&section.source_sample);
        for (i, &p) in section.geometry.iter().enumerate() {
            let way = ways
                .and_then(|w| w.get(i.saturating_sub(1)))
                .or(section.way.as_ref());
            let tags = way
                .and_then(|id| graph.features.get(&format!("way/{id}")))
                .map(|f| &f.tags);
            let bridge = tags.is_some_and(|t| {
                t.get("bridge").is_some_and(|v| v != "no")
                    || t.get("layer").is_some_and(|v| v != "0")
            });
            let tunnel = tags.is_some_and(|t| t.get("tunnel").is_some_and(|v| v != "no"));
            if tunnel {
                return Err(error("tunnel_review","This road sequence includes a tunnel. Tunnel interior geometry is not implemented; retain the road choice, but this preview cannot be built yet."));
            }
            let start = points
                .last()
                .map(|v| [v.latitude, v.longitude])
                .unwrap_or(p);
            let length = ride_core::haversine(start[0], start[1], p[0], p[1]);
            if !points.is_empty() && length < 0.01 {
                continue;
            }
            // Keep long-road profiles within the canonical route payload budget;
            // road meshes are independently subdivided to <=10 m below.
            let count = (length / 30.).ceil().max(1.) as usize;
            for step in 1..=count {
                let t = step as f64 / count as f64;
                let q = [
                    start[0] + (p[0] - start[0]) * t,
                    start[1] + (p[1] - start[1]) * t,
                ];
                let [x, z] = frame.project(q)?;
                let height = terrain.sample(x, z)?;
                let distance_m = points
                    .last()
                    .map(|v| {
                        v.distance_m + ride_core::haversine(v.latitude, v.longitude, q[0], q[1])
                    })
                    .unwrap_or(0.);
                points.push(Point {
                    distance_m,
                    latitude: q[0],
                    longitude: q[1],
                    elevation_m: height,
                    position: [x, height, z],
                    source_sample: section.source_sample,
                    break_before: points.is_empty(),
                    grade_percent: None,
                    elevated: bridge,
                });
                structures.push(bridge);
                if points.len() > 10000 {
                    return Err(bad("Generated road profile exceeds 10,000 stations"));
                }
            }
        }
    }
    if points.len() < 2 {
        return Err(bad("Road sequence has fewer than two positions"));
    }
    // Smooth terrain at a fixed physical scale, independent of GPS sample density.
    let sampled: Vec<_> = points.iter().map(|p| p.elevation_m).collect();
    for i in 0..points.len() {
        if structures[i] {
            continue;
        }
        let mut sum = 0.;
        let mut weight = 0.;
        for j in i.saturating_sub(32)..(i + 33).min(points.len()) {
            let d = (points[j].distance_m - points[i].distance_m).abs();
            if d <= 30. && !structures[j] {
                let w = 1. - d / 31.;
                sum += sampled[j] * w;
                weight += w;
            }
        }
        points[i].elevation_m = sum / weight;
    }
    let mut i = 0;
    while i < points.len() {
        if !structures[i] {
            i += 1;
            continue;
        }
        let start = i;
        while i < points.len() && structures[i] {
            i += 1;
        }
        if start == 0 || i == points.len() {
            return Err(error("bridge_review","Bridge profile requires terrain-supported road at both ends; extend the road sequence before building"));
        }
        let (a, b) = (start - 1, i);
        let span = points[b].distance_m - points[a].distance_m;
        for j in start..i {
            let t = (points[j].distance_m - points[a].distance_m) / span;
            points[j].elevation_m = points[a].elevation_m * (1. - t) + points[b].elevation_m * t;
        }
    }
    let origin_height = points[0].elevation_m;
    for i in 0..points.len() {
        points[i].position[1] = points[i].elevation_m - origin_height;
        if i > 0 {
            let grade = 100. * (points[i].elevation_m - points[i - 1].elevation_m)
                / (points[i].distance_m - points[i - 1].distance_m);
            if grade.abs() > 100. {
                return Err(error(
                    "terrain_profile",
                    "Terrain-derived road grade exceeds 100%; inspect elevation coverage",
                ));
            }
            if grade.abs() > 15. && !accept_steep {
                return Err(error("steep_review","Terrain-derived road grade exceeds 15%; inspect it before retaining it in a preview"));
            }
            points[i].grade_percent = Some(grade);
        }
    }
    Ok((
        Frame {
            origin: [frame.origin[0], frame.origin[1], origin_height],
        },
        points,
    ))
}
struct Dem {
    width: usize,
    height: usize,
    values: Vec<f64>,
}
struct Terrain<'a> {
    root: &'a Path,
    plan: &'a maps::Plan,
    frame: Frame,
    index: BTreeMap<(i32, i32), Vec<usize>>,
    cache: VecDeque<(usize, Dem)>,
    max_overlap: f64,
}
impl<'a> Terrain<'a> {
    fn new(root: &'a Path, plan: &'a maps::Plan, frame: Frame) -> Result<Self> {
        let mut index: BTreeMap<(i32, i32), Vec<usize>> = BTreeMap::new();
        for (i, tile) in plan.tiles.iter().enumerate() {
            if !maps::verified(&root.join("maps").join(&plan.key).join(&tile.id), "terrain") {
                return Err(error(
                    "missing_terrain",
                    format!("Acquire or repair terrain tile {} before building", tile.id),
                ));
            }
            let [w, s, e, n] = tile.bounds;
            let corners = [[s, w], [s, e], [n, w], [n, e]]
                .map(|p| frame.project(p))
                .into_iter()
                .collect::<Result<Vec<_>>>()?;
            let xmin = corners.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
            let xmax = corners
                .iter()
                .map(|p| p[0])
                .fold(f64::NEG_INFINITY, f64::max);
            let zmin = corners.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
            let zmax = corners
                .iter()
                .map(|p| p[1])
                .fold(f64::NEG_INFINITY, f64::max);
            for x in ((xmin - 10.) / 1000.).floor() as i32..=((xmax + 10.) / 1000.).floor() as i32 {
                for z in
                    ((zmin - 10.) / 1000.).floor() as i32..=((zmax + 10.) / 1000.).floor() as i32
                {
                    index.entry((x, z)).or_default().push(i);
                }
            }
        }
        Ok(Self {
            root,
            plan,
            frame,
            index,
            cache: VecDeque::new(),
            max_overlap: 0.,
        })
    }
    fn grid(&mut self, key: usize) -> Result<&Dem> {
        if let Some(i) = self.cache.iter().position(|(k, _)| *k == key) {
            let value = self.cache.remove(i).unwrap();
            self.cache.push_back(value);
        } else {
            let dir = self
                .root
                .join("maps")
                .join(&self.plan.key)
                .join(&self.plan.tiles[key].id);
            let export: Value = serde_json::from_slice(&maps::read(&dir.join("export.json"))?)
                .map_err(|e| bad(e.to_string()))?;
            let terrain = content_builder::terrain::build(
                &maps::read(&dir.join("terrain.tif"))?,
                &export,
                &json!({"origin_lat_lon_elevation":self.frame.origin,"points":[]}),
            )
            .map_err(bad)?;
            if terrain["nodata_pixels"] != 0 {
                return Err(bad("Terrain has missing pixels"));
            }
            let width = terrain["width"].as_u64().unwrap() as usize;
            let height = terrain["height"].as_u64().unwrap() as usize;
            let values = terrain["mesh"]["vertices"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v[1].as_f64().unwrap())
                .collect();
            if self.cache.len() >= 8 {
                self.cache.pop_front();
            }
            self.cache.push_back((
                key,
                Dem {
                    width,
                    height,
                    values,
                },
            ));
        }
        Ok(&self.cache.back().unwrap().1)
    }
    fn sample(&mut self, x: f64, z: f64) -> Result<f64> {
        let [lat, lon] = self.frame.geographic(x, z);
        let keys = self
            .index
            .get(&((x / 1000.).floor() as i32, (z / 1000.).floor() as i32))
            .cloned()
            .unwrap_or_default();
        let mut values = vec![];
        for key in keys {
            let [w, s, e, n] = self.plan.tiles[key].bounds;
            if lon < w || lon > e || lat < s || lat > n {
                continue;
            }
            let grid = self.grid(key)?;
            let col = (lon - w) / (e - w) * grid.width as f64 - 0.5;
            let row = (n - lat) / (n - s) * grid.height as f64 - 0.5;
            if col < 0.
                || row < 0.
                || col > (grid.width - 1) as f64
                || row > (grid.height - 1) as f64
            {
                continue;
            }
            let ix = (col as usize).min(grid.width - 2);
            let iz = (row as usize).min(grid.height - 2);
            let fx = col - ix as f64;
            let fz = row - iz as f64;
            let i = iz * grid.width + ix;
            let q = [
                grid.values[i],
                grid.values[i + 1],
                grid.values[i + grid.width],
                grid.values[i + grid.width + 1],
            ];
            let value =
                (q[0] * (1. - fx) + q[1] * fx) * (1. - fz) + (q[2] * (1. - fx) + q[3] * fx) * fz;
            let weight = col
                .min(row)
                .min((grid.width - 1) as f64 - col)
                .min((grid.height - 1) as f64 - row)
                .min(8.)
                .powi(2);
            let range = q.iter().copied().fold(f64::NEG_INFINITY, f64::max)
                - q.iter().copied().fold(f64::INFINITY, f64::min);
            values.push((value, weight, range));
        }
        if values.is_empty() {
            return Err(error(
                "missing_terrain",
                "Course extends beyond acquired terrain coverage",
            ));
        }
        let spread = values.iter().map(|v| v.0).fold(f64::NEG_INFINITY, f64::max)
            - values.iter().map(|v| v.0).fold(f64::INFINITY, f64::min);
        self.max_overlap = self.max_overlap.max(spread);
        let allowance = values.iter().map(|v| v.2).fold(2., f64::max).min(10.);
        if spread > allowance {
            return Err(bad(format!(
                "Terrain overlap disagreement {spread:.2} m exceeds {allowance:.2} m"
            )));
        }
        let weight: f64 = values.iter().map(|v| v.1).sum();
        Ok(if weight > 0. {
            values.iter().map(|v| v.0 * v.1).sum::<f64>() / weight
        } else {
            values[0].0
        })
    }
}
#[derive(Clone)]
struct Segment {
    a: [f64; 3],
    b: [f64; 3],
    elevated: bool,
    cut_only: bool,
}
struct Geometry<'a> {
    terrain: Terrain<'a>,
    segments: BTreeMap<(i32, i32), Vec<Segment>>,
    heights: BTreeMap<(i32, i32), f64>,
    scenery: crate::scenery_mesh::World,
}
impl Geometry<'_> {
    fn closest(&self, x: f64, z: f64) -> (f64, f64, bool, bool) {
        let mut found = (f64::INFINITY, 0., true, false);
        let mut near = vec![];
        if let Some(segments) = self
            .segments
            .get(&((x / CELL).floor() as i32, (z / CELL).floor() as i32))
        {
            for s in segments {
                let dx = s.b[0] - s.a[0];
                let dz = s.b[2] - s.a[2];
                let denom = dx * dx + dz * dz;
                if denom == 0. {
                    continue;
                }
                let t = (((x - s.a[0]) * dx + (z - s.a[2]) * dz) / denom).clamp(0., 1.);
                let distance = (x - s.a[0] - t * dx).hypot(z - s.a[2] - t * dz);
                let y = s.a[1] + t * (s.b[1] - s.a[1]) - 0.25;
                if distance < 30. {
                    near.push(y);
                }
                if distance < found.0 {
                    found = (distance, y, !s.elevated, s.cut_only);
                }
            }
        }
        if near.iter().any(|y| (y - found.1).abs() > 3.) {
            found.2 = false;
        }
        found
    }
    fn height(&mut self, x: i32, z: i32) -> Result<f64> {
        if let Some(v) = self.heights.get(&(x, z)) {
            return Ok(*v);
        }
        let px = x as f64 * GRID;
        let pz = z as f64 * GRID;
        let mut y = self.terrain.sample(px, pz)?;
        let (d, road, conform, cut_only) = self.closest(px, pz);
        if let Some(water) = self.scenery.water_level(px, pz) {
            y = y.min(water - 0.4);
        } else if conform {
            let w = ((30. - d) / 12.).clamp(0., 1.);
            let w = w * w * (3. - 2. * w);
            // Reviewed manual/trace segments need clearance too, but must not
            // grow an embankment when their structure identity is unknown.
            let target = if cut_only { road.min(y) } else { road };
            y += (target - y) * w;
        }
        if self.heights.len() >= 16384 {
            self.heights.clear();
        }
        self.heights.insert((x, z), y);
        Ok(y)
    }
    fn vertex(
        &mut self,
        mesh: &mut Mesh,
        lookup: &mut BTreeMap<(i32, i32), usize>,
        x: i32,
        z: i32,
    ) -> Result<usize> {
        if let Some(i) = lookup.get(&(x, z)) {
            return Ok(*i);
        }
        let i = mesh.vertices.len();
        mesh.vertices
            .push([x as f64 * GRID, self.height(x, z)?, z as f64 * GRID]);
        mesh.normals.push(normal([
            self.height(x - 1, z)? - self.height(x + 1, z)?,
            2. * GRID,
            self.height(x, z - 1)? - self.height(x, z + 1)?,
        ]));
        lookup.insert((x, z), i);
        Ok(i)
    }
}
fn ribbon(points: &[[f64; 3]]) -> Result<Mesh> {
    let mut mesh = Mesh::default();
    let mut cuts = vec![0];
    let directions: Vec<_> = points
        .windows(2)
        .map(|w| {
            let dx = w[1][0] - w[0][0];
            let dz = w[1][2] - w[0][2];
            let n = dx.hypot(dz);
            [dx / n, dz / n]
        })
        .collect();
    for i in 1..points.len() - 1 {
        if (directions[i - 1][0] + directions[i][0]).hypot(directions[i - 1][1] + directions[i][1])
            < 0.001
        {
            cuts.push(i);
        }
    }
    cuts.push(points.len() - 1);
    for range in cuts.windows(2) {
        let first = range[0];
        let last = range[1];
        let base = mesh.vertices.len();
        for (i, p) in points.iter().enumerate().take(last + 1).skip(first) {
            let before = directions[if i == first { first } else { i - 1 }];
            let after = directions[if i == last { last - 1 } else { i }];
            let nx = -before[1] - after[1];
            let nz = before[0] + after[0];
            let d = nx.hypot(nz);
            let nx = nx / d;
            let nz = nz / d;
            let scale = 3. / (-nx * after[1] + nz * after[0]).max(0.5);
            mesh.vertices.extend([
                [p[0] + nx * scale, p[1], p[2] + nz * scale],
                [p[0] - nx * scale, p[1], p[2] - nz * scale],
            ]);
            if i > first {
                let k = base + (i - first) * 2;
                mesh.indices.extend([k - 2, k - 1, k, k - 1, k + 1, k]);
            }
        }
    }
    mesh.normals = vec![[0.; 3]; mesh.vertices.len()];
    for ids in mesh.indices.chunks_exact(3) {
        let a = mesh.vertices[ids[0]];
        let b = mesh.vertices[ids[1]];
        let c = mesh.vertices[ids[2]];
        if length(a, b).max(length(a, c)).max(length(b, c)) > 32. {
            return Err(bad("Road triangle exceeds 32 m chunk overhang"));
        }
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let mut n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        if n[1] < 0. {
            n = n.map(|x| -x);
        }
        for &i in ids {
            for (j, value) in n.iter().enumerate() {
                mesh.normals[i][j] += value;
            }
        }
    }
    mesh.normals = mesh.normals.into_iter().map(normal).collect();
    Ok(mesh)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Build {
    pub schema_version: u32,
    pub key: String,
    pub path: String,
    pub course_sha256: String,
    pub chunks: usize,
    pub length_m: f64,
    pub reused_chunks: usize,
    #[serde(default)]
    pub scenery_revision: String,
    #[serde(default)]
    pub map_revision: String,
    #[serde(default)]
    pub project_name: String,
    #[serde(default)]
    pub current: bool,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub imagery_chunks: usize,
    #[serde(default)]
    pub imagery_gaps: usize,
    #[serde(default)]
    pub export: Option<String>,
}
pub fn load(root: &Path) -> Result<Option<Build>> {
    let path = root.join("build/course/current.json");
    if !path.exists() {
        return Ok(None);
    }
    let raw = maps::read(&path)?;
    if raw.len() > 2 * 1024 * 1024 {
        return Err(bad("Oversized build record"));
    }
    let build: Build = serde_json::from_slice(&raw).map_err(|e| bad(e.to_string()))?;
    if build.schema_version != 1
        || build.key.len() != 64
        || !build.key.bytes().all(|b| b.is_ascii_hexdigit())
        || build.path != format!("build/course/{}", build.key)
    {
        return Err(bad("Invalid saved build path"));
    }
    Ok(Some(build))
}
pub fn map_revision(root: &Path, plan: &maps::Plan) -> Result<String> {
    let mut receipts = BTreeMap::new();
    for tile in &plan.tiles {
        for layer in ["terrain", "imagery"] {
            let p = root
                .join("maps")
                .join(&plan.key)
                .join(&tile.id)
                .join(format!("{layer}.receipt.json"));
            if p.exists() {
                receipts.insert(format!("{}/{layer}", tile.id), hash(&maps::read(&p)?));
            }
        }
    }
    Ok(hash(&serde_json::to_vec(&receipts).unwrap()))
}
pub fn build(
    root: &Path,
    revision: &str,
    accept_steep: bool,
    cancel: Option<&Path>,
) -> Result<Snapshot> {
    let _guard = lock(root)?;
    let current = checked(root, revision)?;
    cancelled(cancel)?;
    let plan = maps::plan(&current)?;
    let course = current
        .course
        .as_ref()
        .ok_or_else(|| bad("Build the course first"))?;
    let (frame, points) = if course.roads.is_some() {
        road_profile(root, &current, &plan, accept_steep, cancel)?
    } else {
        let first_height = course
            .sections
            .first()
            .and_then(|s| s.recorded_elevation_m)
            .ok_or_else(|| bad("Missing initial elevation"))?;
        let frame = Frame {
            origin: [plan.origin[0], plan.origin[1], first_height],
        };
        let points = profile(&current, frame, accept_steep)?;
        (frame, points)
    };
    // Reject an oversized route before preparing imagery or generating any chunks.
    let route=serde_json::to_vec(&json!({"points":points,"mode":"gps","validated":false,"projection":{"kind":"spherical_orthographic","radius_m":R,"origin":frame.origin}})).unwrap();
    if route.len() > ride_core::MAX_ROUTE_PROFILE_BYTES {
        return Err(bad(format!(
            "Canonical route exceeds 8 MiB ({} bytes, {} points)",
            route.len(),
            points.len()
        )));
    }
    let course_raw = serde_json::to_vec(course).unwrap();
    let course_hash = hash(&course_raw);
    let mut sources = BTreeMap::new();
    for tile in &plan.tiles {
        cancelled(cancel)?;
        let dir = root.join("maps").join(&plan.key).join(&tile.id);
        if !maps::verified(&dir, "terrain") {
            return Err(error(
                "missing_terrain",
                format!("Terrain tile {} is missing or corrupt", tile.id),
            ));
        }
        sources.insert(
            tile.id.clone(),
            hash(&maps::read(&dir.join("terrain.receipt.json"))?),
        );
    }
    let mut imagery = crate::imagery_tiles::Tiles::new(root, &plan)?;
    let key=hash(&serde_json::to_vec(&json!({"recipe":hash(include_bytes!("assembly.rs")),"imagery_recipe":hash(include_bytes!("imagery_tiles.rs")),"scenery_recipe":hash(include_bytes!("scenery_mesh.rs")),"scenery":current.scenery.revision,"dependencies":hash(include_bytes!("../../../Cargo.lock")),"name":current.project.name,"course":course_hash,"terrain":sources,"imagery":imagery.sources,"origin":frame.origin,"accept_steep":accept_steep})).unwrap());
    let base = root.join("build/course");
    fs::create_dir_all(&base)?;
    let destination = base.join(&key);
    let staging = root.join("jobs").join(format!("build-{key}"));
    fs::create_dir_all(&staging)?;
    let cache = root.join("jobs/chunk-cache");
    fs::create_dir_all(&cache)?;
    let mut terrain = Terrain::new(root, &plan, frame)?;
    let graph = maps::street_graph(root, &plan, cancel)?;
    let elevated: BTreeSet<_> = graph
        .features
        .values()
        .filter(|f| {
            f.kind == "way"
                && (f.tags.get("bridge").is_some_and(|v| v != "no")
                    || f.tags.get("tunnel").is_some_and(|v| v != "no")
                    || f.tags.get("layer").is_some_and(|v| v != "0"))
        })
        .map(|f| f.id)
        .collect();
    let raised_samples: BTreeSet<_> = course
        .sections
        .iter()
        .filter(|s| s.way.is_some_and(|id| elevated.contains(&id)))
        .map(|s| s.source_sample)
        .collect();
    let manual_samples: BTreeSet<_> = course
        .sections
        .iter()
        .filter(|s| s.way.is_none())
        .map(|s| s.source_sample)
        .collect();
    let mut dense = vec![points[0].position];
    let mut segments: BTreeMap<(i32, i32), Vec<Segment>> = BTreeMap::new();
    let mut cells = BTreeSet::new();
    for w in points.windows(2) {
        let count = ((w[1].distance_m - w[0].distance_m) / 10.).ceil().max(1.) as usize;
        for i in 1..=count {
            let t = i as f64 / count as f64;
            let b = std::array::from_fn(|j| {
                w[0].position[j] + t * (w[1].position[j] - w[0].position[j])
            });
            let a = *dense.last().unwrap();
            dense.push(b);
            if dense.len() > 32768 {
                return Err(bad("Road exceeds 32,768 mesh stations"));
            }
            let segment = Segment {
                a,
                b,
                elevated: if course.roads.is_some() {
                    w[0].elevated || w[1].elevated
                } else {
                    raised_samples.contains(&w[0].source_sample)
                        || raised_samples.contains(&w[1].source_sample)
                },
                cut_only: manual_samples.contains(&w[0].source_sample)
                    || manual_samples.contains(&w[1].source_sample),
            };
            for x in ((a[0].min(b[0]) - BUFFER - 16.) / CELL).floor() as i32
                ..=((a[0].max(b[0]) + BUFFER + 16.) / CELL).floor() as i32
            {
                for z in ((a[2].min(b[2]) - BUFFER - 16.) / CELL).floor() as i32
                    ..=((a[2].max(b[2]) + BUFFER + 16.) / CELL).floor() as i32
                {
                    segments.entry((x, z)).or_default().push(segment.clone());
                    cells.insert((x, z));
                }
            }
        }
    }
    if cells.len() > 4096 {
        return Err(bad("Corridor exceeds 4096 chunks"));
    }
    let road = ribbon(&dense)?;
    let mut roads: BTreeMap<(i32, i32), Vec<[usize; 3]>> = BTreeMap::new();
    for triangle in road.indices.chunks_exact(3) {
        let key = (
            (triangle.iter().map(|i| road.vertices[*i][0]).sum::<f64>() / 3. / CELL).floor() as i32,
            (triangle.iter().map(|i| road.vertices[*i][2]).sum::<f64>() / 3. / CELL).floor() as i32,
        );
        roads
            .entry(key)
            .or_default()
            .push([triangle[0], triangle[1], triangle[2]]);
        cells.insert(key);
    }
    let scenery = crate::scenery_mesh::World::prepare(
        &current.scenery,
        &cells,
        &dense,
        |p| frame.project(p),
        |x, z| terrain.sample(x, z),
        frame.origin[2],
    )?;
    let mut geometry = Geometry {
        terrain,
        segments,
        heights: BTreeMap::new(),
        scenery,
    };
    let mut entries = vec![];
    let mut reused = 0;
    for (number, &(cx, cz)) in cells.iter().enumerate() {
        cancelled(cancel)?;
        let mut terrain = Mesh::default();
        let mut lookup = BTreeMap::new();
        for z in cz * 32..(cz + 1) * 32 {
            for x in cx * 32..(cx + 1) * 32 {
                if geometry
                    .closest((x as f64 + 0.5) * GRID, (z as f64 + 0.5) * GRID)
                    .0
                    > BUFFER
                {
                    continue;
                }
                let a = geometry.vertex(&mut terrain, &mut lookup, x, z)?;
                let b = geometry.vertex(&mut terrain, &mut lookup, x + 1, z)?;
                let c = geometry.vertex(&mut terrain, &mut lookup, x, z + 1)?;
                let d = geometry.vertex(&mut terrain, &mut lookup, x + 1, z + 1)?;
                terrain.indices.extend([a, b, c, b, d, c]);
            }
        }
        let mut ribbon = Mesh::default();
        let mut mapping = BTreeMap::new();
        if let Some(triangles) = roads.get(&(cx, cz)) {
            for ids in triangles {
                for &id in ids {
                    let next = ribbon.vertices.len();
                    let i = *mapping.entry(id).or_insert_with(|| {
                        ribbon.vertices.push(road.vertices[id]);
                        ribbon.normals.push(road.normals[id]);
                        next
                    });
                    ribbon.indices.push(i);
                }
            }
        }
        if terrain.indices.is_empty() && ribbon.indices.is_empty() {
            continue;
        }
        for mesh in [&terrain, &ribbon] {
            if mesh.vertices.len() > 30000
                || mesh.indices.len() > 90000
                || mesh
                    .vertices
                    .iter()
                    .flatten()
                    .any(|v| !v.is_finite() || v.abs() > 100000.)
            {
                return Err(bad("Chunk mesh exceeds runtime limits"));
            }
        }
        let scenery = geometry
            .scenery
            .chunk(cx, cz, |x, z| geometry.terrain.sample(x, z))?;
        let raw = serde_json::to_vec(&json!({"terrain":terrain,"road":ribbon,"lines":[],"buildings":scenery.buildings,"roofs":scenery.roofs,"water":scenery.water,"vegetation":scenery.vegetation})).unwrap();
        if raw.len() > 2 * 1024 * 1024 {
            return Err(bad("Chunk exceeds 2 MiB"));
        }
        let digest = hash(&raw);
        let cached = cache.join(format!("{digest}.json"));
        if cached.exists() && hash(&maps::read(&cached)?) == digest {
            reused += 1;
        } else {
            maps::atomic_file(&cached, &raw)?;
        }
        let name = format!("chunk_{cx}_{cz}.json");
        let staged = staging.join(&name);
        if !staged.exists() {
            fs::hard_link(&cached, &staged)?;
        } else if hash(&maps::read(&staged)?) != digest {
            maps::atomic_file(&staged, &raw)?;
        }
        let mut entry = json!({"x":cx,"z":cz,"path":name,"bytes":raw.len(),"sha256":digest});
        if !imagery.sources.is_empty() {
            let (image, missing) = imagery.bake(cx, cz, |x, z| frame.geographic(x, z))?;
            entry["imagery_missing_pixels"] = json!(missing);
            if !image.is_empty() {
                let name = format!("imagery_{cx}_{cz}.png");
                maps::atomic_file(&staging.join(&name), &image)?;
                entry["imagery"] = json!({"path":name,"bytes":image.len(),"sha256":hash(&image)});
            }
        }
        entries.push(entry);
        if number % 50 == 0 {
            maps::atomic_file(
                &root.join("jobs/build-progress.json"),
                &serde_json::to_vec(&json!({"chunks":entries.len(),"total_cells":cells.len()}))
                    .unwrap(),
            )?;
            println!(
                "{}",
                json!({"protocol_version":1,"event":"progress","action":"build-package","chunks":entries.len(),"total_cells":cells.len()})
            );
        }
    }
    let mut coarse = Mesh::default();
    let mut coarse_lookup = BTreeMap::new();
    for entry in &entries {
        let cx = entry["x"].as_i64().unwrap() as i32;
        let cz = entry["z"].as_i64().unwrap() as i32;
        for z in [cz * 32, cz * 32 + 16] {
            for x in [cx * 32, cx * 32 + 16] {
                if geometry
                    .closest((x + 8) as f64 * GRID, (z + 8) as f64 * GRID)
                    .0
                    > BUFFER
                {
                    continue;
                }
                let a = geometry.vertex(&mut coarse, &mut coarse_lookup, x, z)?;
                let b = geometry.vertex(&mut coarse, &mut coarse_lookup, x + 16, z)?;
                let c = geometry.vertex(&mut coarse, &mut coarse_lookup, x, z + 16)?;
                let d = geometry.vertex(&mut coarse, &mut coarse_lookup, x + 16, z + 16)?;
                coarse.indices.extend([a, b, c, b, d, c]);
            }
        }
    }
    if coarse.vertices.len() > 30000 || coarse.indices.len() > 90000 {
        return Err(bad("Coarse terrain exceeds runtime budget"));
    }
    let coarse = serde_json::to_vec(&coarse).unwrap();
    if coarse.len() > 2 * 1024 * 1024 {
        return Err(bad("Coarse terrain exceeds 2 MiB"));
    }
    let provenance=serde_json::to_vec(&json!({"kind":"openride.terrain-provenance","schema_version":1,"source_revision":current.source_revision,"course_sha256":course_hash,"tiles":sources,"assembly":{"recipe":"native-continuous-v1","grid_m":GRID,"max_overlap_difference_m":geometry.terrain.max_overlap,"overlap_policy":"Edge-weighted feather bounded by local DEM range and 10 m cap","terrain_conformance":"Presentation only; bridge/tunnel/layer and stacked heights excluded","steep_segments_retained":accept_steep}})).unwrap();
    let mut manifest = json!({"kind":"openride.stream-preview","schema_version":1,"trainer_ready":false,"name":current.project.name,"tile_m":256,"neighborhood_radius_cells":1,"max_live_chunks":9,"triangle_overhang_bound_m":32,"review_status":"unvalidated-preview","chunks":entries,"origin_lat_lon_elevation":frame.origin,"attribution":plan.attribution,"presentation":{"terrain_corridor":true,"inner_m":18,"outer_m":30,"clearance_m":0.25}});
    manifest["scenery"] = json!({"schema_version":1,"revision":current.scenery.revision,"features":current.scenery.features.len(),"warnings":geometry.scenery.warnings,"water_height_policy":"Explicit elevation or minimum sampled boundary DEM; preview only","vegetation_policy":"Deterministic global 8 m lattice, course clearance, water/building/exclusion masks"});
    if !imagery.sources.is_empty() {
        manifest["tiled_imagery"] = json!({"schema_version":1,"pixel_m":2,"width":130,"gutter_pixels":1,"sources":imagery.sources,"peak_decoded_source_tiles":imagery.peak_decoded,"chunks_with_imagery":entries.iter().filter(|e|e.get("imagery").is_some()).count(),"chunks_with_gaps":entries.iter().filter(|e|e["imagery_missing_pixels"].as_u64().unwrap_or(0)>0).count()});
    }
    for (key, name, raw) in [
        ("coarse_terrain", "coarse-terrain.json", coarse),
        ("route", "route.json", route),
        ("terrain_provenance", "terrain-provenance.json", provenance),
    ] {
        manifest[key] = json!({"path":name,"bytes":raw.len(),"sha256":hash(&raw)});
        maps::atomic_file(&staging.join(name), &raw)?;
    }
    maps::atomic_file(
        &staging.join("manifest.json"),
        &serde_json::to_vec(&manifest).unwrap(),
    )?;
    package::validate(&staging)?;
    cancelled(cancel)?;
    if destination.exists() {
        package::validate(&destination)?;
    } else {
        fs::rename(&staging, &destination)?;
    }
    let record = Build {
        schema_version: 1,
        key: key.clone(),
        path: format!("build/course/{key}"),
        course_sha256: course_hash,
        chunks: entries.len(),
        length_m: points.last().unwrap().distance_m,
        reused_chunks: reused,
        scenery_revision: current.scenery.revision.clone(),
        map_revision: map_revision(root, &plan)?,
        project_name: current.project.name.clone(),
        current: true,
        warnings: geometry.scenery.warnings.clone(),
        imagery_chunks: entries
            .iter()
            .filter(|e| e.get("imagery").is_some())
            .count(),
        imagery_gaps: entries
            .iter()
            .filter(|e| {
                e.get("imagery").is_none() || e["imagery_missing_pixels"].as_u64().unwrap_or(0) > 0
            })
            .count(),
        export: None,
    };
    maps::atomic_file(
        &base.join("current.json"),
        &serde_json::to_vec_pretty(&record).unwrap(),
    )?;
    open(root)
}
pub fn export(root: &Path, revision: &str, cancel: Option<&Path>) -> Result<Snapshot> {
    let _guard = lock(root)?;
    let current = checked(root, revision)?;
    let mut build = current
        .build
        .ok_or_else(|| bad("Build a package before exporting"))?;
    let name: String = current
        .project
        .name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let relative = format!("exports/{}-{}.zip", name, &build.key[..12]);
    let destination = root.join(&relative);
    let package = root.join(&build.path);
    fs::create_dir_all(root.join("exports"))?;
    cancelled(cancel)?;
    if destination.exists() {
        let existing = package::validate_input(&destination)?;
        if existing.files["manifest.json"].sha256
            != hash(&maps::read(&package.join("manifest.json"))?)
        {
            return Err(error(
                "exists",
                "Export filename is occupied by different content",
            ));
        }
    } else {
        package::pack(&package, &destination)?;
    }
    build.export = Some(relative);
    maps::atomic_file(
        &root.join("build/course/current.json"),
        &serde_json::to_vec_pretty(&build).unwrap(),
    )?;
    open(root)
}
