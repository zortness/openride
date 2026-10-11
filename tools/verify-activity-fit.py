"""Check an export using the independent MIT fitdecode package (dev tool only).

Install into local/fit-validation/vendor, or use a Python environment with fitdecode.
Usage: python tools/verify-activity-fit.py EXPORT_DIRECTORY
No network access, uploads, or source changes are performed by this script.
"""
import csv
import datetime as dt
import json
import math
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "local/fit-validation/vendor"))
try:
    import fitdecode
except ImportError:
    sys.exit("Install fitdecode first; see docs/activity-exports.md. No independent validation performed.")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def main(folder):
    report = json.loads((folder / "report.json").read_text(encoding="utf-8"))
    messages = {}
    with fitdecode.FitReader(str(folder / "activity.fit"), check_crc=fitdecode.CrcCheck.RAISE,
                             error_handling=fitdecode.ErrorHandling.RAISE) as reader:
        for frame in reader:
            if isinstance(frame, fitdecode.FitDataMessage):
                messages.setdefault(frame.name, []).append({f.name: f.value for f in frame.fields})
    for name in ("file_id", "session", "lap", "activity"):
        require(len(messages.get(name, [])) == 1, f"Expected one {name}")
    session = messages["session"][0]
    require(messages["file_id"][0]["type"] == "activity", "Not an activity file")
    virtual = report.get("route_config") is not None
    require(session["sport"] == "cycling" and session["sub_sport"] == ("virtual_activity" if virtual else "indoor_cycling"), "Wrong sport")
    for summary in (session, messages["lap"][0]):
        require(math.isclose(summary["total_elapsed_time"], report["elapsed_ms"] / 1000, abs_tol=.001), "Elapsed mismatch")
        require(math.isclose(summary["total_timer_time"], report["timer_ms"] / 1000, abs_tol=.001), "Timer mismatch")
    require(math.isclose(messages["activity"][0]["total_timer_time"], report["timer_ms"] / 1000, abs_tol=.001), "Activity timer mismatch")
    records = messages.get("record", [])
    require(len(records) == report["fit_records"], "Record count mismatch")
    with (folder / "activity.csv").open(newline="", encoding="utf-8") as file:
        samples = list(csv.DictReader(file))
    require(len(samples) == report["csv_samples"], "CSV count mismatch")
    if virtual:
        require(math.isclose(session["total_distance"], report["virtual_distance_m"], abs_tol=.0051), "Virtual total distance mismatch")
        require(math.isclose(messages["lap"][0]["total_distance"], report["virtual_distance_m"], abs_tol=.0051), "Lap distance mismatch")
        with (folder / "virtual.csv").open(newline="", encoding="utf-8") as file:
            samples = list(csv.DictReader(file))
        require(len(samples) == report["virtual_samples"], "Virtual snapshot count mismatch")
    selected = {}
    for sample in samples:
        if sample["timer_active"] == "true" and (not virtual or sample["simulation_state"] == "running"):
            instant = dt.datetime.fromisoformat(sample["timestamp_utc"].replace("Z", "+00:00"))
            selected[int(instant.timestamp())] = sample
    last = None
    for record in records:
        instant = record["timestamp"]
        if instant.tzinfo is None:
            instant = instant.replace(tzinfo=dt.timezone.utc)
        stamp = int(instant.timestamp())
        require(last is None or stamp > last, "Non-increasing record timestamps")
        last = stamp
        sample = selected[stamp]
        expected_power = int(sample["power_w"]) if sample["power_w"] else None
        if expected_power is not None and expected_power < 0:
            expected_power = None
        require(record.get("power") == expected_power, "Power mismatch")
        expected_hr = int(sample["heart_rate_bpm"]) if sample["heart_rate_bpm"] else None
        if expected_hr == 255:
            expected_hr = None
        require(record.get("heart_rate") == expected_hr, "Missing/zero HR mismatch")
        if sample["cadence_rpm"] and float(sample["cadence_rpm"]) < 255:
            cadence = record["cadence"] + (record.get("fractional_cadence") or 0)
            require(math.isclose(cadence, float(sample["cadence_rpm"]), abs_tol=1/128), "Cadence mismatch")
        speed_key = "virtual_speed_kmh" if virtual else "trainer_speed_kmh"
        if sample[speed_key]:
            speed = record.get("enhanced_speed", record.get("speed"))
            require(math.isclose(speed, float(sample[speed_key]) / 3.6, abs_tol=.001), "Speed unit mismatch")
        if virtual:
            require(math.isclose(record["distance"], float(sample["virtual_distance_m"]), abs_tol=.0051), "Virtual record distance mismatch")
    events = [m for m in messages.get("event", []) if m.get("event") == "timer"]
    require(sum(e["event_type"] == "start" for e in events) == len(report["running_intervals"]), "Timer start count mismatch")
    require(sum(e["event_type"] in ("stop", "stop_all") for e in events) >= len(report["running_intervals"]), "Missing timer stops")
    print(json.dumps({"result": "FITDECODE_OK", "fit_records": len(records), "csv_samples": len(samples),
                      "elapsed_s": session["total_elapsed_time"], "timer_s": session["total_timer_time"]}))


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit("Usage: verify-activity-fit.py EXPORT_DIRECTORY")
    main(Path(sys.argv[1]))
