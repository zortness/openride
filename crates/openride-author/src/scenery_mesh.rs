//! Bounded deterministic scenery geometry. Geographic decisions stay in scenery.json.
use crate::{
    error,
    scenery::{self, View},
    Result,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Default, Serialize)]
pub struct Mesh {
    pub vertices: Vec<[f64; 3]>,
    pub indices: Vec<usize>,
    pub normals: Vec<[f64; 3]>,
}
impl Mesh {
    fn triangle(&mut self, mut p: [[f64; 3]; 3]) -> Result<()> {
        if self.vertices.len() + 3 > 30000 {
            return Err(error(
                "scenery_budget",
                "Scenery chunk exceeds 30,000 vertices; reduce density or simplify features",
            ));
        }
        if p[0][1] == p[1][1]
            && p[1][1] == p[2][1]
            && (p[1][2] - p[0][2]) * (p[2][0] - p[0][0]) - (p[1][0] - p[0][0]) * (p[2][2] - p[0][2])
                < 0.
        {
            p.swap(1, 2);
        }
        let a = std::array::from_fn::<_, 3, _>(|i| p[1][i] - p[0][i]);
        let b = std::array::from_fn::<_, 3, _>(|i| p[2][i] - p[0][i]);
        let n = [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ];
        let length = n.iter().map(|x| x * x).sum::<f64>().sqrt();
        if length < 1e-9 {
            return Ok(());
        }
        let normal = n.map(|v| v / length);
        let base = self.vertices.len();
        self.vertices.extend(p);
        self.normals.extend([normal; 3]);
        self.indices.extend([base, base + 1, base + 2]);
        Ok(())
    }
}
#[derive(Clone)]
struct Feature {
    source: scenery::Feature,
    outer: Vec<[f64; 2]>,
    holes: Vec<Vec<[f64; 2]>>,
    points: Vec<[f64; 2]>,
    triangles: Vec<usize>,
    level: f64,
}
impl Feature {
    fn contains(&self, p: [f64; 2]) -> bool {
        scenery::contains(p, &self.outer, &self.holes)
    }
}
#[derive(Default)]
pub struct World {
    features: Vec<Feature>,
    index: BTreeMap<(i32, i32), Vec<usize>>,
    roads: BTreeMap<(i32, i32), Vec<[[f64; 2]; 2]>>,
    pub warnings: Vec<String>,
}
#[derive(Default, Serialize)]
pub struct Chunk {
    pub buildings: Mesh,
    pub roofs: Mesh,
    pub water: Mesh,
    pub vegetation: Vec<[f64; 6]>,
}
fn segment_distance(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let dx = b[0] - a[0];
    let dz = b[1] - a[1];
    let t = if dx * dx + dz * dz > 0. {
        ((p[0] - a[0]) * dx + (p[1] - a[1]) * dz) / (dx * dx + dz * dz)
    } else {
        0.
    };
    let t = t.clamp(0., 1.);
    (p[0] - a[0] - dx * t).hypot(p[1] - a[1] - dz * t)
}
impl World {
    pub fn prepare(
        view: &View,
        cells: &BTreeSet<(i32, i32)>,
        route: &[[f64; 3]],
        project: impl Fn([f64; 2]) -> Result<[f64; 2]>,
        mut height: impl FnMut(f64, f64) -> Result<f64>,
        origin_height: f64,
    ) -> Result<Self> {
        if !view.conflicts.is_empty() {
            return Err(error(
                "scenery_conflict",
                "Scenery sources changed; review saved overrides before building",
            ));
        }
        let mut world = Self {
            warnings: view.warnings.clone(),
            ..Default::default()
        };
        for pair in route.windows(2) {
            let a = [pair[0][0], pair[0][2]];
            let b = [pair[1][0], pair[1][2]];
            for x in ((a[0].min(b[0]) - 10.) / 256.).floor() as i32
                ..=((a[0].max(b[0]) + 10.) / 256.).floor() as i32
            {
                for z in ((a[1].min(b[1]) - 10.) / 256.).floor() as i32
                    ..=((a[1].max(b[1]) + 10.) / 256.).floor() as i32
                {
                    world.roads.entry((x, z)).or_default().push([a, b]);
                }
            }
        }
        for source in view.features.iter().filter(|f| !f.hidden) {
            scenery::validate(source)?;
            let outer = source
                .outer
                .iter()
                .map(|p| project(*p))
                .collect::<Result<Vec<_>>>()?;
            let holes = source
                .holes
                .iter()
                .map(|r| r.iter().map(|p| project(*p)).collect::<Result<Vec<_>>>())
                .collect::<Result<Vec<_>>>()?;
            let minx = outer.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
            let maxx = outer.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max);
            let minz = outer.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
            let maxz = outer.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max);
            let affected: Vec<_> = cells
                .iter()
                .copied()
                .filter(|&(x, z)| {
                    (x + 1) as f64 * 256. >= minx
                        && x as f64 * 256. <= maxx
                        && (z + 1) as f64 * 256. >= minz
                        && z as f64 * 256. <= maxz
                })
                .collect();
            if affected.is_empty() {
                continue;
            }
            let mut points = outer.clone();
            let mut starts = vec![];
            for h in &holes {
                starts.push(points.len());
                points.extend(h);
            }
            let flat: Vec<f64> = points.iter().flatten().copied().collect();
            let triangles = earcutr::earcut(&flat, &starts, 2)
                .map_err(|e| error("scenery_polygon", e.to_string()))?;
            if triangles.is_empty() {
                return Err(error(
                    "scenery_polygon",
                    "Polygon triangulation produced no surface",
                ));
            }
            let mut level = source.elevation_m.map(|v| v - origin_height);
            if level.is_none() {
                for p in &outer {
                    if let Ok(h) = height(p[0], p[1]) {
                        level = Some(level.map_or(h, |old: f64| old.min(h)));
                    }
                }
            }
            if level.is_none() {
                let (x, z) = affected[0];
                level = Some(height((x as f64 + 0.5) * 256., (z as f64 + 0.5) * 256.)?);
            }
            let feature = Feature {
                source: source.clone(),
                outer,
                holes,
                points,
                triangles,
                level: level.unwrap(),
            };
            if source.kind == "building"
                && affected.iter().any(|cell| {
                    world.roads.get(cell).is_some_and(|roads| {
                        roads.iter().any(|[a, b]| {
                            feature.contains(*a)
                                || feature.contains(*b)
                                || feature
                                    .outer
                                    .iter()
                                    .any(|p| segment_distance(*p, *a, *b) < 8.)
                                || feature
                                    .outer
                                    .iter()
                                    .zip(feature.outer.iter().cycle().skip(1))
                                    .take(feature.outer.len())
                                    .any(|(u, v)| segment_distance(*a, *u, *v) < 8.)
                        })
                    })
                })
            {
                if world.warnings.len() < 1000 {
                    world.warnings.push(format!(
                        "{}: building omitted for course clearance",
                        source.id
                    ));
                }
                continue;
            }
            let id = world.features.len();
            world.features.push(feature);
            for cell in affected {
                world.index.entry(cell).or_default().push(id);
            }
        }
        Ok(world)
    }
    pub fn water_level(&self, x: f64, z: f64) -> Option<f64> {
        self.index
            .get(&((x / 256.).floor() as i32, (z / 256.).floor() as i32))
            .into_iter()
            .flatten()
            .filter_map(|i| {
                let f = &self.features[*i];
                (f.source.kind == "water" && f.contains([x, z])).then_some(f.level)
            })
            .min_by(f64::total_cmp)
    }
    pub fn chunk(
        &self,
        cx: i32,
        cz: i32,
        mut height: impl FnMut(f64, f64) -> Result<f64>,
    ) -> Result<Chunk> {
        let mut out = Chunk::default();
        let Some(ids) = self.index.get(&(cx, cz)) else {
            return Ok(out);
        };
        let bounds = [
            cx as f64 * 256.,
            cz as f64 * 256.,
            (cx + 1) as f64 * 256.,
            (cz + 1) as f64 * 256.,
        ];
        for &id in ids {
            let f = &self.features[id];
            if ["building", "water"].contains(&f.source.kind.as_str()) {
                let roof = f.level
                    + if f.source.kind == "building" {
                        f.source.height_m.unwrap_or(6.)
                    } else {
                        0.05
                    };
                for t in f.triangles.chunks_exact(3) {
                    let p = t.map_array(|i| [f.points[i][0], roof, f.points[i][1]]);
                    let mesh = if f.source.kind == "water" {
                        &mut out.water
                    } else {
                        &mut out.roofs
                    };
                    clipped(mesh, p, bounds)?;
                }
                if f.source.kind == "building" {
                    for ring in std::iter::once(&f.outer).chain(&f.holes) {
                        for (a, b) in ring
                            .iter()
                            .zip(ring.iter().cycle().skip(1))
                            .take(ring.len())
                        {
                            let a0 = [a[0], f.level - 1., a[1]];
                            let b0 = [b[0], f.level - 1., b[1]];
                            let a1 = [a[0], roof, a[1]];
                            let b1 = [b[0], roof, b[1]];
                            clipped(&mut out.buildings, [a0, b0, a1], bounds)?;
                            clipped(&mut out.buildings, [a1, b0, b1], bounds)?;
                        }
                    }
                }
            }
        }
        // One global candidate per 8 m cell: neighboring features cannot duplicate plants.
        for z in cz * 32..(cz + 1) * 32 {
            for x in cx * 32..(cx + 1) * 32 {
                let seed = (x as u64).wrapping_mul(0x9e3779b185ebca87)
                    ^ (z as u64).wrapping_mul(0xc2b2ae3d27d4eb4f);
                let seed = seed ^ (seed >> 29);
                let p = [
                    x as f64 * 8. + 1. + (seed & 1023) as f64 / 1023. * 6.,
                    z as f64 * 8. + 1. + ((seed >> 10) & 1023) as f64 / 1023. * 6.,
                ];
                if self
                    .roads
                    .get(&(cx, cz))
                    .is_some_and(|r| r.iter().any(|[a, b]| segment_distance(p, *a, *b) < 8.))
                {
                    continue;
                }
                if ids.iter().any(|i| {
                    let f = &self.features[*i];
                    ["water", "building", "exclusion"].contains(&f.source.kind.as_str())
                        && f.contains(p)
                }) {
                    continue;
                }
                if let Some(f) = ids.iter().map(|i| &self.features[*i]).find(|f| {
                    f.source.kind == "vegetation"
                        && f.contains(p)
                        && ((seed >> 20) & 65535) as f64 / 65535. < f.source.density
                }) {
                    out.vegetation.push([
                        p[0],
                        height(p[0], p[1])?,
                        p[1],
                        0.65 + ((seed >> 36) & 255) as f64 / 255. * 0.55,
                        if f.source.trees { 1. } else { 0. },
                        (seed >> 44) as f64 / ((1u64 << 20) - 1) as f64 * std::f64::consts::TAU,
                    ]);
                }
            }
        }
        Ok(out)
    }
}
trait TriangleMap {
    fn map_array(&self, f: impl Fn(usize) -> [f64; 3]) -> [[f64; 3]; 3];
}
impl TriangleMap for [usize] {
    fn map_array(&self, f: impl Fn(usize) -> [f64; 3]) -> [[f64; 3]; 3] {
        [f(self[0]), f(self[1]), f(self[2])]
    }
}
fn clipped(mesh: &mut Mesh, triangle: [[f64; 3]; 3], bounds: [f64; 4]) -> Result<()> {
    let mut polygon = triangle.to_vec();
    for (axis, limit, sign) in [
        (0, bounds[0], 1.),
        (0, bounds[2], -1.),
        (2, bounds[1], 1.),
        (2, bounds[3], -1.),
    ] {
        let mut next = vec![];
        for (a, b) in polygon
            .iter()
            .zip(polygon.iter().cycle().skip(1))
            .take(polygon.len())
        {
            let da = (a[axis] - limit) * sign;
            let db = (b[axis] - limit) * sign;
            if da >= 0. {
                next.push(*a);
            }
            if (da >= 0.) != (db >= 0.) {
                let t = da / (da - db);
                let mut p = std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t);
                p[axis] = limit;
                next.push(p);
            }
        }
        polygon = next;
    }
    for i in 1..polygon.len().saturating_sub(1) {
        split(mesh, [polygon[0], polygon[i], polygon[i + 1]], 0)?;
    }
    Ok(())
}
fn split(mesh: &mut Mesh, p: [[f64; 3]; 3], depth: usize) -> Result<()> {
    let mut longest = (0, 0.);
    for i in 0..3 {
        let j = (i + 1) % 3;
        let n = (p[i][0] - p[j][0]).hypot(p[i][2] - p[j][2]);
        if n > longest.1 {
            longest = (i, n);
        }
    }
    if longest.1 <= 32. {
        return mesh.triangle(p);
    }
    if depth > 20 {
        return Err(error(
            "scenery_budget",
            "Scenery subdivision exceeded budget",
        ));
    }
    let a = longest.0;
    let b = (a + 1) % 3;
    let c = (a + 2) % 3;
    let m = std::array::from_fn(|i| (p[a][i] + p[b][i]) * 0.5);
    split(mesh, [p[a], m, p[c]], depth + 1)?;
    split(mesh, [m, p[b], p[c]], depth + 1)
}
