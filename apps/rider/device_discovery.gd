extends ScrollContainer

var bridge: RefCounted
var devices: ItemList
var status: Label
var details: TextEdit
var scan: Button
var inspect: Button
var review: Button
var report_directory := ""
var pending := false
var completed := false
var operation_failed := false
var candidate := {}
var candidate_name := ""
var save_roles: Button
var use_cadence: CheckBox
var saved_roles: Label
var sensor_buttons := {}

func _ready() -> void:
	follow_focus = true
	var column := VBoxContainer.new(); column.size_flags_horizontal = Control.SIZE_EXPAND_FILL; add_child(column)
	bridge = ClassDB.instantiate("TrainerSession")
	bridge.configure_pairing_path(ProjectSettings.globalize_path("user://pairing.json"))
	var intro := Label.new()
	intro.text = "Device discovery\nPair devices in Windows Bluetooth settings first, then refresh. Inspection reads capabilities only."
	intro.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	column.add_child(intro)
	var actions := HBoxContainer.new(); column.add_child(actions)
	scan = Button.new(); scan.text = "Refresh paired devices"; scan.pressed.connect(func(): start("")); actions.add_child(scan)
	inspect = Button.new(); inspect.text = "Inspect selected device"; inspect.disabled = true; inspect.pressed.connect(inspect_selected); actions.add_child(inspect)
	review = Button.new(); review.text = "Review diagnostic files"; review.disabled = true; review.pressed.connect(func(): OS.shell_open(report_directory)); actions.add_child(review)
	devices = ItemList.new(); devices.custom_minimum_size.y = 120; column.add_child(devices)
	devices.item_selected.connect(func(_index: int):
		inspect.disabled = pending
		candidate = {}; save_roles.disabled = true; use_cadence.disabled = true)
	status = Label.new(); status.text = "Ready. Discovery does not verify telemetry or resistance."; status.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART; column.add_child(status)
	var pairing_row := HBoxContainer.new(); column.add_child(pairing_row)
	save_roles = Button.new(); save_roles.text = "Use inspected trainer for resistance + power"; save_roles.disabled = true; save_roles.pressed.connect(save_selection); pairing_row.add_child(save_roles)
	use_cadence = CheckBox.new(); use_cadence.text = "Use its cadence"; use_cadence.disabled = true; pairing_row.add_child(use_cadence)
	var sensor_row := HFlowContainer.new(); column.add_child(sensor_row)
	for role in ["power", "cadence", "heart_rate"]:
		var button := Button.new(); button.text = "Use for " + role.replace("_", " "); button.disabled = true
		button.pressed.connect(func(): assign_sensor(role)); sensor_row.add_child(button); sensor_buttons[role] = button
	for role in ["cadence", "heart_rate"]:
		var button := Button.new(); button.text = "Clear " + role.replace("_", " ")
		button.pressed.connect(func(): assign_sensor(role, true)); sensor_row.add_child(button)
	saved_roles = Label.new(); saved_roles.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART; column.add_child(saved_roles); refresh_saved()
	details = TextEdit.new(); details.editable = false; details.custom_minimum_size.y = 140; column.add_child(details)
	var privacy := Label.new()
	privacy.text = "Reports contain device names, Windows device IDs and capability/device-information fields. Review before sharing. Routes, activity history, network keys and credentials are excluded. Nothing is uploaded."
	privacy.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART; column.add_child(privacy)

func refresh_saved() -> void:
	var saved: Dictionary = JSON.parse_string(bridge.pairing_selection())
	if saved.has("error"):
		saved_roles.text = "No usable trainer pairing. Inspect a trainer, then save its roles."
	else:
		var selection: Dictionary = saved.selection
		saved_roles.text = "Resistance: %s | Power: %s\nCadence: %s | Bluetooth HR: %s" % [selection.resistance.name, selection.power.name, "Off" if selection.cadence == null else selection.cadence.name, "Off" if selection.get("heart_rate") == null else selection.heart_rate.name]
	saved_roles.text += "\nUse one HR source: saved Bluetooth HR or ANT in ride setup."

func assign_sensor(role: String, clear := false) -> void:
	if pending: return
	var saved: Dictionary = JSON.parse_string(bridge.pairing_selection())
	if saved.has("error"): status.text = "Select a resistance trainer first."; return
	var selection: Dictionary = saved.selection
	if not clear:
		var protocol: Variant = candidate.get("sensor_roles", {}).get(role)
		if candidate.is_empty() or protocol == null or not completed or operation_failed: return
		selection[role] = {"id":str(candidate.id), "name":candidate_name, "protocol":protocol}
	else: selection[role] = null
	selection.schema = 2
	var error: String = bridge.save_pairing(JSON.stringify(selection))
	if not error.is_empty(): status.text = error; return
	refresh_saved(); status.text = "Source saved. Sensor loss stays missing; trainer readings will not replace it."

func save_selection() -> void:
	if pending or save_roles.disabled: return
	var device := {"id":str(candidate.id), "name":candidate_name}
	var previous: Dictionary = JSON.parse_string(bridge.pairing_selection()).get("selection", {})
	var selection := {"schema":2, "resistance":device, "power":device, "cadence":device if use_cadence.button_pressed else null, "heart_rate":previous.get("heart_rate")}
	var error: String = bridge.save_pairing(JSON.stringify(selection))
	if not error.is_empty(): status.text = error; return
	refresh_saved()
	status.text = "Pairing saved. Connect from ride setup when ready; capabilities will be checked again."

func inspect_selected() -> void:
	var selected := devices.get_selected_items()
	if not selected.is_empty(): start(str(devices.get_item_metadata(selected[0])))

func start(identity: String, executable_override := "", directory_override := "") -> void:
	if pending: return
	var executable := executable_override
	if executable.is_empty():
		executable = ProjectSettings.globalize_path("res://bin/device-probe.exe") if OS.has_feature("editor") else OS.get_executable_path().get_base_dir().path_join("device-probe.exe")
	var directory := directory_override
	if directory.is_empty():
		directory = ProjectSettings.globalize_path("user://diagnostics").path_join("device-%s-%s" % [str(Time.get_unix_time_from_system()).replace(".", "-"), str(Time.get_ticks_usec())])
	if not bridge.start_discovery(executable, directory, identity):
		status.text = "A device worker is already active. Finish the ride or diagnostic before trying again."
		return
	report_directory = directory
	pending = true; completed = false; operation_failed = false
	candidate = {}; candidate_name = ""; save_roles.disabled = true; use_cadence.disabled = true; use_cadence.button_pressed = false
	if identity.is_empty(): devices.clear()
	details.text = ""
	status.text = "Searching Windows-paired BLE devices..." if identity.is_empty() else "Reading selected device capabilities..."
	scan.disabled = true; inspect.disabled = true; review.disabled = true

func _process(_delta: float) -> void:
	if bridge == null: return
	for role in sensor_buttons:
		sensor_buttons[role].disabled = pending or not completed or operation_failed or candidate.get("sensor_roles", {}).get(role) == null
	for row in JSON.parse_string(bridge.poll_session()): ingest(row)
	if pending and not bridge.is_active():
		pending = false
		scan.disabled = false
		inspect.disabled = devices.get_selected_items().is_empty()
		review.disabled = not DirAccess.dir_exists_absolute(report_directory)
		var roles: Dictionary = candidate.get("live_roles", {})
		save_roles.disabled = not (completed and not operation_failed and roles.get("terrain_control", false) and roles.get("power", false))
		use_cadence.disabled = save_roles.disabled or not roles.get("cadence", false)
		use_cadence.button_pressed = not use_cadence.disabled
		if completed and not operation_failed and not candidate.is_empty() and save_roles.disabled:
			status.text = "Inspecting does not verify measurements. Assign an available sensor role below; resistance requires an FTMS trainer."
		if not completed and not operation_failed: status.text = "Diagnostic ended without a completion record. Review the files."

func ingest(row: Dictionary) -> void:
	match str(row.get("event", "")):
		"paired_device":
			var id := str(row.get("id", ""))
			if id.is_empty(): return
			var index := devices.add_item(str(row.get("name", "Unnamed device")))
			devices.set_item_metadata(index, id)
			devices.set_item_tooltip(index, id)
		"capabilities":
			candidate = row.duplicate(true)
			status.text = "Capabilities discovered. Telemetry and resistance remain unverified."
		"device": candidate_name = str(row.get("name", "Unnamed trainer"))
		"probe_complete": completed = true
		"worker_error", "probe_error", "capture_error", "display_overflow":
			operation_failed = true
			status.text = "Diagnostic failed: " + str(row.get("message", "Incomplete report; review files."))
		"worker_exited":
			if not row.get("success", false):
				operation_failed = true
				status.text = "Diagnostic failed. Review ble-errors.txt and the partial report."
			elif completed and not operation_failed:
				status.text = "Diagnostic complete. Capabilities are discovery evidence; telemetry and control remain unverified."
	# Display diagnostic rows and process status, with a bounded UI.
	if details.text.length() < 200000:
		details.text += JSON.stringify(row, "  ") + "\n"
