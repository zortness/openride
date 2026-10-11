extends Window

var bridge: RefCounted
var status: Label
var metrics: Label
var grade: Label
var recording: Label
var freshness: Label
var start: Button
var pause: Button
var resume: Button
var finish: Button
var last_sample := 0
var completed := false
var pending := ""
var phase := ""
var resume_epoch := 0
var can_resume := false
var faulted := false
var fault_message := ""

func _ready() -> void:
	title = "OpenRide / Trainer session"
	size = Vector2i(760, 440)
	close_requested.connect(func():
		request_finish()
		hide())
	bridge = ClassDB.instantiate("TrainerSession")
	var margin := MarginContainer.new()
	margin.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	for side in ["left", "right", "top", "bottom"]:
		margin.add_theme_constant_override("margin_" + side, 24)
	add_child(margin)
	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", 16)
	margin.add_child(column)
	var intro := Label.new()
	intro.text = "SUITO / Supervised session\nStart or Resume: 0%, 1%, 2%, 1%, 0%, then hold flat. Finish within 40 minutes."
	intro.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	column.add_child(intro)
	var buttons := HBoxContainer.new()
	column.add_child(buttons)
	start = Button.new()
	start.text = "Start session"
	start.pressed.connect(begin)
	buttons.add_child(start)
	pause = Button.new()
	pause.text = "Pause"
	pause.disabled = true
	pause.pressed.connect(request_pause)
	buttons.add_child(pause)
	resume = Button.new()
	resume.text = "Resume"
	resume.disabled = true
	resume.pressed.connect(request_resume)
	buttons.add_child(resume)
	finish = Button.new()
	finish.text = "Finish"
	finish.disabled = true
	finish.pressed.connect(request_finish)
	buttons.add_child(finish)
	status = Label.new()
	status.text = "Disconnected. Close other trainer applications before starting."
	status.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	column.add_child(status)
	metrics = Label.new()
	metrics.text = "No telemetry yet"
	column.add_child(metrics)
	freshness = Label.new()
	column.add_child(freshness)
	grade = Label.new()
	grade.text = "No grade acknowledged"
	column.add_child(grade)
	recording = Label.new()
	recording.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	column.add_child(recording)

func begin() -> void:
	if bridge.is_active(): return
	var executable: String
	if OS.has_feature("editor"):
		executable = ProjectSettings.globalize_path("res://bin/device-probe.exe")
	else:
		executable = OS.get_executable_path().get_base_dir().path_join("device-probe.exe")
	var directory := ProjectSettings.globalize_path("user://sessions").path_join("session-%s-%s" % [str(Time.get_unix_time_from_system()).replace(".", "-"), str(Time.get_ticks_usec())])
	completed = false
	pending = ""
	phase = "connecting"
	can_resume = false
	faulted = false
	fault_message = ""
	last_sample = 0
	metrics.text = "Waiting for trainer measurements"
	grade.text = "No grade acknowledged"
	status.text = "Connecting to SUITO..."
	recording.text = "Recording pending: " + directory
	if bridge.start_session(executable, directory):
		start.disabled = true
		pause.disabled = false

func request_pause() -> void:
	if bridge != null and bridge.is_active():
		bridge.pause_session()
		pending = "pause"
		pause.disabled = true
		status.text = "Pause requested; waiting for controller acknowledgement..."

func request_resume() -> void:
	if bridge != null and bridge.is_active() and can_resume and pending.is_empty() and not faulted:
		bridge.resume_session(resume_epoch)
		pending = "resume"
		status.text = "Resume requested; waiting for fresh telemetry and Start acknowledgement..."

func request_finish() -> void:
	if bridge != null and bridge.is_active():
		bridge.finish_session()
		pending = "finish"
		status.text = "Finishing; waiting for flat grade, Pause, and disconnect..."

func _process(_delta: float) -> void:
	if bridge == null: return
	var rows = JSON.parse_string(bridge.poll_session())
	if rows is Array:
		for row in rows:
			if row is Dictionary: ingest(row)
	start.disabled = bridge.is_active()
	pause.disabled = not bridge.is_active() or not pending.is_empty() or phase != "running" or faulted
	resume.disabled = not bridge.is_active() or not pending.is_empty() or not can_resume or faulted
	finish.disabled = not bridge.is_active() or pending == "finish" or completed or phase == "finished"
	if last_sample > 0:
		var age := (Time.get_ticks_msec() - last_sample) / 1000.0
		freshness.text = "Last displayed sample %.1f s ago%s" % [age, " / updates stale" if age > 3.0 else ""]

func value_text(data: Dictionary, key: String) -> String:
	return str(data[key]) if data.get(key) != null else "—"

func ingest(row: Dictionary) -> void:
	match row.get("event", ""):
		"session_state":
			phase = str(row.get("state", ""))
			resume_epoch = int(row.get("resume_epoch", 0))
			can_resume = row.get("can_resume", false)
			if (pending == "pause" and phase == "paused") or (pending == "resume" and phase == "running"):
				pending = ""
			if not faulted and pending.is_empty():
				if phase == "running": status.text = "Running / Start acknowledged"
				elif phase == "paused": status.text = "Paused / flat and Pause acknowledged / " + ("Resume available" if can_resume else "waiting for fresh power telemetry")
				elif phase == "finished": status.text = "Finished / observing trainer before disconnect..."
		"resume_rejected":
			pending = ""
			status.text = "Resume rejected: " + str(row.get("reason", ""))
		"session_check_created":
			recording.text = recording.text.replace("Recording pending:", "Recording:")
		"indoor_bike_data":
			var data = row.get("decoded", {}).get("Ok")
			if data is Dictionary:
				last_sample = Time.get_ticks_msec()
				metrics.text = "%s W   %s rpm   %s km/h   %s bpm" % [value_text(data, "power_w"), value_text(data, "cadence_rpm"), value_text(data, "speed_kmh"), value_text(data, "heart_rate_bpm")]
		"session_check_ready":
			status.text = "Connected; acquiring trainer control..."
		"command_requested":
			if row.get("opcode") == 17:
				var bytes = row.get("bytes", [])
				if bytes.size() == 7:
					grade.text = "Requested grade %.1f%%; awaiting acknowledgement" % ((int(bytes[3]) + int(bytes[4]) * 256) / 100.0)
		"command_acknowledged":
			match int(row.get("opcode", -1)):
				0:
					if not faulted: status.text = "Control acquired; preparing session..."
				7:
					if not faulted: status.text = "Running / Start acknowledged"
				8:
					if not faulted: status.text = "Pause acknowledged; waiting for session state..."
				17:
					var bytes = row.get("bytes", [])
					if bytes.size() == 7:
						grade.text = "Acknowledged grade %.1f%%" % ((int(bytes[3]) + int(bytes[4]) * 256) / 100.0)
		"session_check_complete":
			completed = true
			phase = "finished"
			can_resume = false
			recording.text = recording.text.replace("Recording:", "Recorded:")
			status.text = "Finished / Pause acknowledged / " + str(row.get("reason"))
			if faulted: status.text += " / Earlier error: " + fault_message
		"session_check_failed", "probe_error", "worker_error", "capture_error", "display_overflow":
			faulted = true
			fault_message = str(row.get("message", "Controller failed"))
			status.text = "Trainer state unconfirmed: " + str(row.get("message", "Controller failed"))
			request_finish()
			# Preserve the error rather than replacing it with the pending request.
			status.text = "Trainer state unconfirmed: " + str(row.get("message", "Controller failed"))
		"worker_exited":
			if not completed or not row.get("success", false):
				status.text = "Controller exited; trainer state unconfirmed. Inspect recording."
		"device_closed":
			if completed: status.text += " / Disconnected"
