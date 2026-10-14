//! Road-first authoring. Recordings are evidence; connected OSM roads own geometry.
use crate::{
    cancelled, checked,
    course::{Candidate, Course, Network, Section},
    error, lock, maps, open,
    streets::Graph,
    Result, Snapshot,
};
use ride_core::Sample;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Metadata {
    pub policy: String,
    pub way_names: BTreeMap<i64, String>,
    #[serde(default)]
    pub nearby_roads: BTreeMap<i64, Vec<[f64; 2]>>,
    #[serde(default)]
    pub overlay_limited: bool,
    /// One way identity for each geometry segment, including intermediate roads.
    pub section_ways: BTreeMap<usize, Vec<i64>>,
    pub issues: Vec<Issue>,
    #[serde(default)]
    pub choice_groups: BTreeMap<usize, usize>,
    #[serde(default)]
    pub connection_previews: BTreeMap<usize, Vec<[f64; 2]>>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Issue {
    pub first_sample: usize,
    pub last_sample: usize,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct State {
    schema_version: u32,
    source_revision: String,
    choices: BTreeMap<usize, i64>,
    #[serde(default)]
    batches: Vec<ChoiceBatch>,
    #[serde(default)]
    connections: BTreeMap<usize, usize>,
    course: Course,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct ChoiceBatch {
    previous: BTreeMap<usize, Option<i64>>,
}

/// Extend only through an unbranched chain of identically named/ref'd road ways.
/// Unnamed roads are limited to their exact OSM identity.
fn road_chain(graph: &Graph, way: i64) -> BTreeSet<i64> {
    let mut chain = BTreeSet::from([way]);
    let Some(seed) = graph.features.get(&format!("way/{way}")) else {
        return chain;
    };
    let Some(name) = seed
        .tags
        .get("name")
        .or_else(|| seed.tags.get("ref"))
        .filter(|s| !s.is_empty())
    else {
        return chain;
    };
    let mut endpoints: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    for feature in graph.features.values().filter(|f| {
        f.kind == "way"
            && f.tags.get("name").or_else(|| f.tags.get("ref")) == Some(name)
            && f.tags.get("highway") == seed.tags.get("highway")
    }) {
        for node in [feature.nodes.first(), feature.nodes.last()]
            .into_iter()
            .flatten()
        {
            endpoints.entry(*node).or_default().push(feature.id);
        }
    }
    let mut pending = vec![way];
    while let Some(id) = pending.pop() {
        let f = &graph.features[&format!("way/{id}")];
        for node in [f.nodes.first(), f.nodes.last()].into_iter().flatten() {
            let adjacent = &endpoints[node];
            if adjacent.len() == 2 {
                for other in adjacent {
                    if chain.insert(*other) {
                        pending.push(*other);
                    }
                }
            }
        }
    }
    chain
}

/// Plan in occurrence order, never by geographical proximity alone. The 5 km
/// bound applies in each direction; a later return to the road is a separate visit.
pub fn nearby_choices(
    samples: &[Sample],
    sections: &[Section],
    graph: &Graph,
    existing: &BTreeMap<usize, i64>,
    sample: usize,
    way: i64,
) -> Result<BTreeMap<usize, i64>> {
    let start = sections
        .iter()
        .position(|s| s.source_sample == sample)
        .ok_or_else(|| error("invalid_edit", "Select an inferred road occurrence first"))?;
    if !sections[start].alternative_ways.contains(&way) {
        return Err(error(
            "invalid_edit",
            "Selected road is not available at this occurrence",
        ));
    }
    let chain = road_chain(&road_graph(graph), way);
    let mut result = BTreeMap::from([(sample, way)]);
    if !["ambiguous", "matched", "manual_match"].contains(&sections[start].status.as_str()) {
        return Err(error(
            "invalid_edit",
            "Resolve gaps or disconnected roads individually before propagating a choice",
        ));
    }
    for direction in [-1isize, 1] {
        let mut previous = start;
        let mut traveled = 0.;
        loop {
            let next = previous as isize + direction;
            if next < 0 || next >= sections.len() as isize {
                break;
            }
            let next = next as usize;
            let section = &sections[next];
            let a = sections[previous].source_sample;
            let b = section.source_sample;
            if existing.contains_key(&b)
                || !["ambiguous", "matched"].contains(&section.status.as_str())
                || sections[previous.max(next)].break_before
                || samples[a.min(b) + 1..=a.max(b)]
                    .iter()
                    .any(|s| s.break_before || s.latitude.is_none() || s.longitude.is_none())
            {
                break;
            }
            let position = |i: usize| {
                samples[i]
                    .latitude
                    .zip(samples[i].longitude)
                    .map(|(a, b)| [a, b])
            };
            let (Some(p), Some(q)) = (position(a), position(b)) else {
                break;
            };
            let step = distance(p, q);
            traveled += step;
            if step > 500. || traveled > 5000. {
                break;
            }
            // Do not propagate around an out-and-back turnaround.
            let low = previous.min(next);
            if low > 0 {
                if let (Some(x), Some(y), Some(z)) = (
                    position(sections[low - 1].source_sample),
                    position(sections[low].source_sample),
                    position(sections[low + 1].source_sample),
                ) {
                    let cos_lat = y[0].to_radians().cos();
                    let u = [y[0] - x[0], (y[1] - x[1]) * cos_lat];
                    let v = [z[0] - y[0], (z[1] - y[1]) * cos_lat];
                    if u[0] * v[0] + u[1] * v[1] < 0. {
                        break;
                    }
                }
            }
            let candidates: Vec<_> = section
                .alternative_ways
                .iter()
                .copied()
                .filter(|id| chain.contains(id))
                .collect();
            let chosen = if section.way.is_some_and(|id| candidates.contains(&id)) {
                section.way
            } else if candidates.len() == 1 && section.status == "ambiguous" {
                Some(candidates[0])
            } else {
                None
            };
            let Some(chosen) = chosen else { break };
            // Save constraints only where review is needed; traverse automatic matches.
            if section.status == "ambiguous" {
                result.insert(b, chosen);
            }
            previous = next;
        }
    }
    Ok(result)
}
fn digest(raw: &[u8]) -> String {
    format!("{:x}", Sha256::digest(raw))
}
fn read(root: &Path) -> Result<Option<State>> {
    let path = root.join("roads.json");
    if !path.exists() {
        return Ok(None);
    }
    if fs::metadata(&path)?.len() > 64 * 1024 * 1024 {
        return Err(error("invalid_roads", "Road plan exceeds 64 MiB"));
    }
    let state: State = serde_json::from_slice(&fs::read(path)?)
        .map_err(|e| error("invalid_roads", e.to_string()))?;
    if state.schema_version != 1 || state.course.roads.is_none() {
        return Err(error("invalid_roads", "Unsupported road plan"));
    }
    Ok(Some(state))
}
pub(crate) fn load(root: &Path, snapshot: &mut Snapshot) -> Result<()> {
    if let Some(mut state) = read(root)? {
        snapshot.revision = digest(
            format!(
                "{}:{}",
                snapshot.revision,
                digest(&serde_json::to_vec(&state).unwrap())
            )
            .as_bytes(),
        );
        let valid = snapshot
            .review
            .as_ref()
            .is_some_and(|r| r.revision == state.source_revision)
            && snapshot.maps.as_ref().is_some_and(|p| {
                p.key == state.course.map_key
                    && maps::street_sources(root, p) == state.course.map_sources
            });
        // Never fall back silently to a recording-derived course when roads become stale.
        if valid {
            if let Some(review) = &snapshot.review {
                if let Some(source) = review
                    .activities
                    .iter()
                    .find(|a| Some(&a.source_sha256) == review.document.geometry_source.as_ref())
                {
                    for pair in state.course.sections.windows(2) {
                        if pair[1].status == "disconnected" {
                            if let Some(geometry) =
                                connection_geometry(&source.samples, &pair[0], &pair[1])
                            {
                                state
                                    .course
                                    .roads
                                    .as_mut()
                                    .unwrap()
                                    .connection_previews
                                    .insert(pair[1].source_sample, geometry);
                            }
                        }
                    }
                }
            }
        }
        snapshot.course = valid.then_some(state.course);
    }
    Ok(())
}
pub fn road_graph(graph: &Graph) -> Graph {
    let mut result = graph.clone();
    result.features.retain(|_, f| {
        if f.kind != "way" {
            return true;
        }
        let tag = |key: &str| f.tags.get(key).map(String::as_str).unwrap_or("");
        let street = [
            "trunk",
            "trunk_link",
            "primary",
            "primary_link",
            "secondary",
            "secondary_link",
            "tertiary",
            "tertiary_link",
            "unclassified",
            "residential",
            "living_street",
            "service",
            "road",
            "cycleway",
        ]
        .contains(&tag("highway"));
        let paved_path = ["path", "footway"].contains(&tag("highway"))
            && ["yes", "designated"].contains(&tag("bicycle"))
            && ["asphalt", "concrete", "paved", "paving_stones"].contains(&tag("surface"));
        (street || paved_path)
            && ![
                "unpaved",
                "gravel",
                "fine_gravel",
                "dirt",
                "earth",
                "ground",
                "sand",
                "mud",
                "grass",
            ]
            .contains(&tag("surface"))
    });
    for f in result.features.values_mut() {
        if f.tags.get("junction").is_some_and(|v| v == "roundabout")
            && !f.tags.contains_key("oneway")
        {
            f.tags.insert("oneway".into(), "yes".into());
        }
    }
    result
}
fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    ride_core::haversine(a[0], a[1], b[0], b[1])
}
fn route_length(g: &[[f64; 2]]) -> f64 {
    g.windows(2).map(|w| distance(w[0], w[1])).sum()
}

fn connection_geometry(
    samples: &[Sample],
    previous: &Section,
    section: &Section,
) -> Option<Vec<[f64; 2]>> {
    let start = previous.source_sample;
    let end = section.source_sample;
    if start >= end || end >= samples.len() {
        return None;
    }
    let mut geometry = vec![*previous.geometry.last()?];
    for (offset, sample) in samples[start..=end].iter().enumerate() {
        if offset > 0 && sample.break_before {
            return None;
        }
        geometry.push([sample.latitude?, sample.longitude?]);
    }
    geometry.push(*section.geometry.first()?);
    geometry.dedup();
    let length = route_length(&geometry);
    (length > 0.01 && length <= 250.).then_some(geometry)
}

fn apply_connections(
    samples: &[Sample],
    sections: &mut [Section],
    roads: &mut Metadata,
    connections: &BTreeMap<usize, usize>,
) -> Result<()> {
    for (&end, &start) in connections {
        let index = sections.iter().position(|s| s.source_sample == end)
            .filter(|&i| i > 0 && sections[i-1].source_sample == start)
            .ok_or_else(|| error("connection_conflict", "Connection endpoints changed; undo the connection before changing nearby road choices"))?;
        let geometry = connection_geometry(samples, &sections[index - 1], &sections[index])
            .ok_or_else(|| {
                error(
                    "connection_conflict",
                    "A recorded connection must be continuous and no longer than 250 m",
                )
            })?;
        if sections[index].status != "disconnected" {
            return Err(error("connection_conflict", "This connection no longer matches a road break; undo it before changing nearby road choices"));
        }
        let section = &mut sections[index];
        section.length_m = route_length(&geometry);
        section.geometry = geometry;
        section.way = None;
        section.status = "manual_connection".into();
        section.break_before = false;
        roads.section_ways.remove(&end);
    }
    roads.issues.clear();
    for (i, section) in sections.iter().enumerate() {
        if ["matched", "manual_match", "manual_connection"].contains(&section.status.as_str()) {
            continue;
        }
        if let Some(last) = roads.issues.last_mut().filter(|issue| {
            issue.reason == section.status
                && i > 0
                && issue.last_sample == sections[i - 1].source_sample
        }) {
            last.last_sample = section.source_sample;
        } else {
            roads.issues.push(Issue {
                first_sample: section.source_sample,
                last_sample: section.source_sample,
                reason: section.status.clone(),
            });
        }
    }
    Ok(())
}
struct Observation {
    source: usize,
    position: [f64; 2],
    gap: bool,
    candidates: Vec<Candidate>,
}
struct Layer {
    forward: Vec<f64>,
    parent: Vec<usize>,
    transition: Vec<Vec<f64>>,
}

/// Forward/backward sequence scoring lets later observations resolve earlier choices.
/// Distinct equally plausible roads remain review items rather than nearest-road guesses.
pub fn infer(
    samples: &[Sample],
    graph: &Graph,
    choices: &BTreeMap<usize, i64>,
    cancel: Option<&Path>,
) -> Result<(Vec<Section>, Metadata)> {
    let filtered = road_graph(graph);
    let origin = samples
        .iter()
        .find_map(|s| s.latitude.zip(s.longitude).map(|(a, b)| [a, b]))
        .ok_or_else(|| error("source_required", "Recording has no positions"))?;
    let network = Network::new(&filtered, origin);
    let mut observations: Vec<Observation> = vec![];
    let mut gap = false;
    for (i, s) in samples.iter().enumerate() {
        cancelled(cancel)?;
        gap |= s.break_before && i > 0;
        let Some(p) = s.latitude.zip(s.longitude).map(|(a, b)| [a, b]) else {
            gap = true;
            continue;
        };
        // Reduce stationary GPS noise and redundant fixes, retaining explicit choices.
        if !gap
            && i + 1 < samples.len()
            && !choices.contains_key(&i)
            && observations
                .last()
                .is_some_and(|o| distance(o.position, p) < 15.)
        {
            continue;
        }
        let mut candidates = network.candidates(p);
        if let Some(way) = choices.get(&i) {
            candidates.retain(|c| network.edges[c.edge].way == *way);
            if candidates.is_empty() {
                return Err(error(
                    "road_choice_conflict",
                    format!("Chosen road is unavailable near recording sample {i}"),
                ));
            }
        }
        observations.push(Observation {
            source: i,
            position: p,
            gap,
            candidates,
        });
        gap = false;
    }
    if observations.len() < 2 || observations.len() > 20000 {
        return Err(error(
            "road_limit",
            "Road inference requires 2–20,000 spaced observations",
        ));
    }
    let mut meta = Metadata {
        policy: "road-first-v1; terrain elevations; paved road rides including cycleways".into(),
        ..Default::default()
    };
    for f in filtered.features.values().filter(|f| f.kind == "way") {
        meta.way_names.insert(
            f.id,
            f.tags
                .get("name")
                .or_else(|| f.tags.get("ref"))
                .cloned()
                .unwrap_or_else(|| {
                    format!(
                        "Unnamed {}",
                        f.tags.get("highway").map(String::as_str).unwrap_or("road")
                    )
                }),
        );
    }
    let mut layers: Vec<Layer> = vec![];
    let mut run_starts = vec![0];
    for (i, o) in observations.iter().enumerate() {
        cancelled(cancel)?;
        let mut layer = Layer {
            forward: vec![f64::INFINITY; o.candidates.len()],
            parent: vec![0; o.candidates.len()],
            transition: vec![],
        };
        let raw = if i > 0 {
            distance(observations[i - 1].position, o.position)
        } else {
            0.
        };
        let connect = i > 0 && !observations[i - 1].candidates.is_empty() && raw <= 500.;
        if connect {
            for (k, a) in observations[i - 1].candidates.iter().enumerate() {
                let mut costs = vec![f64::INFINITY; o.candidates.len()];
                for (j, b) in o.candidates.iter().enumerate() {
                    if let Some((g, _)) =
                        network.transition_roads(a, b, (raw * 2. + 40.).min(1200.))
                    {
                        let cost = (route_length(&g) - raw).abs() / 10.;
                        costs[j] = cost;
                        let score = layers[i - 1].forward[k] + cost + b.offset * b.offset / 50.;
                        if score < layer.forward[j] {
                            layer.forward[j] = score;
                            layer.parent[j] = k;
                        }
                    }
                }
                layer.transition.push(costs);
            }
        }
        if !layer.forward.iter().any(|s| s.is_finite()) {
            if i > 0 {
                run_starts.push(i);
            }
            for (j, c) in o.candidates.iter().enumerate() {
                layer.forward[j] = c.offset * c.offset / 50.;
            }
            layer.transition.clear();
        }
        layers.push(layer);
    }
    run_starts.push(observations.len());
    let mut selected = vec![0; observations.len()];
    let mut ambiguous = vec![false; observations.len()];
    for range in run_starts.windows(2) {
        let (start, end) = (range[0], range[1]);
        if observations[start].candidates.is_empty() {
            continue;
        }
        let last = end - 1;
        selected[last] = layers[last]
            .forward
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.total_cmp(b.1))
            .unwrap()
            .0;
        let best = layers[last].forward[selected[last]];
        let mut suffix = vec![0.; observations[last].candidates.len()];
        for i in (start..end).rev() {
            let chosen = &observations[i].candidates[selected[i]];
            ambiguous[i] = observations[i].candidates.iter().enumerate().any(|(j, c)| {
                let a = &network.edges[c.edge];
                let b = &network.edges[chosen.edge];
                // Nearby projections along the same road are not competing road
                // choices. Keep genuinely separate ways/branches reviewable.
                let different = a.way != b.way
                    && (distance(c.position, chosen.position) > 8.
                        || (a.a != b.a && a.a != b.b && a.b != b.a && a.b != b.b));
                different && layers[i].forward[j] + suffix[j] - best < 3.
            });
            if i > start {
                selected[i - 1] = layers[i].parent[selected[i]];
                suffix = layers[i]
                    .transition
                    .iter()
                    .map(|costs| {
                        costs
                            .iter()
                            .enumerate()
                            .map(|(j, c)| {
                                c + observations[i].candidates[j].offset.powi(2) / 50. + suffix[j]
                            })
                            .fold(f64::INFINITY, f64::min)
                    })
                    .collect();
            }
        }
    }
    let mut sections = vec![];
    for (i, o) in observations.iter().enumerate() {
        cancelled(cancel)?;
        let candidate = o.candidates.get(selected[i]);
        let mut geometry = candidate
            .map(|c| vec![c.position])
            .unwrap_or_else(|| vec![o.position]);
        let mut ways = vec![];
        let connected = i > 0 && !layers[i].transition.is_empty();
        if connected {
            let a = &observations[i - 1].candidates[selected[i - 1]];
            let raw = distance(observations[i - 1].position, o.position);
            (geometry, ways) = network
                .transition_roads(a, candidate.unwrap(), (raw * 2. + 40.).min(1200.))
                .unwrap();
        }
        let status = if candidate.is_none() {
            "missing_road"
        } else if !connected && i > 0 {
            "disconnected"
        } else if ambiguous[i] {
            "ambiguous"
        } else if o.gap && !choices.contains_key(&o.source) {
            "recording_gap"
        } else if choices.contains_key(&o.source) {
            "manual_match"
        } else {
            "matched"
        };
        if !["matched", "manual_match"].contains(&status) {
            if let Some(last) = meta.issues.last_mut().filter(|v| {
                v.reason == status
                    && sections
                        .last()
                        .is_some_and(|s: &Section| s.source_sample == v.last_sample)
            }) {
                last.last_sample = o.source;
            } else {
                meta.issues.push(Issue {
                    first_sample: o.source,
                    last_sample: o.source,
                    reason: status.into(),
                });
            }
        }
        meta.section_ways.insert(o.source, ways);
        sections.push(Section {
            source_sample: o.source,
            status: status.into(),
            way: candidate.map(|c| network.edges[c.edge].way),
            recorded_elevation_m: samples[o.source].elevation_m,
            geometry: geometry.clone(),
            length_m: route_length(&geometry),
            break_before: i > 0 && !connected,
            alternative_ways: network
                .candidates(o.position)
                .iter()
                .map(|c| network.edges[c.edge].way)
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect(),
        });
    }
    // Only names referenced by this course/options need to cross the CLI boundary.
    let referenced: std::collections::BTreeSet<_> = sections
        .iter()
        .flat_map(|s| s.alternative_ways.iter().copied())
        .chain(meta.section_ways.values().flatten().copied())
        .collect();
    meta.way_names.retain(|id, _| referenced.contains(id));
    let mut overlay_points = 0;
    for id in &referenced {
        if let Some(f) = filtered.features.get(&format!("way/{id}")) {
            if overlay_points + f.nodes.len() > 100000 || f.nodes.len() > 5000 {
                meta.overlay_limited = true;
                continue;
            }
            overlay_points += f.nodes.len();
            meta.nearby_roads.insert(
                *id,
                f.nodes
                    .iter()
                    .filter_map(|n| {
                        filtered
                            .features
                            .get(&format!("node/{n}"))
                            .and_then(|f| f.position)
                    })
                    .take(5000)
                    .collect(),
            );
        }
    }
    for pair in sections.windows(2) {
        if pair[1].status == "disconnected" {
            if let Some(geometry) = connection_geometry(samples, &pair[0], &pair[1]) {
                meta.connection_previews
                    .insert(pair[1].source_sample, geometry);
            }
        }
    }
    Ok((sections, meta))
}
pub fn build(
    root: &Path,
    revision: &str,
    choice: Option<(usize, Option<i64>)>,
    cancel: Option<&Path>,
) -> Result<Snapshot> {
    execute(root, revision, choice, false, false, None, cancel)
}
pub fn choose_nearby(
    root: &Path,
    revision: &str,
    sample: usize,
    way: i64,
    cancel: Option<&Path>,
) -> Result<Snapshot> {
    execute(
        root,
        revision,
        Some((sample, Some(way))),
        false,
        true,
        None,
        cancel,
    )
}
pub fn reset(root: &Path, revision: &str, cancel: Option<&Path>) -> Result<Snapshot> {
    execute(root, revision, None, true, false, None, cancel)
}
pub fn connect(
    root: &Path,
    revision: &str,
    sample: usize,
    undo: bool,
    cancel: Option<&Path>,
) -> Result<Snapshot> {
    execute(
        root,
        revision,
        None,
        false,
        false,
        Some((sample, undo)),
        cancel,
    )
}
fn execute(
    root: &Path,
    revision: &str,
    choice: Option<(usize, Option<i64>)>,
    reset: bool,
    nearby: bool,
    connection: Option<(usize, bool)>,
    cancel: Option<&Path>,
) -> Result<Snapshot> {
    let guard = lock(root)?;
    let current = checked(root, revision)?;
    let review = current
        .review
        .as_ref()
        .ok_or_else(|| error("source_required", "Import a recording first"))?;
    let source = review
        .activities
        .iter()
        .find(|a| Some(&a.source_sha256) == review.document.geometry_source.as_ref())
        .ok_or_else(|| error("source_required", "Choose a reference recording"))?;
    let plan = maps::plan(&current)?;
    let graph = maps::street_graph(root, &plan, cancel)?;
    if graph.sources.is_empty() {
        return Err(error("maps_required", "Download road maps first"));
    }
    let old = read(root)?;
    let mut choices = old.as_ref().map(|s| s.choices.clone()).unwrap_or_default();
    let mut batches = old.as_ref().map(|s| s.batches.clone()).unwrap_or_default();
    let mut connections = old
        .as_ref()
        .map(|s| s.connections.clone())
        .unwrap_or_default();
    if reset {
        choices.clear();
        batches.clear();
        connections.clear();
    }
    if (!choices.is_empty() || !connections.is_empty())
        && old.as_ref().is_some_and(|s| {
            s.source_revision != review.revision || s.course.map_sources != graph.sources
        })
    {
        return Err(error("road_choice_conflict","Recording or road maps changed. Use Restart road inference to archive previous choices and review the new roads."));
    }
    if let Some((sample, way)) = choice {
        if sample >= source.samples.len() {
            return Err(error(
                "invalid_edit",
                "Recording occurrence is out of range",
            ));
        }
        if let Some(way) = way {
            let applied = if nearby {
                let course = current
                    .course
                    .as_ref()
                    .filter(|c| c.roads.is_some())
                    .ok_or_else(|| {
                        error("invalid_edit", "Infer roads before propagating a choice")
                    })?;
                nearby_choices(
                    &source.samples,
                    &course.sections,
                    &graph,
                    &choices,
                    sample,
                    way,
                )?
            } else {
                BTreeMap::from([(sample, way)])
            };
            // A later individual edit takes ownership of that point from its old batch.
            for batch in &mut batches {
                batch.previous.remove(&sample);
            }
            batches.retain(|b| !b.previous.is_empty());
            let previous = applied
                .into_iter()
                .map(|(sample, way)| (sample, choices.insert(sample, way)))
                .collect();
            if nearby {
                batches.push(ChoiceBatch { previous });
            }
        } else {
            if let Some(index) = batches
                .iter()
                .position(|b| b.previous.contains_key(&sample))
            {
                for (sample, previous) in batches.remove(index).previous {
                    if let Some(way) = previous {
                        choices.insert(sample, way);
                    } else {
                        choices.remove(&sample);
                    }
                }
            } else {
                choices.remove(&sample);
            }
        }
    }
    let (mut sections, mut roads) = infer(&source.samples, &graph, &choices, cancel)?;
    if let Some((sample, undo)) = connection {
        if undo {
            connections.remove(&sample);
        } else {
            let index = sections.iter().position(|s| s.source_sample == sample)
                .filter(|&i| i > 0 && roads.connection_previews.contains_key(&sample))
                .ok_or_else(|| error("invalid_connection", "Select a disconnected stretch with a continuous recording of at most 250 m"))?;
            connections.insert(sample, sections[index - 1].source_sample);
        }
    }
    apply_connections(&source.samples, &mut sections, &mut roads, &connections)?;
    for batch in &batches {
        for sample in batch.previous.keys() {
            roads.choice_groups.insert(*sample, batch.previous.len());
        }
    }
    let course=Course{schema_version:1,input_revision:review.revision.clone(),corrections_hash:digest(&serde_json::to_vec(&choices).unwrap()),map_key:plan.key,map_sources:graph.sources,source_length_m:source.route_distance_m,length_m:sections.iter().map(|s|s.length_m).sum(),unresolved:roads.issues.len(),sections,elevation_note:"Course heights come from terrain along the selected roads. Recording elevations remain reference data. Bridge profiles interpolate between terrain at their ends; tunnel previews are not yet supported.".into(),roads:Some(roads)};
    let mut course = course;
    course.corrections_hash = digest(&serde_json::to_vec(&(&choices, &connections)).unwrap());
    let state = State {
        schema_version: 1,
        source_revision: review.revision.clone(),
        choices,
        batches,
        connections,
        course,
    };
    cancelled(cancel)?;
    if reset {
        if let Some(old) = old {
            let raw = serde_json::to_vec(&old).unwrap();
            fs::create_dir_all(root.join("jobs"))?;
            maps::atomic_file(
                &root
                    .join("jobs")
                    .join(format!("roads-{}.json", digest(&raw))),
                &raw,
            )?;
        }
    }
    let raw = serde_json::to_vec(&state).unwrap();
    if raw.len() > 64 * 1024 * 1024 {
        return Err(error(
            "road_limit",
            "Road plan exceeds 64 MiB; previous plan retained",
        ));
    }
    maps::atomic_file(&root.join("roads.json"), &raw)?;
    drop(guard);
    open(root)
}
