"""Offline structural/semantic validation of a virtual TCX/GPX export bundle.

Uses Python's independent XML parser. This is not XSD or external-app validation.
"""
import bisect
import csv
import hashlib
import json
import math
import sys
import xml.etree.ElementTree as ET
from pathlib import Path


def verify(folder):
    report = json.loads((folder / "report.json").read_text())
    ns = {
        "t": "http://www.garmin.com/xmlschemas/TrainingCenterDatabase/v2",
        "ae": "http://www.garmin.com/xmlschemas/ActivityExtension/v2",
        "g": "http://www.topografix.com/GPX/1/1",
        "or": "https://openride.local/xmlschemas/activity/v1",
    }
    with (folder / "virtual.csv").open(newline="") as stream:
        samples = [r for r in csv.DictReader(stream)
                   if r["timer_active"] == "true" and r["simulation_state"] == "running"]
    tcx = ET.parse(folder / "activity.tcx").getroot()
    assert tcx.tag == "{" + ns["t"] + "}TrainingCenterDatabase"
    lap = tcx.find("t:Activities/t:Activity/t:Lap", ns)
    assert lap is not None
    assert float(lap.findtext("t:TotalTimeSeconds", namespaces=ns)) == report["timer_ms"] / 1000
    assert math.isclose(float(lap.findtext("t:DistanceMeters", namespaces=ns)),
                        report["virtual_distance_m"], abs_tol=1e-6)
    points = lap.findall("t:Track/t:Trackpoint", ns)
    assert len(points) == len(samples) == report["tcx_records"]
    for p, s in zip(points, samples):
        assert p.findtext("t:Time", namespaces=ns) == s["timestamp_utc"]
        assert math.isclose(float(p.findtext("t:DistanceMeters", namespaces=ns)),
                            float(s["virtual_distance_m"]), abs_tol=1e-6)
        assert math.isclose(float(p.findtext("t:Extensions/ae:TPX/ae:Speed", namespaces=ns)),
                            float(s["virtual_speed_kmh"]) / 3.6, abs_tol=1e-6)
        watts = p.findtext("t:Extensions/ae:TPX/ae:Watts", namespaces=ns)
        assert watts == (str(int(s["power_w"])) if s["power_w"] else None)
    if report["geographic_source"] is None:
        assert not (folder / "activity.gpx").exists()
        assert all(p.find("t:Position", ns) is None for p in points)
    else:
        profile_bytes = (folder / "route-profile.json").read_bytes()
        manifest_bytes = (folder / "package-manifest.json").read_bytes()
        config = report["route_config"]
        assert hashlib.sha256(profile_bytes).hexdigest() == config["route_sha256"]
        assert hashlib.sha256(manifest_bytes).hexdigest() == config["package_manifest_sha256"]
        route = json.loads(profile_bytes)["points"]
        origin = json.loads(manifest_bytes)["origin_lat_lon_elevation"]
        stations = [p["distance_m"] for p in route]
        gpx = ET.parse(folder / "activity.gpx").getroot()
        assert gpx.tag == "{" + ns["g"] + "}gpx" and gpx.attrib["version"] == "1.1"
        track = gpx.findall("g:trk/g:trkseg/g:trkpt", ns)
        assert len(track) == len(samples) == report["gpx_records"]
        assert len(gpx.findall("g:trk/g:trkseg", ns)) == len(lap.findall("t:Track", ns))
        for p, t, s in zip(track, points, samples):
            assert p.findtext("g:time", namespaces=ns) == s["timestamp_utc"]
            d = float(s["virtual_distance_m"])
            i = max(1, bisect.bisect_left(stations, d))
            a, b = route[i - 1], route[i]
            ratio = (d - stations[i - 1]) / (stations[i] - stations[i - 1])
            x, _, z = [v + ratio * (w - v) for v, w in zip(a["position"], b["position"])]
            expected = [origin[0] - math.degrees(z / 6371000),
                        origin[1] + math.degrees(x / (6371000 * math.cos(math.radians(origin[0])))),
                        a["elevation_m"] + ratio * (b["elevation_m"] - a["elevation_m"])]
            actual = [float(p.attrib["lat"]), float(p.attrib["lon"]),
                      float(p.findtext("g:ele", namespaces=ns))]
            assert all(abs(v - w) < 1e-6 for v, w in zip(expected, actual))
            assert p.attrib["lat"] == t.findtext("t:Position/t:LatitudeDegrees", namespaces=ns)
            assert p.attrib["lon"] == t.findtext("t:Position/t:LongitudeDegrees", namespaces=ns)
    print(f"ACTIVITY_XML_OK: {len(points)} TCX points, {report['gpx_records']} GPX points; "
          f"{report['timer_ms'] / 1000:.3f} s timer, {report['virtual_distance_m']:.3f} m")


if __name__ == "__main__":
    verify(Path(sys.argv[1]))
