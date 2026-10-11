use ride_core::import_xml;
#[test]
fn kml_lines_preserve_boundaries_and_only_absolute_altitudes() {
    let a=import_xml(r#"<kml xmlns="http://www.opengis.net/kml/2.2"><Document><Placemark><name>Test</name><MultiGeometry><LineString><altitudeMode>absolute</altitudeMode><coordinates>-118,33,12 -118.001,33,14</coordinates></LineString><LineString><coordinates>-118.002,33,0 -118.003,33,0</coordinates></LineString></MultiGeometry></Placemark></Document></kml>"#,"sample.kml").unwrap();
    assert_eq!(a.format,"KML");assert_eq!(a.samples.len(),4);
    assert_eq!(a.samples[0].elevation_m,Some(12.));assert_eq!(a.samples[2].elevation_m,None);
    assert!(a.samples[2].break_before);assert!(a.samples[0].time_ms.is_none());
    assert!(import_xml("<kml><LineString><coordinates>NaN,33</coordinates></LineString></kml>","bad.kml").is_err());
    assert!(import_xml("<kml><Point><coordinates>1,2</coordinates></Point></kml>","point.kml").is_err());
}
