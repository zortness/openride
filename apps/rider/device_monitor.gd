extends "res://screen.gd"

var source: Label
var state: Label
var metrics: Label
var commands: Label
var picker: FileDialog
var path := ""
var offset := 0
var countdown := 0.0
var samples := 0
var last_change := 0

func _ready() -> void:
	title = "OpenRide / Device capture monitor"
	size = Vector2i(720, 360)
	close_requested.connect(hide)
	var margin := MarginContainer.new()
	margin.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	for side in ["left", "top", "right", "bottom"]:
		margin.add_theme_constant_override("margin_" + side, 20)
	add_child(margin)
	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", 16)
	margin.add_child(column)
	var open := Button.new()
	open.text = "Open device capture or replay (.jsonl)"
	column.add_child(open)
	source = Label.new()
	source.text = "Capture viewer — values are recorded observations, not connection status."
	source.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	column.add_child(source)
	state = Label.new()
	state.text = "No capture selected"
	column.add_child(state)
	metrics = Label.new()
	metrics.text = "Power — W   Cadence — rpm   Speed — km/h   HR — bpm"
	column.add_child(metrics)
	commands = Label.new()
	commands.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	commands.text = "No command result recorded"
	column.add_child(commands)
	picker = FileDialog.new(); picker.display_mode = FileDialog.DISPLAY_LIST
	picker.access = FileDialog.ACCESS_FILESYSTEM
	picker.file_mode = FileDialog.FILE_MODE_OPEN_FILE
	picker.filters = PackedStringArray(["*.jsonl ; Device capture / replay"])
	add_child(picker)
	open.pressed.connect(func(): picker.popup_centered_ratio(0.8))
	picker.file_selected.connect(open_capture)

func open_capture(selected: String) -> void:
	path = selected
	offset = 0
	samples = 0
	last_change = 0
	source.text = selected + "\nTailing recorded data; this window does not control the trainer."
	commands.text = "No command result recorded"
	metrics.text = "No decoded telemetry yet"
	read_updates()

func _process(delta: float) -> void:
	if not visible or path.is_empty(): return
	countdown -= delta
	if countdown <= 0:
		countdown = 0.5
		read_updates()

func read_updates() -> void:
	var file := FileAccess.open(path, FileAccess.READ)
	if file == null:
		state.text = "Cannot read capture: " + str(FileAccess.get_open_error())
		return
	if file.get_length() < offset:
		state.text = "Capture was truncated; reopen it to reset the view"
		return
	file.seek(offset)
	# Bound work per frame and wait for complete lines from a running probe.
	for _i in range(256):
		if file.eof_reached() or file.get_position() >= file.get_length(): break
		var start := file.get_position()
		var raw := PackedByteArray()
		var complete := false
		while file.get_position() < file.get_length() and raw.size() < 65536:
			var byte := file.get_8()
			if byte == 10:
				complete = true
				break
			raw.append(byte)
		if not complete:
			if raw.size() >= 65536:
				state.text = "Capture line exceeds 64 KiB"
				path = ""
				return
			file.seek(start)
			break
		offset = file.get_position()
		var row = JSON.parse_string(raw.get_string_from_utf8().trim_prefix("\ufeff"))
		if row is Dictionary:
			ingest(row)
			last_change = Time.get_ticks_msec()
	state.text = "%d decoded observations • last file update observed %s" % [samples,
		"—" if last_change == 0 else "%.1f s ago" % ((Time.get_ticks_msec()-last_change)/1000.0)]

func number(data: Dictionary, key: String) -> String:
	return "—" if data.get(key) == null else str(data[key])

func ingest(row: Dictionary) -> void:
	var decoded = row.get("decoded")
	if decoded is Dictionary:
		if decoded.has("Err"):
			commands.text = "Telemetry decode error: " + str(decoded.Err)
		elif decoded.has("Ok"):
			decoded = decoded.Ok
		if decoded.has("power_w"):
			samples += 1
			metrics.text = "Power %s W   Cadence %s rpm\nSpeed %s km/h   HR %s bpm" % [number(decoded,"power_w"), number(decoded,"cadence_rpm"), number(decoded,"speed_kmh"), number(decoded,"heart_rate_bpm")]
	match row.get("event", ""):
		"control_request": commands.text = "Control requested; awaiting write completion and FTMS acknowledgement"
		"control_write_complete": commands.text = "GATT write completed: %s; trainer acknowledgement is separate" % row.get("success")
		"control_indication": commands.text = "FTMS response bytes: " + str(row.get("bytes"))
		"control_acquired": commands.text = "Control acquisition acknowledged. No start or resistance command sent."
		"device_closed": commands.text += "\nDevice handle closed: " + str(row.get("success"))
		"probe_error": commands.text = "Probe failed: " + str(row.get("message"))
		"probe_complete": commands.text += "\nProbe finished."
		"command_requested": commands.text = "Requested FTMS opcode %s; awaiting acknowledgement" % row.get("opcode")
		"command_write_complete": commands.text = "GATT write for opcode %s completed: %s; awaiting trainer acknowledgement" % [row.get("opcode"), row.get("success")]
		"command_acknowledged": commands.text = "Trainer acknowledged FTMS opcode %s" % row.get("opcode")
		"command_rejected": commands.text = "Trainer rejected opcode %s: result %s" % [row.get("opcode"),row.get("result")]
		"pause_reason": commands.text = "Pause requested because: " + str(row.get("reason"))
		"session_check_complete": commands.text = "Session check complete; Pause acknowledged. " + str(row.get("reason"))
		"session_check_failed": commands.text = "Session check failed: " + str(row.get("message")) + "\nTrainer state unconfirmed; connection closing."
