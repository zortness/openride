//! Project-owned scenery choices, independent of recorded course heights.
use crate::{cancelled, checked, error, lock, maps, open, streets::Graph, Result, Snapshot};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Feature {
    pub id: String,
    pub kind: String,
    pub outer: Vec<[f64; 2]>,
    #[serde(default)]
    pub holes: Vec<Vec<[f64; 2]>>,
    #[serde(default)]
    pub height_m: Option<f64>,
    #[serde(default)]
    pub elevation_m: Option<f64>,
    #[serde(default = "density")]
    pub density: f64,
    #[serde(default)]
    pub trees: bool,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub provenance: String,
}
fn density() -> f64 {
    0.35
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Catalog {
    pub features: BTreeMap<String, Feature>,
    pub warnings: Vec<String>,
    pub sources: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Override {
    pub original: Option<String>,
    pub feature: Feature,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Book {
    pub schema_version: u32,
    pub overrides: BTreeMap<String, Override>,
    pub history: Vec<(String, Option<Override>)>,
}
impl Default for Book {
    fn default() -> Self {
        Self {
            schema_version: 1,
            overrides: BTreeMap::new(),
            history: vec![],
        }
    }
}
#[derive(Debug, Serialize)]
pub struct View {
    pub features: Vec<Feature>,
    pub warnings: Vec<String>,
    pub conflicts: Vec<String>,
    pub revision: String,
}
fn bad(s: impl Into<String>) -> crate::Error {
    error("invalid_scenery", s)
}
fn hash(v: &impl Serialize) -> String {
    format!("{:x}", Sha256::digest(serde_json::to_vec(v).unwrap()))
}
pub fn inside(p: [f64; 2], ring: &[[f64; 2]]) -> bool {
    let mut hit = false;
    for (a, b) in ring
        .iter()
        .zip(ring.iter().cycle().skip(1))
        .take(ring.len())
    {
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            hit = !hit;
        }
    }
    hit
}
pub fn contains(p: [f64; 2], outer: &[[f64; 2]], holes: &[Vec<[f64; 2]>]) -> bool {
    inside(p, outer) && !holes.iter().any(|r| inside(p, r))
}
pub fn validate(f: &Feature) -> Result<()> {
    if f.id.is_empty()
        || f.id.len() > 120
        || !f
            .id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/-_".contains(&b))
        || !["building", "vegetation", "water", "exclusion"].contains(&f.kind.as_str())
        || !f.density.is_finite()
        || !(0.0..=1.).contains(&f.density)
        || f.height_m
            .is_some_and(|v| !v.is_finite() || !(1.0..=120.).contains(&v))
        || f.elevation_m
            .is_some_and(|v| !v.is_finite() || !(-500.0..=9000.).contains(&v))
        || f.provenance.len() > 2048
    {
        return Err(bad("Invalid scenery identity, kind or dimensions"));
    }
    if f.holes.len() > 64 || f.outer.len() + f.holes.iter().map(Vec::len).sum::<usize>() > 2048 {
        return Err(bad("Scenery polygon exceeds vertex budget"));
    }
    for ring in std::iter::once(&f.outer).chain(&f.holes) {
        if ring.len() < 3
            || ring.len() > 512
            || ring.iter().any(|p| {
                !p[0].is_finite() || !p[1].is_finite() || p[0].abs() > 80. || p[1].abs() > 180.
            })
        {
            return Err(bad(
                "Use a polygon with 3-512 finite geographic vertices per ring",
            ));
        }
        let edges: Vec<_> = ring
            .iter()
            .zip(ring.iter().cycle().skip(1))
            .take(ring.len())
            .collect();
        let area = edges
            .iter()
            .map(|(a, b)| {
                (a[0] - ring[0][0]) * (b[1] - ring[0][1])
                    - (b[0] - ring[0][0]) * (a[1] - ring[0][1])
            })
            .sum::<f64>();
        if area.abs() < 1e-12 || edges.iter().any(|(a, b)| a == b) {
            return Err(bad("Degenerate scenery polygon"));
        }
        for (i, (a, b)) in edges.iter().enumerate() {
            for (j, (c, d)) in edges.iter().enumerate().skip(i + 1) {
                if j == i + 1 || i == 0 && j == edges.len() - 1 {
                    continue;
                }
                if crosses(**a, **b, **c, **d) {
                    return Err(bad("Self-intersecting scenery polygon"));
                }
            }
        }
    }
    for (i, hole) in f.holes.iter().enumerate() {
        if hole.iter().any(|p| !inside(*p, &f.outer))
            || rings_cross(hole, &f.outer)
            || f.holes[..i]
                .iter()
                .any(|h| rings_cross(hole, h) || inside(hole[0], h) || inside(h[0], hole))
        {
            return Err(bad(
                "Holes must be disjoint and contained by the outer ring",
            ));
        }
    }
    Ok(())
}
fn crosses(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let side = |p: [f64; 2], q: [f64; 2], r: [f64; 2]| {
        (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])
    };
    let on = |p: [f64; 2], q: [f64; 2], r: [f64; 2]| {
        side(p, q, r).abs() < 1e-15
            && r[0] >= p[0].min(q[0]) - 1e-12
            && r[0] <= p[0].max(q[0]) + 1e-12
            && r[1] >= p[1].min(q[1]) - 1e-12
            && r[1] <= p[1].max(q[1]) + 1e-12
    };
    side(a, b, c) * side(a, b, d) < 0. && side(c, d, a) * side(c, d, b) < 0.
        || on(a, b, c)
        || on(a, b, d)
        || on(c, d, a)
        || on(c, d, b)
}
fn rings_cross(a: &[[f64; 2]], b: &[[f64; 2]]) -> bool {
    a.iter()
        .zip(a.iter().cycle().skip(1))
        .take(a.len())
        .any(|(x, y)| {
            b.iter()
                .zip(b.iter().cycle().skip(1))
                .take(b.len())
                .any(|(u, v)| crosses(*x, *y, *u, *v))
        })
}
fn read<T: serde::de::DeserializeOwned + Default>(path: &Path) -> Result<T> {
    if !path.exists() {
        return Ok(T::default());
    }
    let raw = maps::read(path)?;
    if raw.len() > 16 * 1024 * 1024 {
        return Err(bad("Scenery exceeds 16 MiB"));
    }
    serde_json::from_slice(&raw).map_err(|e| bad(e.to_string()))
}
pub fn book(root: &Path) -> Result<Book> {
    let b: Book = read(&root.join("scenery.json"))?;
    if b.schema_version != 1 || b.overrides.len() > 10000 || b.history.len() > 10000 {
        return Err(bad("Unsupported or oversized scenery book"));
    }
    for (id, v) in &b.overrides {
        if id != &v.feature.id {
            return Err(bad("Scenery override identity mismatch"));
        }
        validate(&v.feature)?;
    }
    for (id, v) in &b.history {
        if let Some(v) = v {
            if id != &v.feature.id {
                return Err(bad("Scenery history identity mismatch"));
            }
            validate(&v.feature)?;
        }
    }
    Ok(b)
}
pub fn view(root: &Path, plan: Option<&maps::Plan>) -> Result<View> {
    let catalog: Catalog = if let Some(p) = plan {
        read(&root.join("maps").join(&p.key).join("scenery-catalog.json"))?
    } else {
        Catalog::default()
    };
    let b = book(root)?;
    let revision = hash(&(&b, &catalog));
    let mut features = catalog.features;
    let mut conflicts = vec![];
    for (id, v) in b.overrides {
        if v.original
            .as_ref()
            .is_some_and(|old| features.get(&id).is_none_or(|f| hash(f) != *old))
        {
            conflicts.push(id.clone());
        }
        features.insert(id, v.feature);
    }
    Ok(View {
        features: features.into_values().collect(),
        warnings: catalog.warnings,
        conflicts,
        revision,
    })
}
pub fn edit(
    root: &Path,
    revision: &str,
    mode: &str,
    id: &str,
    feature: Option<Feature>,
    cancel: Option<&Path>,
) -> Result<Snapshot> {
    let _guard = lock(root)?;
    let current = checked(root, revision)?;
    cancelled(cancel)?;
    let catalog: Catalog = if let Some(p) = &current.maps {
        read(&root.join("maps").join(&p.key).join("scenery-catalog.json"))?
    } else {
        Catalog::default()
    };
    let mut b = book(root)?;
    if mode == "undo" {
        let index = b
            .history
            .iter()
            .rposition(|v| v.0 == id)
            .ok_or_else(|| bad("No scenery edit to undo"))?;
        let (_, old) = b.history.remove(index);
        if let Some(v) = old {
            b.overrides.insert(id.into(), v);
        } else {
            b.overrides.remove(id);
        }
    } else {
        if b.history.len() >= 10000 {
            return Err(bad("Scenery history limit reached"));
        }
        let previous = b.overrides.get(id).cloned();
        if mode == "reset" {
            b.overrides.remove(id);
        } else {
            let mut f = feature
                .or_else(|| previous.as_ref().map(|v| v.feature.clone()))
                .or_else(|| catalog.features.get(id).cloned())
                .ok_or_else(|| bad("Choose a scenery feature"))?;
            if mode == "hide" {
                f.hidden = true;
            } else if mode != "save" {
                return Err(bad("Use save, hide, undo or reset"));
            }
            if f.id != id || !catalog.features.contains_key(id) && !id.starts_with("user-") {
                return Err(bad("New features need a user- identity"));
            }
            validate(&f)?;
            // Saving after a source refresh is an explicit rebase of this one feature.
            let original = catalog.features.get(id).map(hash);
            b.overrides.insert(
                id.into(),
                Override {
                    original,
                    feature: f,
                },
            );
        }
        b.history.push((id.into(), previous));
    }
    let raw = serde_json::to_vec_pretty(&b).unwrap();
    if raw.len() > 16 * 1024 * 1024 {
        return Err(bad("Scenery book exceeds 16 MiB"));
    }
    cancelled(cancel)?;
    maps::atomic_file(&root.join("scenery.json"), &raw)?;
    open(root)
}
fn rings(mut parts: Vec<Vec<i64>>) -> Result<Vec<Vec<i64>>> {
    parts.sort_by_key(|p| p.iter().min().copied().unwrap_or(0));
    let mut result = vec![];
    while let Some(mut ring) = parts.pop() {
        while ring.first() != ring.last() {
            let last = *ring.last().ok_or_else(|| bad("Empty polygon way"))?;
            let i = parts
                .iter()
                .position(|p| p.first() == Some(&last) || p.last() == Some(&last))
                .ok_or_else(|| bad("Unclosed multipolygon boundary"))?;
            let mut next = parts.remove(i);
            if next.last() == Some(&last) {
                next.reverse();
            }
            ring.extend(next.into_iter().skip(1));
            if ring.len() > 2049 {
                return Err(bad("Polygon exceeds vertex budget"));
            }
        }
        ring.pop();
        result.push(ring);
    }
    result.sort_by_key(|p| p.iter().min().copied().unwrap_or(0));
    Ok(result)
}
pub fn catalog(graph: &Graph) -> Catalog {
    let mut out = Catalog {
        sources: graph.sources.clone(),
        ..Default::default()
    };
    let mut members = BTreeSet::new();
    let kind = |tags: &BTreeMap<String, String>| -> Option<&'static str> {
        if tags.get("building").is_some_and(|v| v != "no") {
            Some("building")
        } else if tags.get("natural").is_some_and(|v| v == "water")
            || tags.get("waterway").is_some_and(|v| v == "riverbank")
        {
            Some("water")
        } else if tags
            .get("natural")
            .is_some_and(|v| ["wood", "scrub", "heath"].contains(&v.as_str()))
            || tags.get("landuse").is_some_and(|v| v == "forest")
        {
            Some("vegetation")
        } else {
            None
        }
    };
    for relation in graph
        .features
        .values()
        .filter(|f| f.kind == "relation" && kind(&f.tags).is_some())
    {
        for m in &relation.members {
            if m.kind == "way" {
                members.insert(m.id);
            }
        }
    }
    for f in graph
        .features
        .values()
        .filter(|f| f.kind == "relation" || f.kind == "way" && !members.contains(&f.id))
    {
        if f.tags.get("natural").is_some_and(|v| v == "coastline") {
            if out.warnings.len() < 1000 {
                out.warnings.push(format!("way/{}: open coastline requires an explicit water boundary; sea side was not guessed",f.id));
            }
            continue;
        }
        let Some(k) = kind(&f.tags) else {
            continue;
        };
        let generated = (|| -> Result<Vec<Feature>> {
            let (outer, holes) = if f.kind == "way" {
                (rings(vec![f.nodes.clone()])?, vec![])
            } else {
                if f.members
                    .iter()
                    .any(|m| m.kind != "way" || !["outer", "inner", ""].contains(&m.role.as_str()))
                {
                    return Err(bad("Unsupported multipolygon member roles"));
                }
                let ways = |inner: bool| {
                    f.members
                        .iter()
                        .filter(|m| (m.role == "inner") == inner)
                        .map(|m| graph.features[&format!("way/{}", m.id)].nodes.clone())
                        .collect()
                };
                (rings(ways(false))?, rings(ways(true))?)
            };
            let positions = |nodes: Vec<i64>| -> Result<Vec<[f64; 2]>> {
                nodes
                    .iter()
                    .map(|id| {
                        graph
                            .features
                            .get(&format!("node/{id}"))
                            .and_then(|n| n.position)
                            .ok_or_else(|| bad("Missing scenery node"))
                    })
                    .collect()
            };
            let holes: Vec<_> = holes.into_iter().map(positions).collect::<Result<_>>()?;
            let mut result = vec![];
            let mut assigned = 0;
            for (part, nodes) in outer.into_iter().enumerate() {
                let outer = positions(nodes)?;
                let local_holes: Vec<_> = holes
                    .iter()
                    .filter(|h| inside(h[0], &outer))
                    .cloned()
                    .collect();
                assigned += local_holes.len();
                let stated = f
                    .tags
                    .get("height")
                    .and_then(|v| v.trim_end_matches(" m").parse::<f64>().ok())
                    .or_else(|| {
                        f.tags
                            .get("building:levels")
                            .and_then(|v| v.parse::<f64>().ok())
                            .map(|v| v * 3.)
                    });
                let feature = Feature {
                    id: format!("{}/{}/{part}", f.kind, f.id),
                    kind: k.into(),
                    outer,
                    holes: local_holes,
                    height_m: if k == "building" {
                        Some(stated.unwrap_or(6.))
                    } else {
                        None
                    },
                    elevation_m: None,
                    density: density(),
                    trees: f.tags.get("natural").is_some_and(|v| v == "wood")
                        || f.tags.get("landuse").is_some_and(|v| v == "forest"),
                    hidden: false,
                    provenance: format!(
                        "OSM {}/{} version {}; {}",
                        f.kind,
                        f.id,
                        f.version,
                        if k == "building" && stated.is_none() {
                            "default building height 6 m"
                        } else {
                            "source geometry; inferred surface elevation"
                        }
                    ),
                };
                validate(&feature)?;
                result.push(feature);
            }
            if assigned != holes.len() {
                return Err(bad("Unassigned or multiply assigned polygon hole"));
            }
            Ok(result)
        })();
        match generated {
            Ok(features) => {
                for f in features {
                    out.features.insert(f.id.clone(), f);
                }
            }
            Err(e) => {
                if out.warnings.len() < 1000 {
                    out.warnings
                        .push(format!("{}/{}: {}", f.kind, f.id, e.message));
                }
            }
        }
    }
    out
}
struct Corridor {
    origin: [f64; 3],
    points: BTreeMap<(i32, i32), Vec<[f64; 2]>>,
}
impl Corridor {
    fn new(origin: [f64; 3]) -> Self {
        Self {
            origin,
            points: BTreeMap::new(),
        }
    }
    fn xy(&self, p: [f64; 2]) -> [f64; 2] {
        [
            (p[1] - self.origin[1]) * 111195. * self.origin[0].to_radians().cos(),
            (p[0] - self.origin[0]) * 111195.,
        ]
    }
    fn add(&mut self, p: [f64; 2]) {
        self.points
            .entry(((p[0] / 256.).floor() as i32, (p[1] / 256.).floor() as i32))
            .or_default()
            .push(p);
    }
    fn segment(&mut self, a: [f64; 2], b: [f64; 2]) {
        let n = ((a[0] - b[0]).hypot(a[1] - b[1]) / 50.).ceil().max(1.) as usize;
        for i in 0..=n {
            let t = i as f64 / n as f64;
            self.add([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]);
        }
    }
    fn from_snapshot(snapshot: &Snapshot, plan: &maps::Plan) -> Self {
        let mut corridor = Self::new(plan.origin);
        if let Some(course) = &snapshot.course {
            for section in &course.sections {
                for p in &section.geometry {
                    corridor.add(corridor.xy(*p));
                }
                for pair in section.geometry.windows(2) {
                    corridor.segment(corridor.xy(pair[0]), corridor.xy(pair[1]));
                }
            }
        } else if let Some(preview) = &snapshot.repair_preview {
            let mut previous = None;
            for sample in &preview.samples {
                if let (Some(lat), Some(lon)) = (sample.latitude, sample.longitude) {
                    let p = corridor.xy([lat, lon]);
                    if let Some(a) = previous.filter(|_| !sample.break_before) {
                        corridor.segment(a, p);
                    } else {
                        corridor.add(p);
                    }
                    previous = Some(p);
                } else {
                    previous = None;
                }
            }
        }
        corridor
    }
    fn includes(&self, feature: &Feature) -> bool {
        let mut min = [f64::INFINITY; 2];
        let mut max = [f64::NEG_INFINITY; 2];
        for p in &feature.outer {
            let p = self.xy(*p);
            for axis in 0..2 {
                min[axis] = min[axis].min(p[axis]);
                max[axis] = max[axis].max(p[axis]);
            }
        }
        if !min[0].is_finite() {
            return false;
        }
        const R: f64 = 150.;
        for x in ((min[0] - R) / 256.).floor() as i32..=((max[0] + R) / 256.).floor() as i32 {
            for z in ((min[1] - R) / 256.).floor() as i32..=((max[1] + R) / 256.).floor() as i32 {
                if self.points.get(&(x, z)).is_some_and(|points| {
                    points.iter().any(|p| {
                        p[0] >= min[0] - R
                            && p[0] <= max[0] + R
                            && p[1] >= min[1] - R
                            && p[1] <= max[1] + R
                    })
                }) {
                    return true;
                }
            }
        }
        false
    }
}
pub fn refresh_catalog(
    root: &Path,
    plan: &maps::Plan,
    snapshot: &Snapshot,
    cancel: Option<&Path>,
) -> Result<()> {
    let mut graph = Graph::default();
    for tile in &plan.tiles {
        for layer in ["buildings", "vegetation", "water"] {
            cancelled(cancel)?;
            let dir = root.join("maps").join(&plan.key).join(&tile.id).join(layer);
            if maps::verified(&dir, layer) {
                let raw = maps::read(&dir.join("scenery.osm"))?;
                let mut part = crate::streets::parse(&raw)?;
                part.sources.insert(
                    format!("{}/{layer}", tile.id),
                    format!("{:x}", Sha256::digest(raw)),
                );
                graph.merge(part)?;
            }
        }
    }
    let mut catalog = catalog(&graph);
    catalog
        .warnings
        .retain(|s| !s.contains("open coastline requires"));
    let covered: Vec<_> = plan
        .tiles
        .iter()
        .filter(|t| {
            maps::verified(
                &root.join("maps").join(&plan.key).join(&t.id).join("water"),
                "water",
            )
        })
        .cloned()
        .collect();
    let (coast, warnings) = crate::coastline::water(&graph, &covered);
    for f in coast {
        catalog.features.insert(f.id.clone(), f);
    }
    let corridor = Corridor::from_snapshot(snapshot, plan);
    if corridor.points.is_empty() {
        return Err(bad("Scenery needs a located ride corridor"));
    }
    let pinned = book(root)?.overrides.into_keys().collect::<BTreeSet<_>>();
    catalog
        .features
        .retain(|id, feature| pinned.contains(id) || corridor.includes(feature));
    catalog.warnings.extend(warnings);
    catalog.warnings.truncate(1000);
    let raw = serde_json::to_vec(&catalog).unwrap();
    if catalog.features.len() > 30000 || raw.len() > 16 * 1024 * 1024 {
        return Err(bad("Scenery catalog exceeds 30,000 features / 16 MiB"));
    }
    fs::create_dir_all(root.join("maps").join(&plan.key))?;
    maps::atomic_file(
        &root
            .join("maps")
            .join(&plan.key)
            .join("scenery-catalog.json"),
        &raw,
    )
}

#[cfg(test)]
mod corridor_tests {
    use super::{Corridor, Feature};

    fn polygon(id: &str, west: f64, east: f64) -> Feature {
        Feature {
            id: id.into(),
            kind: "building".into(),
            outer: vec![
                [-0.0001, west],
                [-0.0001, east],
                [0.0001, east],
                [0.0001, west],
            ],
            holes: vec![],
            height_m: None,
            elevation_m: None,
            density: 0.35,
            trees: false,
            hidden: false,
            provenance: String::new(),
        }
    }

    #[test]
    fn scenery_catalog_uses_the_ride_corridor_including_long_segments() {
        let mut corridor = Corridor::new([0., 0., 0.]);
        corridor.segment([0., 0.], [1000., 0.]);
        assert!(corridor.includes(&polygon("near", 0.004, 0.0041)));
        let mut far = polygon("far", 0.004, 0.0041);
        for p in &mut far.outer {
            p[0] += 0.01;
        }
        assert!(!corridor.includes(&far));
        assert!(corridor.includes(&polygon("large", -0.01, 0.01)));
    }
}
