extends RefCounted

func check(ok: bool, message: String, app: Node) -> bool:
	if not ok: push_error("UX_FAILED: "+message); app.get_tree().quit(1)
	return ok

func run(app: Node) -> void:
	var shell = app.shell
	if not check(shell.stack[-1]==shell.pages.Home,"Startup home",app): return
	if not check(not shell.has_dialog(),"Unexpected startup popup",app): return
	var project := ""; var package := ""; var sessions := ""; var snapshot := ""; var worker := ""
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--ux-project="): project = arg.trim_prefix("--ux-project=")
		if arg.begins_with("--ux-package="): package = arg.trim_prefix("--ux-package=")
		if arg.begins_with("--ux-sessions="): sessions = arg.trim_prefix("--ux-sessions=")
		if arg.begins_with("--ux-snapshot="): snapshot = arg.trim_prefix("--ux-snapshot=")
		if arg.begins_with("--ux-worker="): worker = arg.trim_prefix("--ux-worker=")
	await app.get_tree().process_frame
	var button: Button = app.get_viewport().gui_get_focus_owner()
	if not check(button==shell.tab_buttons[0] and shell.tab_buttons[0].button_pressed,"Home initial focus and selected tab",app): return
	if not check(app.get_tree().root.title=="OpenRide","Windows title",app): return
	var replay: Control = shell.pages.Home.get_child(0)
	await app.get_tree().create_timer(0.12).timeout
	if not check(float(replay.get("progress_m"))>0.0,"Home ride replay moves",app): return
	var mode_button: Button = shell.header.get_child(shell.header.get_child_count()-1)
	if not check(mode_button.text.is_empty() and mode_button.custom_minimum_size.x<=48,"Compact window-mode icon",app): return
	shell.tab_buttons[1].grab_focus()
	var accept := InputEventJoypadButton.new(); accept.button_index=JOY_BUTTON_A; accept.pressed=true
	Input.parse_input_event(accept.duplicate()); await app.get_tree().process_frame
	accept.pressed=false; Input.parse_input_event(accept.duplicate()); await app.get_tree().process_frame
	if not check(shell.stack[-1]==shell.pages.Ride,"Controller confirm route screen",app): return
	if not check(shell.tab_buttons[1].button_pressed and not shell.tab_buttons[0].button_pressed,"Ride outline updates on first activation",app): return
	if not check(replay.get("viewport").render_target_update_mode==SubViewport.UPDATE_DISABLED,"Hidden Home replay stops rendering",app): return
	var back_event := InputEventJoypadButton.new(); back_event.button_index=JOY_BUTTON_B; back_event.pressed=true
	Input.parse_input_event(back_event.duplicate()); await app.get_tree().process_frame
	back_event.pressed=false; Input.parse_input_event(back_event.duplicate())
	if not check(shell.stack[-1]==shell.pages.Home,"Controller Back home",app): return
	if not check(shell.tab_buttons[0].button_pressed and not shell.tab_buttons[1].button_pressed,"Home outline follows navigation",app): return
	shell.tab_buttons[2].pressed.emit()
	if not check(shell.stack[-1]==shell.pages.Create and shell.tab_buttons[2].button_pressed and not shell.tab_buttons[0].button_pressed,"Create outline follows one tab press",app): return
	shell.tab_buttons[0].pressed.emit()
	if not check(shell.stack[-1]==shell.pages.Home and shell.tab_buttons[0].button_pressed and not shell.tab_buttons[2].button_pressed,"Home outline follows one tab press",app): return
	var back_button: Button = shell.header.get_child(0)
	back_button.pressed.emit()
	if not check(not shell.has_dialog(),"First Home Back arms exit",app): return
	back_button.pressed.emit()
	var exit_dialog: ConfirmationDialog
	for window in app.get_tree().root.find_children("*","ConfirmationDialog",true,false):
		if window.visible and window.dialog_text=="Are you sure you want to exit?": exit_dialog=window; break
	if not check(exit_dialog!=null and exit_dialog.ok_button_text=="Yes" and exit_dialog.get_cancel_button().text=="No","Second Home Back confirms exit",app): return
	exit_dialog.hide(); exit_dialog.queue_free()
	await app.get_tree().process_frame
	shell.axis=Vector2(0,1); shell.gamepad=true
	Input.joy_connection_changed.emit(0,false)
	if not check(shell.axis==Vector2.ZERO and not shell.gamepad and shell.stack[-1]==shell.pages.Home,"Disconnect clears held navigation without leaving screen",app): return
	shell.navigate(app.route_workflow,true)
	var initial_tab: int=app.route_workflow.tabs.current_tab
	var shoulder := InputEventJoypadButton.new(); shoulder.button_index=JOY_BUTTON_RIGHT_SHOULDER; shoulder.pressed=true
	Input.parse_input_event(shoulder.duplicate()); await app.get_tree().process_frame
	shoulder.pressed=false; Input.parse_input_event(shoulder.duplicate())
	if not check(shell.stack[-1]==app.route_workflow and app.route_workflow.tabs.current_tab!=initial_tab,"Shoulder switches inner authoring tab",app): return
	var picker := FileDialog.new(); picker.file_mode=FileDialog.FILE_MODE_OPEN_DIR; picker.access=FileDialog.ACCESS_FILESYSTEM
	app.add_child(picker); picker.popup_centered(Vector2i(800,600))
	await app.get_tree().process_frame
	var reached_open := false; var reached_cancel := false
	for i in range(60):
		shoulder.pressed=true; picker.push_input(shoulder)
		shoulder.pressed=false; picker.push_input(shoulder)
		await app.get_tree().process_frame
		reached_open = reached_open or picker.gui_get_focus_owner()==picker.get_ok_button()
		reached_cancel = reached_cancel or picker.gui_get_focus_owner()==picker.get_cancel_button()
	if not check(reached_open and reached_cancel,"File picker controller reaches Open and Cancel",app): return
	picker.get_cancel_button().grab_focus()
	accept.pressed=true; picker.push_input(accept)
	accept.pressed=false; picker.push_input(accept)
	await app.get_tree().process_frame
	if not check(not picker.visible,"File picker Cancel activates from controller",app): return
	picker.queue_free(); await app.get_tree().process_frame
	shell.navigate(shell.pages.Home,true)
	if not sessions.is_empty():
		shell.history.directory=sessions; shell.navigate(shell.history,true)
		var deadline := Time.get_ticks_msec()+15000
		while (shell.history.file!=null or not shell.history.queue.is_empty()) and Time.get_ticks_msec()<deadline: await app.get_tree().process_frame
		if not check(not shell.history.entries.is_empty(),"Existing sessions discovered",app): return
		if not check(shell.history.entries.any(func(e): return e.complete and e.distance>0),"Completed ride summary",app): return
		shell.history.select(0); shell.history.export_button.pressed.emit()
		if not check(shell.stack[-1]==app.activity_export and not app.activity_export.source.is_empty(),"History export navigation",app): return
		shell.back()
	if not worker.is_empty() and not package.is_empty():
		var pairing_bridge: RefCounted = ClassDB.instantiate("TrainerSession")
		pairing_bridge.configure_pairing_path(ProjectSettings.globalize_path("user://pairing.json"))
		var fixture_device := {"id":"synthetic-two", "name":"Synthetic trainer"}
		if not check(pairing_bridge.save_pairing(JSON.stringify({"schema":1,"resistance":fixture_device,"power":fixture_device,"cadence":fixture_device})).is_empty(), "Save fixture pairing", app): return
		app._open_scenery(package,true)
		var world = app.scenery_window; var runtime = world.content
		await app.get_tree().process_frame
		if not check(runtime.readiness.visible and not shell.has_dialog(),"Inline disconnected ride setup",app): return
		runtime.live_ride.worker_override=worker
		runtime.hr_settings.enabled.button_pressed=true; runtime.hr_settings.number.value=42; runtime.hr_settings.transmission.value=241
		runtime.connect_ready.pressed.emit()
		var deadline := Time.get_ticks_msec()+10000
		while runtime.live_ride.phase!="paused" and Time.get_ticks_msec()<deadline: await app.get_tree().process_frame
		if not check(runtime.live_ride.active() and runtime.live_ride.phase=="paused","Fixture connected paused",app): return
		shell.back()
		if not check(shell.ride_menu.visible and runtime.live_ride.active() and shell.stack[-1]==world,"Back preserves controller ownership",app): return
		shell.navigate(shell.pages.Home,true)
		if not check(shell.stack[-1]==world,"Active ride cannot be abandoned by tabs",app): return
		shell.pause_button.pressed.emit()
		while runtime.live_ride.phase!="running" and Time.get_ticks_msec()<deadline: await app.get_tree().process_frame
		if not check(runtime.live_ride.phase=="running","Menu resumes ride",app): return
		shell.ride_menu.hide()
		shoulder.pressed=true; Input.parse_input_event(shoulder.duplicate())
		await app.get_tree().process_frame
		while runtime.live_ride.shift_pending and Time.get_ticks_msec()<deadline: await app.get_tree().process_frame
		if not check(runtime.live_ride.gear==6,"Shoulder shifts through worker: held=%s pending=%s focus=%s active_world=%s dialog=%s" % [shell.ride_inputs.held,runtime.live_ride.shift_pending,app.get_viewport().gui_get_focus_owner(),shell.active_world==world,shell.has_dialog()],app): return
		Input.parse_input_event(shoulder.duplicate()); await app.get_tree().process_frame
		if not check(runtime.live_ride.gear==6,"Held shoulder does not repeat",app): return
		shoulder.pressed=false; Input.parse_input_event(shoulder.duplicate())
		var easier := InputEventKey.new(); easier.physical_keycode=KEY_Q; easier.pressed=true
		Input.parse_input_event(easier.duplicate())
		await app.get_tree().process_frame
		while runtime.live_ride.shift_pending and Time.get_ticks_msec()<deadline: await app.get_tree().process_frame
		easier.pressed=false; Input.parse_input_event(easier.duplicate())
		if not check(runtime.live_ride.gear==5 and runtime.live_ride.state.power_w==200 and runtime.live_ride.state.course_grade_percent==1.7,"Keyboard shift preserves power and route grade",app): return
		var direction := InputEventKey.new(); direction.physical_keycode=KEY_J; direction.pressed=true
		Input.parse_input_event(direction.duplicate()); direction.pressed=false; Input.parse_input_event(direction.duplicate())
		await app.get_tree().process_frame
		if not check(runtime.live_ride.shift_note.contains("no selectable junctions"),"Direction action explicitly unavailable on fixed route",app): return
		var modal := AcceptDialog.new(); app.add_child(modal); modal.popup_centered()
		shoulder.pressed=true; Input.parse_input_event(shoulder.duplicate()); shoulder.pressed=false; Input.parse_input_event(shoulder.duplicate())
		await app.get_tree().process_frame
		if not check(runtime.live_ride.gear==5 and not runtime.live_ride.shift_pending,"Dialog blocks shifts",app): return
		modal.hide(); modal.queue_free(); await app.get_tree().process_frame
		var menu := InputEventJoypadButton.new(); menu.button_index=JOY_BUTTON_START; menu.pressed=true
		Input.parse_input_event(menu.duplicate()); menu.pressed=false; Input.parse_input_event(menu.duplicate())
		await app.get_tree().process_frame
		if not check(shell.ride_menu.visible,"Start opens ride menu",app): return
		shoulder.pressed=true; Input.parse_input_event(shoulder.duplicate()); shoulder.pressed=false; Input.parse_input_event(shoulder.duplicate())
		await app.get_tree().process_frame
		if not check(runtime.live_ride.gear==5 and not runtime.live_ride.shift_pending,"Menu blocks shifts",app): return
		menu.pressed=true; Input.parse_input_event(menu.duplicate()); menu.pressed=false; Input.parse_input_event(menu.duplicate())
		await app.get_tree().process_frame
		var pause := InputEventKey.new(); pause.physical_keycode=KEY_P; pause.pressed=true
		Input.parse_input_event(pause.duplicate()); pause.pressed=false; Input.parse_input_event(pause.duplicate())
		while runtime.live_ride.phase!="paused" and Time.get_ticks_msec()<deadline: await app.get_tree().process_frame
		if not check(runtime.live_ride.phase=="paused","Keyboard pauses ride",app): return
		shoulder.pressed=true; Input.parse_input_event(shoulder.duplicate()); shoulder.pressed=false; Input.parse_input_event(shoulder.duplicate())
		if not check(not runtime.live_ride.shift_pending,"Paused ride ignores shifts",app): return
		pause.pressed=true; Input.parse_input_event(pause.duplicate()); pause.pressed=false; Input.parse_input_event(pause.duplicate())
		while runtime.live_ride.phase!="running" and Time.get_ticks_msec()<deadline: await app.get_tree().process_frame
		if not check(runtime.live_ride.phase=="running","Keyboard explicitly resumes ride",app): return
		shell.back()
		shell.pause_button.pressed.emit()
		while runtime.live_ride.phase!="paused" and Time.get_ticks_msec()<deadline: await app.get_tree().process_frame
		if not check(runtime.live_ride.phase=="paused","Menu pauses ride",app): return
		# Force the completion-drain race: the shell must collect final worker
		# events even when the ride screen did not get another processing frame.
		runtime.set_process(false)
		shell.finish_button.pressed.emit()
		while shell.stack[-1]!=shell.history and Time.get_ticks_msec()<deadline: await app.get_tree().process_frame
		if not check(not runtime.live_ride.active() and runtime.live_ride.completed and shell.stack[-1]==shell.history,"End waits for shutdown then opens activities",app): return
		print("UX_RIDE_OK: fixture-only connect/resume/pause/finish; Back does not stop the controller")
	if not project.is_empty():
		app.route_workflow.project_path=project; app.route_workflow.reload_project(); shell.navigate(app.route_workflow,true)
		app.route_workflow.open_review("context")
		var world = app.route_workflow.review_window; var editor = world.content
		if not check(shell.stack[-1]==world and world.get_parent()==shell.body,"Embedded editor",app): return
		var yaw: float=editor.yaw
		var stick := InputEventJoypadMotion.new(); stick.axis=JOY_AXIS_LEFT_X; stick.axis_value=0.8
		Input.parse_input_event(stick.duplicate())
		for i in range(5): await app.get_tree().process_frame
		stick.axis_value=0; Input.parse_input_event(stick.duplicate())
		if not check(editor.yaw!=yaw and shell.axis==Vector2.ZERO,"Left stick orbits instead of moving focus",app): return
		var tool_tab: int=editor.inspector_tabs.current_tab
		shoulder.pressed=true; Input.parse_input_event(shoulder.duplicate()); await app.get_tree().process_frame
		shoulder.pressed=false; Input.parse_input_event(shoulder.duplicate())
		if not check(shell.stack[-1]==world and editor.inspector_tabs.current_tab!=tool_tab and editor.inspector.visible,"Shoulder opens and switches editor tools",app): return
		editor.slider.value=110; editor._focus_point()
		var radius: float = editor.radius; var position: float = editor.slider.value
		shell.back(); app.route_workflow.open_review("context")
		if not check(app.route_workflow.review_window==world and editor.radius==radius and editor.slider.value==position,"Editor state preserved",app): return
		shell.back()
	if not package.is_empty():
		app._open_scenery(package,false)
		var world = app.scenery_window
		for i in range(30): await app.get_tree().process_frame
		if not check(world.viewport.size.x>500 and world.content.get_viewport()==world.viewport,"Embedded 3D viewport",app): return
		shell.back()
	print("UX_OK: home, controller confirm/back, file dialog actions, contextual tabs, editor orbit/state, recorded history/export and embedded preview; controllers=",Input.get_connected_joypads())
	if not snapshot.is_empty():
		shell.navigate(shell.history,true)
		while shell.history.file!=null or not shell.history.queue.is_empty(): await app.get_tree().process_frame
		await app._snapshot(snapshot); return
	shell.navigate(shell.pages.Home,true)
	shell.back(); shell.back()
	for window in app.get_tree().root.find_children("*","ConfirmationDialog",true,false):
		if window.visible and window.dialog_text=="Are you sure you want to exit?":
			window.get_ok_button().pressed.emit()
			return
	check(false,"Home Yes exit dialog remained unavailable",app)
