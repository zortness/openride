//! OSM identity is topology: coincident coordinates never create a junction.
use crate::{error, Result};
use quick_xml::{events::Event, Reader};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Member {
    pub kind: String,
    pub id: i64,
    pub role: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Feature {
    pub id: i64,
    pub version: u64,
    pub kind: String,
    pub position: Option<[f64; 2]>,
    pub nodes: Vec<i64>,
    pub members: Vec<Member>,
    pub tags: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Graph {
    pub features: BTreeMap<String, Feature>,
    pub sources: BTreeMap<String, String>,
}
fn invalid(message: impl Into<String>) -> crate::Error {
    error("invalid_osm", message)
}
fn number<T: std::str::FromStr>(a: &BTreeMap<String, String>, name: &str) -> Result<T> {
    a.get(name)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| invalid(format!("Missing or invalid {name}")))
}
impl Graph {
    pub fn merge(&mut self, other: Graph) -> Result<()> {
        for (key, feature) in other.features {
            if let Some(old) = self.features.get(&key) {
                if old.version == feature.version && *old != feature {
                    return Err(error(
                        "map_conflict",
                        format!("Conflicting OSM {key} version {}", old.version),
                    ));
                }
                if old.version >= feature.version {
                    continue;
                }
            }
            self.features.insert(key, feature);
        }
        self.sources.extend(other.sources);
        if self.features.len() > 2_000_000 {
            return Err(error(
                "map_limit",
                "Street graph exceeds two million features",
            ));
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<()> {
        for f in self.features.values() {
            if f.kind == "way"
                && f.nodes
                    .iter()
                    .any(|n| !self.features.contains_key(&format!("node/{n}")))
            {
                return Err(invalid(format!(
                    "Way {} has missing nodes; retry coverage",
                    f.id
                )));
            }
            if f.members
                .iter()
                .any(|m| !self.features.contains_key(&format!("{}/{}", m.kind, m.id)))
            {
                return Err(invalid(format!("Relation {} has missing members", f.id)));
            }
        }
        Ok(())
    }
    pub fn roads(&self) -> Vec<Vec<[f64; 2]>> {
        self.features
            .values()
            .filter(|f| f.kind == "way" && f.tags.contains_key("highway"))
            .map(|f| {
                f.nodes
                    .iter()
                    .filter_map(|n| {
                        self.features
                            .get(&format!("node/{n}"))
                            .and_then(|p| p.position)
                    })
                    .collect()
            })
            .collect()
    }
}
pub fn parse(raw: &[u8]) -> Result<Graph> {
    if raw.len() > 64 * 1024 * 1024 {
        return Err(invalid("OSM exceeds 64 MiB"));
    }
    let mut reader = Reader::from_reader(raw);
    reader.config_mut().trim_text(true);
    let mut graph = Graph::default();
    let mut stack: Vec<String> = Vec::new();
    let mut current: Option<Feature> = None;
    let mut root_seen = false;
    loop {
        let event = reader.read_event().map_err(|e| invalid(e.to_string()))?;
        match event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let name = std::str::from_utf8(e.name().as_ref())
                    .map_err(|e| invalid(e.to_string()))?
                    .to_string();
                let empty = matches!(event, Event::Empty(_));
                let mut a = BTreeMap::new();
                for attr in e.attributes() {
                    let attr = attr.map_err(|e| invalid(e.to_string()))?;
                    let key = std::str::from_utf8(attr.key.as_ref())
                        .map_err(|e| invalid(e.to_string()))?
                        .to_string();
                    let value = attr
                        .decode_and_unescape_value(reader.decoder())
                        .map_err(|e| invalid(e.to_string()))?
                        .into_owned();
                    if value.len() > 8192 || a.insert(key, value).is_some() {
                        return Err(invalid("Oversized or duplicate OSM attribute"));
                    }
                }
                let parent = stack.last().map(String::as_str).unwrap_or("");
                match name.as_str() {
                    "osm" if parent.is_empty() && !root_seen => {
                        root_seen = true;
                        if a.get("version").map(String::as_str) != Some("0.6") {
                            return Err(invalid("Unsupported OSM version"));
                        }
                    }
                    "node" | "way" | "relation" if parent == "osm" && current.is_none() => {
                        let id: i64 = number(&a, "id")?;
                        if id <= 0 {
                            return Err(invalid("OSM IDs must be positive"));
                        }
                        let position = if name == "node" {
                            let lat: f64 = number(&a, "lat")?;
                            let lon: f64 = number(&a, "lon")?;
                            if !lat.is_finite()
                                || !lon.is_finite()
                                || lat.abs() > 90.
                                || lon.abs() > 180.
                            {
                                return Err(invalid("Invalid OSM coordinate"));
                            }
                            Some([lat, lon])
                        } else {
                            None
                        };
                        current = Some(Feature {
                            id,
                            version: number(&a, "version")?,
                            kind: name.clone(),
                            position,
                            nodes: Vec::new(),
                            members: Vec::new(),
                            tags: BTreeMap::new(),
                        });
                    }
                    "tag" if ["node", "way", "relation"].contains(&parent) => {
                        let f = current.as_mut().ok_or_else(|| invalid("Orphan tag"))?;
                        let k = a
                            .get("k")
                            .ok_or_else(|| invalid("Tag key missing"))?
                            .clone();
                        let v = a
                            .get("v")
                            .ok_or_else(|| invalid("Tag value missing"))?
                            .clone();
                        if f.tags.len() >= 1024 || f.tags.insert(k, v).is_some() {
                            return Err(invalid("Duplicate or excessive tags"));
                        }
                    }
                    "nd" if parent == "way" => {
                        let f = current.as_mut().unwrap();
                        if f.nodes.len() >= 100_000 {
                            return Err(invalid("Way too large"));
                        }
                        f.nodes.push(number(&a, "ref")?);
                    }
                    "member" if parent == "relation" => {
                        let kind = a
                            .get("type")
                            .ok_or_else(|| invalid("Member type missing"))?
                            .clone();
                        if !["node", "way", "relation"].contains(&kind.as_str()) {
                            return Err(invalid("Invalid member type"));
                        }
                        let f = current.as_mut().unwrap();
                        if f.members.len() >= 100_000 {
                            return Err(invalid("Relation too large"));
                        }
                        f.members.push(Member {
                            kind,
                            id: number(&a, "ref")?,
                            role: a.get("role").cloned().unwrap_or_default(),
                        });
                    }
                    "note" | "meta" | "bounds" if parent == "osm" => {}
                    _ => return Err(invalid(format!("Unexpected OSM element {name}"))),
                }
                if empty {
                    if ["node", "way", "relation"].contains(&name.as_str()) {
                        insert(&mut graph, current.take().unwrap())?;
                    }
                } else {
                    stack.push(name);
                }
            }
            Event::End(e) => {
                let name = stack
                    .pop()
                    .ok_or_else(|| invalid("Unexpected closing element"))?;
                if e.name().as_ref() != name.as_bytes() {
                    return Err(invalid("Mismatched OSM elements"));
                }
                if ["node", "way", "relation"].contains(&name.as_str()) {
                    insert(
                        &mut graph,
                        current.take().ok_or_else(|| invalid("Missing feature"))?,
                    )?;
                }
            }
            Event::Eof => break,
            Event::Decl(_) | Event::Comment(_) => {}
            Event::Text(e)
                if stack.last().is_some_and(|s| s == "note")
                    || e.iter().all(u8::is_ascii_whitespace) => {}
            _ => return Err(invalid("Unsupported OSM XML content")),
        }
    }
    if !root_seen || !stack.is_empty() {
        return Err(invalid("Incomplete OSM document"));
    }
    graph.validate()?;
    Ok(graph)
}
fn insert(graph: &mut Graph, f: Feature) -> Result<()> {
    if f.kind == "way" && f.nodes.len() < 2 {
        return Err(invalid("Way needs at least two nodes"));
    }
    let key = format!("{}/{}", f.kind, f.id);
    if graph.features.len() >= 2_000_000 || graph.features.insert(key, f).is_some() {
        return Err(invalid("Duplicate or excessive OSM features"));
    }
    Ok(())
}
