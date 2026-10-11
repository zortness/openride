extends RefCounted
var failed := false

func check(condition: bool, message: String) -> void:
	if not condition:
		failed = true
		push_error(message)

func settle(tree: SceneTree, screen: Control) -> void:
	var deadline := Time.get_ticks_msec() + 10000
	while screen.pending and Time.get_ticks_msec() < deadline: await tree.process_frame
	await tree.process_frame
	check(not screen.pending, "Discovery timed out")

func run(tree: SceneTree) -> void:
	var fixture := ""
	var capture := ""
	for argument in OS.get_cmdline_user_args():
		if argument.begins_with("--worker="): fixture = argument.trim_prefix("--worker=")
		if argument.begins_with("--capture="): capture = argument.trim_prefix("--capture=")
	check(not fixture.is_empty() and not capture.is_empty(), "Provide --worker and --capture")
	if failed: tree.quit(1); return
	var screen = load("res://device_discovery.gd").new()
	tree.root.add_child(screen)
	screen.start("", fixture, capture)
	check(screen.pending and screen.scan.disabled, "Discovery did not enter searching state")
	var competitor: RefCounted = ClassDB.instantiate("TrainerSession")
	check(not competitor.start_session(fixture, capture + "-competing"), "Ride allowed while diagnostic owns worker")
	await settle(tree, screen)
	check(screen.completed and not screen.operation_failed, "Enumeration failed")
	check(screen.devices.item_count == 2, "Duplicate names were collapsed")
	check(screen.devices.get_item_metadata(1) == "synthetic-two", "Device identity lost")
	check(not screen.review.disabled, "Report review unavailable")
	check(FileAccess.file_exists(capture.path_join("ble.jsonl")), "Diagnostic report missing")
	screen.start(str(screen.devices.get_item_metadata(1)), fixture, capture + "-inspect")
	await settle(tree, screen)
	check(screen.completed and not screen.operation_failed, "Selected identity not passed to worker")
	check(screen.details.text.contains("synthetic-two") and screen.details.text.contains("terrain_control"), "Capabilities missing from review")
	check(screen.status.text.contains("unverified"), "Discovery presented as physical verification")
	check(not screen.save_roles.disabled and not screen.use_cadence.disabled, "Supported trainer roles cannot be assigned")
	screen.use_cadence.button_pressed = false
	screen.save_selection()
	var saved: Dictionary = JSON.parse_string(screen.bridge.pairing_selection())
	check(saved.get("selection", {}).get("resistance", {}).get("id") == "synthetic-two", "Pairing saved by name instead of identity")
	check(saved.get("selection", {}).get("cadence") == null, "Cadence opt-out not persisted")
	var reopened: RefCounted = ClassDB.instantiate("TrainerSession")
	reopened.configure_pairing_path(ProjectSettings.globalize_path("user://pairing.json"))
	check(JSON.parse_string(reopened.pairing_selection()) == saved, "Fresh bridge did not reload saved pairing")
	var invalid: Dictionary = saved.selection.duplicate(true)
	invalid.power.id = "synthetic-one"
	check(not reopened.save_pairing(JSON.stringify(invalid)).is_empty(), "Separate unsupported power source accepted")
	check(JSON.parse_string(reopened.pairing_selection()) == saved, "Rejected selection replaced saved pairing")
	screen.devices.item_selected.emit(0)
	check(screen.save_roles.disabled and screen.candidate.is_empty(), "Changing device left stale inspected roles assignable")
	var report := FileAccess.get_file_as_string((capture + "-inspect").path_join("ble.jsonl"))
	check(report.contains("synthetic-two") and not FileAccess.file_exists((capture + "-inspect").path_join("session.jsonl")), "Diagnostic mixed with session recording")
	screen.start("synthetic-sensor", fixture, capture + "-sensor")
	await settle(tree, screen)
	for role in ["power", "cadence", "heart_rate"]:
		check(not screen.sensor_buttons[role].disabled, "Sensor role not offered: " + role)
		screen.assign_sensor(role)
	var sensor_selection: Dictionary = JSON.parse_string(screen.bridge.pairing_selection()).selection
	check(sensor_selection.resistance.id == "synthetic-two", "Sensor assignment changed resistance owner")
	check(sensor_selection.power.protocol == "cycling_power" and sensor_selection.cadence.protocol == "csc" and sensor_selection.heart_rate.protocol == "heart_rate", "Sensor protocols not persisted independently")
	check(sensor_selection.power.id == "synthetic-sensor" and sensor_selection.heart_rate.id == "synthetic-sensor", "Sensor identity lost")
	reopened.configure_heart_rate('{"device_number":42}')
	check(reopened.start_session(fixture, capture + "-hr-conflict"), "HR conflict validation not dispatched")
	var conflict_deadline := Time.get_ticks_msec() + 10000
	while reopened.is_active() and Time.get_ticks_msec() < conflict_deadline: await tree.process_frame
	check(not DirAccess.dir_exists_absolute(capture + "-hr-conflict"), "Conflicting HR sources launched a worker")
	var conflict_rows: Array = JSON.parse_string(reopened.poll_session())
	check(conflict_rows.any(func(row): return row.get("message", "").contains("OR ANT")), "HR conflict not explained")
	screen.assign_sensor("heart_rate", true)
	check(JSON.parse_string(screen.bridge.pairing_selection()).selection.heart_rate == null, "Bluetooth HR clear did not persist")
	screen.start("failure", fixture, capture + "-failure")
	await settle(tree, screen)
	check(screen.operation_failed and screen.status.text.contains("failed"), "Worker failure not displayed")
	check(FileAccess.get_file_as_string((capture + "-failure").path_join("ble-errors.txt")).contains("Synthetic discovery failure"), "Worker stderr not preserved")
	screen.start("", fixture + ".missing", capture + "-missing")
	await settle(tree, screen)
	check(screen.operation_failed and not screen.scan.disabled, "Launch failure did not recover UI")
	# A missing or corrupt pairing must fail before launching even a fake worker.
	var blocked: RefCounted = ClassDB.instantiate("TrainerSession")
	var bad_preferences := capture + "-pairing.json"
	blocked.configure_pairing_path(bad_preferences)
	for suffix in ["-no-pairing", "-corrupt-pairing"]:
		if suffix == "-corrupt-pairing":
			var file := FileAccess.open(bad_preferences, FileAccess.WRITE)
			file.store_string("{broken"); file.close()
		check(blocked.start_session(fixture, capture + suffix), "Pairing validation not dispatched")
		var deadline := Time.get_ticks_msec() + 10000
		while blocked.is_active() and Time.get_ticks_msec() < deadline: await tree.process_frame
		check(not blocked.is_active(), "Pairing validation hung")
		var rows: Array = JSON.parse_string(blocked.poll_session())
		check(rows.any(func(row): return row.get("event") == "worker_error"), "Invalid pairing did not report an error")
		check(not DirAccess.dir_exists_absolute(capture + suffix), "Invalid pairing launched or created a session")
	if not failed: print("DEVICE_DISCOVERY_OK")
	tree.quit(1 if failed else 0)
