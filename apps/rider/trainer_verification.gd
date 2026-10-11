extends RefCounted
var failed := false

func check(condition: bool, message: String) -> void:
	if not condition:
		failed = true
		push_error(message)

func run(tree: SceneTree) -> void:
	var session = load("res://trainer_session.gd").new()
	tree.root.add_child(session)
	session.ingest({"event":"indoor_bike_data","decoded":{"Ok":{"power_w":0,"cadence_rpm":null,"speed_kmh":0,"heart_rate_bpm":null}}})
	check(session.metrics.text.contains("0 W") and session.metrics.text.contains("\u2014 rpm"), "Zero/missing telemetry lost")
	session.ingest({"event":"command_requested","opcode":17,"bytes":[17,0,0,200,0,40,51]})
	check(session.grade.text.contains("awaiting"), "Request mistaken for acknowledgement")
	session.ingest({"event":"command_acknowledged","opcode":17,"bytes":[17,0,0,200,0,40,51]})
	check(session.grade.text.contains("Acknowledged grade 2.0%"), "Acknowledged grade missing")
	session.ingest({"event":"worker_exited","success":false})
	check(session.status.text.contains("unconfirmed"), "Unconfirmed state lost")
	var fixture := ""
	var capture := ""
	for argument in OS.get_cmdline_user_args():
		if argument.begins_with("--worker="): fixture = argument.trim_prefix("--worker=")
		if argument.begins_with("--capture="): capture = argument.trim_prefix("--capture=")
	check(not fixture.is_empty() and not capture.is_empty(), "Provide --worker and --capture")
	if failed:
		tree.quit(1)
		return
	check(session.bridge.start_session(fixture, capture), "Worker failed to launch")
	check(not session.bridge.start_session(fixture, capture), "Second internal worker allowed")
	var deadline := Time.get_ticks_msec() + 10000
	while not session.status.text.contains("Running") and Time.get_ticks_msec() < deadline:
		await tree.process_frame
	check(session.status.text.contains("Running"), "Worker stdout did not reach UI")
	session.request_pause()
	while session.phase != "paused" and Time.get_ticks_msec() < deadline:
		await tree.process_frame
	check(session.bridge.is_active() and not session.completed, "Pause ended the persistent worker")
	check(session.can_resume, "Acknowledged Pause did not offer Resume")
	session.ingest({"event":"session_state","state":"paused","resume_epoch":1,"can_resume":false})
	session.request_resume()
	check(session.pending.is_empty(), "UI allowed Resume without fresh power")
	session.ingest({"event":"session_state","state":"paused","resume_epoch":1,"can_resume":true})
	session.request_resume()
	while session.phase != "running" and Time.get_ticks_msec() < deadline:
		await tree.process_frame
	check(session.phase == "running" and session.pending.is_empty(), "Resume did not acknowledge Running")
	session.request_finish()
	while session.bridge.is_active() and Time.get_ticks_msec() < deadline:
		await tree.process_frame
	await tree.process_frame
	check(session.completed, "Pause did not complete through stdin/stdout")
	check(session.status.text.contains("Finished"), "Completion not shown")
	check(FileAccess.file_exists(capture.path_join("ble.jsonl")), "Capture not saved")
	check(FileAccess.get_file_as_string(capture.path_join("fixture-control.txt")) == "pause\nresume 1\nfinish\n", "Pause/Resume/Finish command sequence incorrect")
	var abandoned: RefCounted = ClassDB.instantiate("TrainerSession")
	check(abandoned.start_session(fixture, capture + "-close"), "Close fixture launch failed")
	var running := false
	deadline = Time.get_ticks_msec() + 10000
	while not running and Time.get_ticks_msec() < deadline:
		for row in JSON.parse_string(abandoned.poll_session()):
			if row.get("event") == "command_acknowledged": running = true
		await tree.process_frame
	check(running, "Close fixture never started")
	abandoned = null
	var marker := (capture + "-close").path_join("fixture-control.txt")
	while not FileAccess.file_exists(marker) and Time.get_ticks_msec() < deadline:
		await tree.process_frame
	check(FileAccess.get_file_as_string(marker) == "eof\n", "Dropping bridge did not close control pipe")
	check(session.bridge.start_session(fixture + ".missing", capture + "-missing"), "Missing executable test not dispatched")
	deadline = Time.get_ticks_msec() + 10000
	while session.bridge.is_active() and Time.get_ticks_msec() < deadline:
		await tree.process_frame
	await tree.process_frame
	check(session.status.text.contains("unconfirmed"), "Launch failure not displayed")
	for argument in OS.get_cmdline_user_args():
		if argument.begins_with("--snapshot="):
			session.popup_centered()
			await tree.process_frame
			await RenderingServer.frame_post_draw
			session.get_texture().get_image().save_png(argument.trim_prefix("--snapshot="))
	if not failed: print("TRAINER_SESSION_OK")
	tree.quit(1 if failed else 0)


