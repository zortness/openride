//! Directed OSM shoreline closure, bounded to acquired tiles. No extrapolated ocean.
use crate::{
    maps::Tile,
    scenery::{inside, validate, Feature},
    streets::Graph,
};
use std::collections::{BTreeMap, BTreeSet};
type Point = [f64; 2];
fn close(a: Point, b: Point) -> bool {
    (a[0] - b[0]).abs() < 1e-10 && (a[1] - b[1]).abs() < 1e-10
}
fn clip(a: Point, b: Point, bounds: [f64; 4]) -> Option<(Point, Point)> {
    let d = [b[0] - a[0], b[1] - a[1]];
    let mut lo: f64 = 0.;
    let mut hi: f64 = 1.;
    for (p, q) in [
        (-d[0], a[0] - bounds[0]),
        (d[0], bounds[2] - a[0]),
        (-d[1], a[1] - bounds[1]),
        (d[1], bounds[3] - a[1]),
    ] {
        if p == 0. {
            if q < 0. {
                return None;
            }
        } else if p < 0. {
            lo = lo.max(q / p);
        } else {
            hi = hi.min(q / p);
        }
    }
    (lo < hi).then(|| {
        (
            [a[0] + lo * d[0], a[1] + lo * d[1]],
            [a[0] + hi * d[0], a[1] + hi * d[1]],
        )
    })
}
fn station(p: Point, b: [f64; 4]) -> Option<f64> {
    let w = b[2] - b[0];
    let h = b[3] - b[1];
    let near = |a: f64, c: f64| (a - c).abs() < 1e-9;
    if near(p[1], b[3]) {
        Some(p[0] - b[0])
    } else if near(p[0], b[2]) {
        Some(w + b[3] - p[1])
    } else if near(p[1], b[1]) {
        Some(w + h + b[2] - p[0])
    } else if near(p[0], b[0]) {
        Some(2. * w + h + p[1] - b[1])
    } else {
        None
    }
}
pub fn water(graph: &Graph, tiles: &[Tile]) -> (Vec<Feature>, Vec<String>) {
    let ways: Vec<_> = graph
        .features
        .values()
        .filter(|f| {
            f.kind == "way"
                && f.tags.get("natural").is_some_and(|v| v == "coastline")
                && f.nodes.len() > 1
        })
        .collect();
    let mut starts: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
    let mut ends: BTreeMap<i64, Vec<usize>> = BTreeMap::new();
    for (i, w) in ways.iter().enumerate() {
        starts.entry(w.nodes[0]).or_default().push(i);
        ends.entry(*w.nodes.last().unwrap()).or_default().push(i);
    }
    if starts.values().chain(ends.values()).any(|v| v.len() > 1) {
        return (
            vec![],
            vec!["Coastline branches or duplicates need review; ocean was not inferred".into()],
        );
    }
    let mut used = BTreeSet::new();
    let mut chains = vec![];
    let order: Vec<_> = (0..ways.len())
        .filter(|i| !ends.contains_key(&ways[*i].nodes[0]))
        .chain(0..ways.len())
        .collect();
    for i in order {
        if used.contains(&i) {
            continue;
        }
        let mut nodes = ways[i].nodes.clone();
        used.insert(i);
        let mut ids = vec![ways[i].id];
        while let Some(next) = starts
            .get(nodes.last().unwrap())
            .and_then(|v| v.first())
            .copied()
        {
            if !used.insert(next) {
                break;
            }
            nodes.extend(ways[next].nodes.iter().skip(1));
            ids.push(ways[next].id);
        }
        if nodes.len() > 50000 {
            return (
                vec![],
                vec!["Coastline exceeds 50,000-node chain budget".into()],
            );
        }
        let p: Vec<_> = nodes
            .iter()
            .filter_map(|n| {
                graph
                    .features
                    .get(&format!("node/{n}"))
                    .and_then(|f| f.position)
                    .map(|p| [p[1], p[0]])
            })
            .collect();
        chains.push((p, ids));
    }
    let mut features = vec![];
    let mut warnings = vec![];
    for tile in tiles {
        // Tile requests include a 16 m halo. Non-overlapping cores avoid duplicate surfaces.
        let [w, s, e, n] = tile.bounds;
        let dx = (e - w) * 16. / 2032.;
        let dy = (n - s) * 16. / 2032.;
        let b = [w + dx, s + dy, e - dx, n - dy];
        let mut runs: Vec<Vec<Point>> = vec![];
        let mut islands = vec![];
        let mut owners: BTreeSet<i64> = BTreeSet::new();
        for (chain, ids) in &chains {
            if chain.len() < 2 {
                continue;
            }
            if close(chain[0], *chain.last().unwrap())
                && chain
                    .iter()
                    .all(|p| p[0] > b[0] && p[0] < b[2] && p[1] > b[1] && p[1] < b[3])
            {
                let ring = chain[..chain.len() - 1].to_vec();
                let area = ring
                    .iter()
                    .zip(ring.iter().cycle().skip(1))
                    .take(ring.len())
                    .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
                    .sum::<f64>();
                if area > 0. {
                    islands.push(ring);
                    owners.extend(ids);
                } else {
                    warnings.push(format!(
                        "{}: reversed enclosed coastline needs review",
                        tile.id
                    ));
                }
                continue;
            }
            let mut run = vec![];
            for pair in chain.windows(2) {
                if let Some((a, c)) = clip(pair[0], pair[1], b) {
                    owners.extend(ids);
                    if !run.is_empty() && !close(*run.last().unwrap(), a) {
                        runs.push(std::mem::take(&mut run));
                    }
                    if run.is_empty() {
                        run.push(a);
                    }
                    run.push(c);
                } else if !run.is_empty() {
                    runs.push(std::mem::take(&mut run));
                }
            }
            if !run.is_empty() {
                runs.push(run);
            }
        }
        if runs.is_empty() && islands.is_empty() {
            continue;
        }
        let rectangle = vec![[b[0], b[3]], [b[2], b[3]], [b[2], b[1]], [b[0], b[1]]];
        let polygon = if runs.len() == 1 {
            let mut run = runs.pop().unwrap();
            let (Some(a), Some(mut end)) = (station(*run.last().unwrap(), b), station(run[0], b))
            else {
                warnings.push(format!(
                    "{}: shoreline ends inside source coverage; ocean not inferred",
                    tile.id
                ));
                continue;
            };
            let width = b[2] - b[0];
            let height = b[3] - b[1];
            let perimeter = 2. * (width + height);
            if end <= a {
                end += perimeter;
            }
            for lap in [0., perimeter] {
                for (t, p) in [0., width, width + height, 2. * width + height]
                    .into_iter()
                    .zip(&rectangle)
                {
                    if t + lap > a + 1e-12 && t + lap < end - 1e-12 {
                        run.push(*p);
                    }
                }
            }
            run
        } else if runs.is_empty() {
            rectangle
        } else {
            warnings.push(format!(
                "{}: multiple shoreline crossings need reviewed water polygons",
                tile.id
            ));
            continue;
        };
        let holes: Vec<_> = islands
            .into_iter()
            .filter(|h| inside(h[0], &polygon))
            .map(|r| r.into_iter().map(|p| [p[1], p[0]]).collect())
            .collect();
        let f=Feature{id:format!("coast/{}",tile.id),kind:"water".into(),outer:polygon.into_iter().map(|p|[p[1],p[0]]).collect(),holes,height_m:None,elevation_m:Some(0.),density:0.,trees:false,hidden:false,provenance:format!("Directed OSM coast from {} ways (identities in source catalog); nominal sea level 0 m; clipped to acquired tile core; vertical datum unvalidated",owners.len())};
        match validate(&f) {
            Ok(()) => features.push(f),
            Err(e) => warnings.push(format!("{}: {}", tile.id, e.message)),
        }
    }
    warnings.truncate(1000);
    (features, warnings)
}
