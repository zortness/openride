extends RefCounted
## UI adapter only. The controller owns simulation, hardware, and recording.
var bridge: RefCounted = ClassDB.instantiate("TrainerSession")
var state := {}
var phase := "disconnected"
var note := "Connect the selected trainer, then Start ride"
var epoch := 0
var can_resume := false
var started := false
var pending := ""
var completed := false
var route_complete := false
var faulted := false
var directory := ""
var worker_override := ""
var cadence: Variant = null
var cadence_deadline := 0
var ride_start_ms := -1
var elapsed_ms := 0
var hr_config := ""
var hr_bpm: Variant = null
var hr_status := "off"
var hr_deadline := 0
var ble_hr_unavailable := false

func _init() -> void:
	bridge.configure_pairing_path(ProjectSettings.globalize_path("user://pairing.json"))

func heart_rate_text() -> String:
	if hr_status=="off":return ""
	if hr_bpm!=null and Time.get_ticks_msec()<hr_deadline:return " | %d bpm" % int(hr_bpm)
	return " | HR -- ("+("unavailable" if hr_status=="unavailable" else "searching")+")"


func active() -> bool:
	return bridge.is_active()

func pairing_summary() -> String:
	var saved: Dictionary = JSON.parse_string(bridge.pairing_selection())
	if saved.has("error"): return "Select a trainer in Settings / Devices before connecting."
	var roles: Dictionary = saved.selection
	return "Resistance: %s | Power: %s\nCadence: %s | Bluetooth HR: %s" % [roles.resistance.name, roles.power.name, "Off" if roles.cadence == null else roles.cadence.name, "Off" if roles.get("heart_rate") == null else roles.heart_rate.name]

func connect_trainer(route_path:String,expected_sha256:String,package_sha256:String) -> void:
	if active():return
	var saved: Dictionary = JSON.parse_string(bridge.pairing_selection())
	if saved.has("error"): note = str(saved.error); return
	if saved.selection.get("heart_rate") != null and not hr_config.is_empty(): note = "Choose Bluetooth OR ANT heart rate, not both."; return
	var executable:String=ProjectSettings.globalize_path("res://bin/device-probe.exe") if OS.has_feature("editor") else OS.get_executable_path().get_base_dir().path_join("device-probe.exe")
	if not worker_override.is_empty():executable=worker_override
	directory=ProjectSettings.globalize_path("user://sessions").path_join("route-%s-%s" % [str(Time.get_unix_time_from_system()).replace(".","-"),str(Time.get_ticks_usec())])
	bridge.configure_heart_rate(hr_config)
	ble_hr_unavailable = false
	cadence=null;ride_start_ms=-1;elapsed_ms=0
	hr_bpm=null;hr_status="off" if hr_config.is_empty() and saved.selection.get("heart_rate") == null else "searching"
	if not bridge.start_route_session(executable,directory,route_path,expected_sha256,package_sha256):
		note="Another trainer session is active";return
	started=false;state={};phase="connecting";note="Connecting to %s..." % saved.selection.resistance.name;epoch=0;can_resume=false;pending="";completed=false;faulted=false;route_complete=false

func toggle() -> void:
	if not active() or not pending.is_empty() or faulted:return
	if phase=="running":
		bridge.pause_session();pending="pause";note="Waiting for flat grade and Pause acknowledgement"
	elif phase=="paused" and can_resume:
		bridge.resume_session(epoch);pending="resume";note="Waiting for Start acknowledgement"

func finish() -> void:
	if active():
		bridge.finish_session();pending="finish"
		if not faulted:note="Finishing; waiting for flat grade, Pause and disconnect"

func poll() -> void:
	for row in JSON.parse_string(bridge.poll_session()):
		elapsed_ms=int(row.get("elapsed_ms",elapsed_ms))
		match row.get("event",""):
			"ble_sensor_data":
				if row.get("protocol") == "heart_rate": ble_hr_unavailable = row.get("error") != null
			"selected_measurements":
				var values: Dictionary = row.get("decoded", {}).get("Ok", {})
				cadence = values.get("cadence_rpm")
				cadence_deadline = Time.get_ticks_msec() + maxi(0,3000-int(row.get("elapsed_ms",0))+int(values.get("cadence_received_ms",0) if values.get("cadence_received_ms") != null else 0))
				if hr_config.is_empty() and hr_status != "off":
					hr_bpm = values.get("heart_rate_bpm"); hr_status = "receiving" if hr_bpm != null else ("unavailable" if ble_hr_unavailable else "searching")
					hr_deadline = Time.get_ticks_msec() + maxi(0,3000-int(row.get("elapsed_ms",0))+int(values.get("hr_received_ms",0) if values.get("hr_received_ms") != null else 0))
			"indoor_bike_data":
				if row.get("selected_raw",false): continue
				cadence=row.get("decoded",{}).get("Ok",{}).get("cadence_rpm")
				cadence_deadline=Time.get_ticks_msec()+maxi(0,3000-int(row.get("elapsed_ms",0))+int(row.get("received_ms",0)))
			"ant_hr_data":
				hr_bpm=row.get("heart_rate_bpm");hr_status="receiving"
				hr_deadline=Time.get_ticks_msec()+maxi(0,3000-int(row.get("elapsed_ms",0))+int(row.get("received_ms",0)))
			"ant_hr_status":
				hr_status=row.get("status","searching")
				if hr_status!="receiving":hr_bpm=null
			"virtual_state":state=row
			"session_state":
				phase=row.state
				if phase=="running" and ride_start_ms<0:ride_start_ms=elapsed_ms
				epoch=int(row.resume_epoch);can_resume=row.can_resume
				started=row.get("start_acknowledged",started or phase=="running")
				if (pending=="pause" and phase=="paused") or (pending=="resume" and phase=="running"):pending=""
				if row.get("reason")=="route_complete" and phase=="finished":route_complete=true
				if pending.is_empty() and not faulted:
					if phase=="running":note="Riding / recording"
					elif not started:note="Ready to start / flat grade acknowledged" if can_resume else "Waiting for fresh power before Start"
					else:note="Paused / ready to resume" if can_resume else "Paused / waiting for fresh power"
			"resume_rejected":pending="";note="Resume rejected; wait for fresh power"
			"session_check_complete":completed=true;note="Saved: "+directory
			"session_check_failed","probe_error","worker_error","capture_error","display_overflow":
				faulted=true;note="Trainer state unconfirmed: "+str(row.get("message","Controller error"));finish()
			"worker_exited":
				if not completed or not row.get("success",false):faulted=true;note="Trainer state unconfirmed; inspect "+directory
