use openride_author::streets::parse;
const TILE: &str = r#"<osm version="0.6"><node id="1" version="1" lat="34" lon="-118"/><node id="2" version="1" lat="34.001" lon="-118"/><node id="3" version="1" lat="34.002" lon="-118"/><way id="10" version="2"><nd ref="1"/><nd ref="2"/><tag k="highway" v="cycleway"/><tag k="access" v="yes"/><tag k="bridge" v="yes"/><tag k="layer" v="1"/></way><way id="11" version="1"><nd ref="2"/><nd ref="3"/><tag k="highway" v="path"/></way><relation id="20" version="1"><member type="way" ref="10" role="outer"/><tag k="type" v="multipolygon"/></relation></osm>"#;
#[test]
fn boundary_dedup_preserves_identity_versions_tags_and_connectivity() {
    let mut graph = parse(TILE.as_bytes()).unwrap();
    graph.sources.insert("tile-a".into(), "hash-a".into());
    let mut next = parse(TILE.as_bytes()).unwrap();
    next.sources.insert("tile-b".into(), "hash-b".into());
    graph.merge(next).unwrap();
    graph.validate().unwrap();
    assert_eq!(graph.features.len(), 6);
    assert_eq!(graph.sources.len(), 2);
    assert_eq!(
        graph.features["way/10"].nodes[1],
        graph.features["way/11"].nodes[0]
    );
    assert_eq!(graph.features["way/10"].tags["bridge"], "yes");
    assert_eq!(graph.features["relation/20"].members[0].role, "outer");
    assert_eq!(graph.roads().len(), 2);
    let conflicting = TILE.replace("v=\"cycleway\"", "v=\"footway\"");
    assert!(graph.merge(parse(conflicting.as_bytes()).unwrap()).is_err());
    let newer = conflicting.replace("id=\"10\" version=\"2\"", "id=\"10\" version=\"3\"");
    graph.merge(parse(newer.as_bytes()).unwrap()).unwrap();
    assert_eq!(graph.features["way/10"].tags["highway"], "footway");
}
#[test]
fn malformed_incomplete_and_unsafe_sources_are_rejected() {
    for bad in [
        TILE.replace("ref=\"3\"", "ref=\"99\""),
        TILE.replace("lat=\"34\"", "lat=\"NaN\""),
        TILE.replace("</osm>", ""),
        TILE.replace("</osm>", "<remark>timeout</remark></osm>"),
        format!("<!DOCTYPE osm>{TILE}"),
        format!("{TILE}{TILE}"),
    ] {
        assert!(parse(bad.as_bytes()).is_err(), "{bad}");
    }
}
