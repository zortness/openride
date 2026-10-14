//! Conservative occurrence-ordered matching. Unknown transitions retain the trace.
use crate::{cancelled, checked, error, lock, maps, open, streets::Graph, Result, Snapshot};
use ride_core::Sample;
use serde::{Deserialize, Serialize};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, BinaryHeap},
    fs,
    path::Path,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Section {
    pub source_sample: usize,
    pub status: String,
    pub way: Option<i64>,
    pub recorded_elevation_m: Option<f64>,
    pub geometry: Vec<[f64; 2]>,
    pub length_m: f64,
    pub break_before: bool,
    #[serde(default)]
    pub alternative_ways: Vec<i64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Course {
    pub schema_version: u32,
    pub input_revision: String,
    #[serde(default)]
    pub corrections_hash: String,
    pub map_key: String,
    pub map_sources: BTreeMap<String, String>,
    pub sections: Vec<Section>,
    pub source_length_m: f64,
    pub length_m: f64,
    pub unresolved: usize,
    pub elevation_note: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roads: Option<crate::road_course::Metadata>,
}
#[derive(Clone)]
pub(crate) struct Edge {
    pub(crate) way: i64,
    pub(crate) a: i64,
    pub(crate) b: i64,
    start: [f64; 2],
    end: [f64; 2],
    forward: bool,
    backward: bool,
}
#[derive(Clone)]
pub(crate) struct Candidate {
    pub(crate) edge: usize,
    t: f64,
    pub(crate) position: [f64; 2],
    pub(crate) offset: f64,
}
pub(crate) struct Network {
    pub(crate) edges: Vec<Edge>,
    cells: BTreeMap<(i32, i32), Vec<usize>>,
    adjacency: BTreeMap<i64, Vec<(i64, f64, i64)>>,
    nodes: BTreeMap<i64, [f64; 2]>,
    origin: [f64; 2],
}
fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    ride_core::haversine(a[0], a[1], b[0], b[1])
}
fn interpolate(a: [f64; 2], b: [f64; 2], t: f64) -> [f64; 2] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}
impl Network {
    fn xy(&self, p: [f64; 2]) -> [f64; 2] {
        [
            (p[1] - self.origin[1]) * 111195. * self.origin[0].to_radians().cos(),
            (p[0] - self.origin[0]) * 111195.,
        ]
    }
    pub(crate) fn new(graph: &Graph, origin: [f64; 2]) -> Self {
        let mut n = Self {
            edges: vec![],
            cells: BTreeMap::new(),
            adjacency: BTreeMap::new(),
            nodes: BTreeMap::new(),
            origin,
        };
        for f in graph
            .features
            .values()
            .filter(|f| f.kind == "way" && f.tags.contains_key("highway"))
        {
            let tag = |key: &str| f.tags.get(key).map(String::as_str).unwrap_or("");
            if [
                "motorway",
                "motorway_link",
                "construction",
                "proposed",
                "steps",
            ]
            .contains(&tag("highway"))
                || ["no", "private"].contains(&tag("bicycle"))
                || (["no", "private"].contains(&tag("access"))
                    && !["yes", "designated", "permissive"].contains(&tag("bicycle")))
            {
                continue;
            }
            let one = if !tag("oneway:bicycle").is_empty() {
                tag("oneway:bicycle")
            } else {
                tag("oneway")
            };
            for w in f.nodes.windows(2) {
                let (Some(a), Some(b)) = (
                    graph
                        .features
                        .get(&format!("node/{}", w[0]))
                        .and_then(|v| v.position),
                    graph
                        .features
                        .get(&format!("node/{}", w[1]))
                        .and_then(|v| v.position),
                ) else {
                    continue;
                };
                let length = distance(a, b);
                if length > 5000. || length < 0.001 {
                    continue;
                }
                let edge = Edge {
                    way: f.id,
                    a: w[0],
                    b: w[1],
                    start: a,
                    end: b,
                    forward: one != "-1",
                    backward: !["yes", "1", "true"].contains(&one),
                };
                n.nodes.insert(edge.a, a);
                n.nodes.insert(edge.b, b);
                if edge.forward {
                    n.adjacency
                        .entry(edge.a)
                        .or_default()
                        .push((edge.b, length, edge.way));
                }
                if edge.backward {
                    n.adjacency
                        .entry(edge.b)
                        .or_default()
                        .push((edge.a, length, edge.way));
                }
                let steps = (length / 25.).ceil() as usize;
                let mut covered = BTreeSet::new();
                for step in 0..=steps {
                    let p = n.xy(interpolate(a, b, step as f64 / steps as f64));
                    covered.insert(((p[0] / 50.).floor() as i32, (p[1] / 50.).floor() as i32));
                }
                for cell in covered {
                    n.cells.entry(cell).or_default().push(n.edges.len());
                }
                n.edges.push(edge);
            }
        }
        n
    }
    pub(crate) fn candidates(&self, p: [f64; 2]) -> Vec<Candidate> {
        let pxy = self.xy(p);
        let mut indices: BTreeSet<usize> = BTreeSet::new();
        let cell = ((pxy[0] / 50.).floor() as i32, (pxy[1] / 50.).floor() as i32);
        for x in cell.0 - 1..=cell.0 + 1 {
            for y in cell.1 - 1..=cell.1 + 1 {
                if let Some(v) = self.cells.get(&(x, y)) {
                    indices.extend(v);
                }
            }
        }
        let mut result = vec![];
        for &i in indices.iter().take(5000) {
            let e = &self.edges[i];
            let a = self.xy(e.start);
            let b = self.xy(e.end);
            let d = [b[0] - a[0], b[1] - a[1]];
            let t = (((pxy[0] - a[0]) * d[0] + (pxy[1] - a[1]) * d[1])
                / (d[0] * d[0] + d[1] * d[1]))
                .clamp(0., 1.);
            let position = interpolate(e.start, e.end, t);
            let offset = distance(p, position);
            if offset <= 30. {
                result.push(Candidate {
                    edge: i,
                    t,
                    position,
                    offset,
                });
            }
        }
        result.sort_by(|a, b| a.offset.total_cmp(&b.offset));
        result.truncate(8);
        result
    }
    fn path(&self, start: i64, end: i64, budget: f64) -> Option<(Vec<i64>, Vec<i64>)> {
        let mut distances = BTreeMap::from([(start, 0.)]);
        let mut parents = BTreeMap::new();
        let mut todo = BinaryHeap::from([Reverse((0f64.to_bits(), start))]);
        let mut visited = BTreeSet::new();
        while !todo.is_empty() && visited.len() < 2000 {
            let Reverse((bits, node)) = todo.pop().unwrap();
            let cost = f64::from_bits(bits);
            if !visited.insert(node) {
                continue;
            }
            if node == end {
                let mut nodes = vec![end];
                let mut ways = vec![];
                let mut n = end;
                while n != start {
                    let (parent, way) = parents[&n];
                    ways.push(way);
                    n = parent;
                    nodes.push(n);
                }
                nodes.reverse();
                ways.reverse();
                return Some((nodes, ways));
            }
            if let Some(next) = self.adjacency.get(&node) {
                for &(v, d, way) in next {
                    let c = cost + d;
                    if c <= budget && c < *distances.get(&v).unwrap_or(&f64::INFINITY) {
                        distances.insert(v, c);
                        parents.insert(v, (node, way));
                        todo.push(Reverse((c.to_bits(), v)));
                    }
                }
            }
        }
        None
    }
    fn transition(&self, a: &Candidate, b: &Candidate, budget: f64) -> Option<Vec<[f64; 2]>> {
        self.transition_roads(a, b, budget).map(|v| v.0)
    }
    pub(crate) fn transition_roads(
        &self,
        a: &Candidate,
        b: &Candidate,
        budget: f64,
    ) -> Option<(Vec<[f64; 2]>, Vec<i64>)> {
        let ea = &self.edges[a.edge];
        let eb = &self.edges[b.edge];
        if a.edge == b.edge && ((b.t >= a.t && ea.forward) || (b.t <= a.t && ea.backward)) {
            return Some((vec![a.position, b.position], vec![ea.way]));
        }
        let mut best: Option<(f64, Vec<[f64; 2]>, Vec<i64>)> = None;
        for (start, allowed) in [(ea.b, ea.forward), (ea.a, ea.backward)] {
            for (end, ok) in [(eb.a, eb.forward), (eb.b, eb.backward)] {
                if !allowed || !ok {
                    continue;
                }
                if let Some((nodes, path_ways)) = self.path(start, end, budget) {
                    let mut geometry = vec![a.position];
                    geometry.extend(nodes.iter().map(|id| self.nodes[id]));
                    geometry.push(b.position);
                    let length = geometry
                        .windows(2)
                        .map(|w| distance(w[0], w[1]))
                        .sum::<f64>();
                    if length <= budget && best.as_ref().is_none_or(|v| length < v.0) {
                        let mut ways = vec![ea.way];
                        ways.extend(path_ways);
                        ways.push(eb.way);
                        best = Some((length, geometry, ways));
                    }
                }
            }
        }
        best.map(|v| (v.1, v.2))
    }
}
pub fn match_trace(
    samples: &[Sample],
    source_samples: &[usize],
    graph: &Graph,
    cancel: Option<&Path>,
) -> Result<Vec<Section>> {
    match_with_edits(samples, source_samples, graph, &BTreeMap::new(), cancel)
}
pub fn match_with_edits(
    samples: &[Sample],
    source_samples: &[usize],
    graph: &Graph,
    edits: &BTreeMap<usize, crate::corrections::Edit>,
    cancel: Option<&Path>,
) -> Result<Vec<Section>> {
    if samples.len() != source_samples.len() || samples.len() > 100_000 {
        return Err(error("source_limit", "Invalid source occurrence mapping"));
    }
    let origin = samples
        .iter()
        .find_map(|s| Some([s.latitude?, s.longitude?]))
        .ok_or_else(|| error("source_required", "No located samples"))?;
    let network = Network::new(graph, origin);
    let mut sections: Vec<Section> = vec![];
    let mut previous: Option<Candidate> = None;
    let mut raw_previous = None;
    for (index, sample) in samples.iter().enumerate() {
        cancelled(cancel)?;
        let Some(position) = sample.latitude.zip(sample.longitude).map(|(a, b)| [a, b]) else {
            previous = None;
            raw_previous = None;
            sections.push(Section {
                source_sample: source_samples[index],
                status: "missing_position".into(),
                way: None,
                recorded_elevation_m: sample.elevation_m,
                geometry: vec![],
                length_m: 0.,
                break_before: true,
                alternative_ways: vec![],
            });
            continue;
        };
        let edit = edits.get(&source_samples[index]);
        if sample.break_before && !edit.is_some_and(|e| e.mode == "join") {
            previous = None;
            raw_previous = None;
        }
        let raw_length = raw_previous.map(|p| distance(p, position)).unwrap_or(0.);
        let display_previous =
            raw_previous.and_then(|_| sections.last().and_then(|s| s.geometry.last()).copied());
        let candidates = network.candidates(position);
        let alternative_ways: Vec<_> = candidates
            .iter()
            .map(|c| network.edges[c.edge].way)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        if let Some(e) = edit.filter(|e| ["trace", "point", "join"].contains(&e.mode.as_str())) {
            let target = if e.mode == "point" || e.mode == "join" {
                e.position
                    .ok_or_else(|| error("invalid_edit", "Edited point missing"))?
            } else {
                position
            };
            let mut geometry = Vec::new();
            if let Some(p) = display_previous {
                geometry.push(p);
            }
            geometry.push(target);
            let length_m = geometry.windows(2).map(|w| distance(w[0], w[1])).sum();
            // An explicit edit may seed matching only at an actual road position.
            let near = network.candidates(target);
            previous = near
                .first()
                .filter(|c| {
                    c.offset < 0.05
                        && near.iter().skip(1).all(|v| {
                            v.offset > 3. || network.edges[v.edge].way == network.edges[c.edge].way
                        })
                })
                .cloned();
            if let Some(p) = &mut previous {
                p.position = target;
            }
            sections.push(Section {
                source_sample: source_samples[index],
                status: "manual".into(),
                way: None,
                recorded_elevation_m: sample.elevation_m,
                geometry,
                length_m,
                break_before: index > 0 && sample.break_before && e.mode != "join",
                alternative_ways,
            });
            raw_previous = Some(position);
            continue;
        }
        let mut choices = vec![];
        for c in candidates {
            if edit.is_some_and(|e| e.mode == "way" && e.way != Some(network.edges[c.edge].way)) {
                continue;
            }
            let geometry = if let Some(prev) = &previous {
                if raw_length > 500. {
                    None
                } else {
                    network.transition(prev, &c, (raw_length * 1.8 + 20.).min(1000.))
                }
            } else {
                Some(vec![c.position])
            };
            if let Some(g) = geometry {
                let length = g.windows(2).map(|w| distance(w[0], w[1])).sum::<f64>();
                let score = c.offset
                    + if previous.is_some() {
                        (length - raw_length).abs() * 0.4
                    } else {
                        0.
                    };
                choices.push((score, c, g));
            }
        }
        choices.sort_by(|a, b| a.0.total_cmp(&b.0));
        if edit.is_some_and(|e| e.mode == "way") && choices.is_empty() {
            return Err(error(
                "edit_conflict",
                "Chosen road has no nearby connected transition at this occurrence",
            ));
        }
        let ambiguous = choices.first().is_some_and(|best| {
            choices.iter().skip(1).any(|c| {
                c.0 - best.0 < 3.
                    && (distance(c.1.position, best.1.position) > 3. || {
                        let a = &network.edges[c.1.edge];
                        let b = &network.edges[best.1.edge];
                        a.way != b.way && a.a != b.a && a.a != b.b && a.b != b.a && a.b != b.b
                    })
            })
        });
        let (status, way, geometry) = if !ambiguous && !choices.is_empty() {
            let (_, c, mut g) = choices.remove(0);
            let way = network.edges[c.edge].way;
            if previous.is_none() {
                if let Some(p) = display_previous {
                    g.insert(0, p);
                }
            }
            let anchored = previous.is_some() || raw_previous.is_none();
            previous = Some(c);
            (
                if anchored {
                    if edit.is_some() {
                        "manual_match"
                    } else {
                        "matched"
                    }
                } else {
                    "unmatched_transition"
                },
                Some(way),
                g,
            )
        } else {
            previous = None;
            let mut g = vec![];
            if let Some(p) = display_previous {
                g.push(p);
            }
            g.push(position);
            (if ambiguous { "ambiguous" } else { "unmatched" }, None, g)
        };
        let length = geometry.windows(2).map(|w| distance(w[0], w[1])).sum();
        sections.push(Section {
            source_sample: source_samples[index],
            status: status.into(),
            way,
            recorded_elevation_m: sample.elevation_m,
            geometry,
            length_m: length,
            break_before: index > 0 && sample.break_before,
            alternative_ways,
        });
        raw_previous = Some(position);
    }
    Ok(sections)
}
pub fn build(root: &Path, revision: &str, cancel: Option<&Path>) -> Result<Snapshot> {
    let _guard = lock(root)?;
    let current = checked(root, revision)?;
    let preview = current
        .repair_preview
        .as_ref()
        .ok_or_else(|| error("source_required", "Select source roles first"))?;
    let plan = maps::plan(&current)?;
    let graph = maps::street_graph(root, &plan, cancel)?;
    if graph.sources.is_empty() {
        return Err(error(
            "maps_required",
            "Acquire street coverage before building the course",
        ));
    }
    let book = crate::corrections::load(root)?;
    crate::corrections::validate(&book, &current.source_revision, &plan, &graph.sources)?;
    let sections = match_with_edits(
        &preview.samples,
        &preview.source_samples,
        &graph,
        &book.decisions,
        cancel,
    )?;
    let course=Course{roads:None,schema_version:1,input_revision:current.source_revision.clone(),corrections_hash:crate::corrections::hash(&book),map_key:plan.key,map_sources:graph.sources,source_length_m:preview.length_m,length_m:sections.iter().map(|s|s.length_m).sum(),unresolved:sections.iter().filter(|s|!["matched","manual","manual_match"].contains(&s.status.as_str()) || s.break_before).count(),sections,elevation_note:"Recorded heights remain unchanged. Road matching and point edits affect the map only; surface heights and cross-source elevation alignment are not built yet.".into()};
    cancelled(cancel)?;
    maps::atomic_file(
        &root.join("course.json"),
        &serde_json::to_vec(&course).unwrap(),
    )?;
    open(root)
}
pub fn load(
    root: &Path,
    revision: &str,
    edits: &str,
    plan: Option<&maps::Plan>,
) -> Result<Option<Course>> {
    let path = root.join("course.json");
    if !path.exists() {
        return Ok(None);
    }
    if fs::metadata(&path)?.len() > 128 * 1024 * 1024 {
        return Err(error("invalid_course", "Course exceeds 128 MiB"));
    }
    let course: Course = serde_json::from_slice(&fs::read(path)?)
        .map_err(|e| error("invalid_course", e.to_string()))?;
    if course.schema_version != 1 {
        return Err(error("invalid_course", "Unsupported course version"));
    }
    // Derived results become stale when source choices or repairs change.
    let valid_maps = plan.is_some_and(|p| {
        p.key == course.map_key && maps::street_sources(root, p) == course.map_sources
    });
    Ok(
        (course.input_revision == revision && course.corrections_hash == edits && valid_maps)
            .then_some(course),
    )
}
