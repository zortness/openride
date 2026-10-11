extends RefCounted

func run(app: Node) -> void:
	var workflow = app.route_workflow
	for argument in OS.get_cmdline_user_args():
		if argument.begins_with("--authoring-project="): workflow.project_path = argument.trim_prefix("--authoring-project=")
	assert(not workflow.project_path.is_empty())
	workflow.reload_project()
	assert(workflow.state.has("reviewed"))
	workflow.popup_centered()
	if OS.get_cmdline_user_args().has("--verify-gap-build"):
		assert(not workflow.connect_gaps.button_pressed)
		workflow.connect_gaps.button_pressed = true
		for action in workflow.actions:
			if action.text == "Build preview": action.pressed.emit(); break
		while workflow.pid > 0: await app.get_tree().process_frame
		# Assertions are stripped from release exports; verification must fail explicitly.
		if workflow.progress.text != "Complete: build" or not workflow.state.has("package"):
			push_error("GAP_BUILD_FAILED: " + workflow.progress.text); app.get_tree().quit(1); return
		if not workflow.state.connect_gaps or not workflow.connect_gaps.button_pressed:
			push_error("GAP_BUILD_FAILED: setting did not persist"); app.get_tree().quit(1); return
		var manifest: Dictionary = workflow.data_at(workflow.project_path.path_join(workflow.state.package).path_join("manifest.json"))
		if manifest.is_empty() or manifest.has("coast"):
			push_error("GAP_BUILD_FAILED: missing inland package or unexpected coast"); app.get_tree().quit(1); return
		workflow.open_review("profile")
		var editor = workflow.review_window.content
		if editor.data.route_profile.get("road_gap_connectors",[]).size() != 2 or not editor.data.route_profile.findings.is_empty() or not editor.route_geometry_info.text.contains("Straight join: points 113"):
			push_error("GAP_BUILD_FAILED: connector review mismatch"); app.get_tree().quit(1); return
		print("GAP_BUILD_OK: checkbox builds Red Rock package; setting persists; both connectors listed in profile; no findings")
		app.get_tree().quit(); return
	if OS.get_cmdline_user_args().has("--verify-repair-flow"):
		workflow.open_review("context")
		var editor = workflow.review_window.content
		var manual = editor.manual_repair
		assert(editor.jitter_review.managed_project)
		assert(editor._candidates(110).size() > 3)
		for i in range(108,115):
			manual.remove_current(i)
			assert(not manual.undo_history.is_empty(), manual.status.text)
			assert(not editor.active_repair.points.any(func(p): return int(p.source_sample) == i))
			manual.undo_removal()
		manual.remove_range(108,114)
		assert(not manual.undo_history.is_empty(), manual.status.text)
		var removed_count: int = editor.active_repair.points.size()
		editor.slider.value = 115
		editor._choose_road(0)
		assert(editor.repaired_road.visible)
		assert(editor.active_repair.points.size() == removed_count)
		assert(not manual.source_hash.is_empty())
		# Rejoin another point after changing a road choice, then undo only that deletion.
		manual.remove_current(47)
		assert(editor.active_repair.points.size() == removed_count-1, manual.status.text)
		manual.undo_removal()
		var spans: Array = manual.current_spans().duplicate(true)
		editor.project_corrections_saved.emit(workflow.editor_document(editor))
		while workflow.pid > 0: await app.get_tree().process_frame
		assert(workflow.progress.text == "Complete: edits", workflow.progress.text)
		workflow.open_review("context")
		editor = workflow.review_window.content
		assert(editor.manual_repair.saved_spans == spans)
		assert(editor.repaired_road.visible)
		assert(editor.active_repair.points.size() == removed_count)
		editor.jitter_review.scan_requested.emit()
		while workflow.pid > 0: await app.get_tree().process_frame
		assert(workflow.progress.text == "Complete: jitter", workflow.progress.text)
		editor = workflow.review_window.content
		assert(not editor.jitter_review.report.is_empty(), editor.jitter_review.details.text)
		assert(editor.inspector.visible and editor.inspector_tabs.current_tab == 2)
		assert(editor.manual_repair.saved_spans == spans)
		assert(editor.repaired_road.visible)
		print("REPAIR_FLOW_OK: points 109–115 deleted individually and as range; road edits retain gold preview; deletion/undo still work; combined save/reopen and in-app jitter scan verified")
		app.get_tree().quit(); return
	if OS.get_cmdline_user_args().has("--verify-point-removal"):
		var fixture = preload("res://editor/manual_repair.gd").new(); app.add_child(fixture)
		var points: Array = []; var choices: Array = []
		for i in range(6):
			points.append({"source_sample":i,"position":[i*10.0,0.0,0.0],"elevation_m":0.0,"break_before":i==0})
			choices.append({"source_sample":i,"way_id":"test"})
		fixture.configure({"points":points,"corrected_profile":{"points":points},"corrections":{"road_choices":choices}},"fixture")
		fixture.remove_current(1); fixture.remove_current(2)
		assert(fixture.proposal.spans.size() == 1)
		assert(fixture.proposal.spans[0].start_source_sample == 0 and fixture.proposal.spans[0].end_source_sample == 3)
		fixture.undo_removal()
		assert(fixture.proposal.spans[0].end_source_sample == 2)
		fixture.undo_removal(); assert(fixture.proposal.spans.is_empty())
		fixture.queue_free()
		workflow.open_review("corrected")
		var editor = workflow.review_window.content
		var manual = editor.manual_repair
		var prior: Array = manual.saved_spans.duplicate(true)
		var chosen := -1
		for i in range(1, editor.data.points.size()-1):
			manual.remove_current(i)
			if not manual.undo_history.is_empty(): chosen = i; manual.undo_removal(); break
		assert(chosen > 0)
		editor.slider.value = chosen
		var key := InputEventKey.new(); key.pressed = true; key.keycode = KEY_DELETE
		editor.slider.grab_focus()
		Input.parse_input_event(key)
		await app.get_tree().process_frame
		assert(manual.undo_history.size() == 1)
		assert(editor.marker.visible)
		assert(int(editor.slider.value) != chosen)
		assert(not editor.road.visible)
		assert(editor.retained_markers.multimesh.instance_count == editor.active_repair.points.size())
		var after := int(editor.slider.value)
		editor.slider.value = chosen
		assert(int(editor.slider.value) != chosen) # dragging back also skips deleted point
		editor.slider.value = after
		key.keycode = KEY_Z; key.ctrl_pressed = true
		Input.parse_input_event(key)
		await app.get_tree().process_frame
		assert(manual.undo_history.is_empty())
		assert(manual.proposal.spans == prior)
		assert(editor.marker.visible)
		manual.remove_current(0)
		assert(manual.undo_history.size() == 1)
		assert(not editor.active_repair.points.any(func(p): return int(p.source_sample) == 0))
		editor.slider.value = 47
		key = InputEventKey.new(); key.pressed = true; key.keycode = KEY_DELETE
		Input.parse_input_event(key); await app.get_tree().process_frame
		assert(manual.undo_history.size() == 2, manual.status.text)
		assert(not editor.active_repair.points.any(func(p): return int(p.source_sample) == 47))
		var proof := FileAccess.open(workflow.project_path.path_join("verified-point-removals.json"), FileAccess.WRITE)
		proof.store_string(JSON.stringify(manual.proposal)); proof.close()
		manual.undo_removal(); manual.undo_removal()
		assert(manual.proposal.spans == prior)
		print("POINT_REMOVAL_OK: points 1 and 48 deleted, start trimmed, slider skips removals, two undos restore prior spans")
		for argument in OS.get_cmdline_user_args():
			if argument.begins_with("--snapshot="):
				editor.slider.value = chosen
				editor._remove_current_point(); editor._focus_point()
				await app._snapshot(argument.trim_prefix("--snapshot=")); return
		app.get_tree().quit(); return
	if OS.get_cmdline_user_args().has("--verify-project-reopen"):
		workflow.job("open")
		while workflow.pid > 0: await app.get_tree().process_frame
		assert(workflow.progress.text == "Complete: open", workflow.progress.text)
		assert(workflow.source.text.contains(workflow.state.source_filename))
		assert(workflow.step_status[1].text.contains("Terrain: Ready"))
		assert(workflow.tabs.get_tab_title(0).contains("✓"))
		var prior_spans: Array = workflow.data_at(workflow.project_path.path_join("repairs.json")).spans
		workflow.open_review("context")
		var editor = workflow.review_window.content
		assert(editor.managed_project)
		assert(editor.edits.size() == editor.data.corrections.road_choices.size())
		editor.project_corrections_saved.emit(editor.data.corrections)
		while workflow.pid > 0: await app.get_tree().process_frame
		assert(workflow.progress.text == "Complete: corrections", workflow.progress.text)
		assert(workflow.data_at(workflow.project_path.path_join("repairs.json")).spans == prior_spans)
		workflow.open_review("corrected")
		assert(workflow.review_window.content.manual_repair.saved_spans == prior_spans)
		workflow.review_window.hide()
		workflow.tabs.current_tab = 3
		print("PROJECT_REOPEN_OK: source restored, completed steps visible, managed corrections saved without picker, repairs retained and loaded")
		await app.get_tree().process_frame
		for argument in OS.get_cmdline_user_args():
			if argument.begins_with("--snapshot="): await app._snapshot(argument.trim_prefix("--snapshot=")); return
		app.get_tree().quit(); return
	if OS.get_cmdline_user_args().has("--verify-repair-tools"):
		workflow.open_review("context")
		var editor = workflow.review_window.content
		assert(editor.data.has("reviewed_profile"))
		assert(editor.edits.size() == editor.data.corrections.road_choices.size())
		editor._next_missing_choice()
		assert(editor.data.points[int(editor.slider.value)].source_sample == 47)
		var center_before: Vector3 = editor.center
		var motion := InputEventMouseMotion.new()
		motion.button_mask = MOUSE_BUTTON_MASK_LEFT; motion.shift_pressed = true; motion.relative = Vector2(80, 40)
		editor._unhandled_input(motion)
		assert(editor.center.distance_to(center_before) > 0.1)
		editor._focus_point()
		assert(editor.center.distance_to(center_before) < 0.01)
		print("REPAIR_TOOLS_OK: saved choices/removals loaded; next missing sample 47; shift-drag pans; focus restores target")
		await app.get_tree().process_frame
		for argument in OS.get_cmdline_user_args():
			if argument.begins_with("--snapshot="): await app._snapshot(argument.trim_prefix("--snapshot=")); return
		app.get_tree().quit(); return
	workflow.job("build", {"geometry": "road", "elevation": "recorded", "use_imagery": true})
	assert(workflow.pid > 0)
	var deadline := Time.get_ticks_msec() + 120000
	while workflow.pid > 0 and Time.get_ticks_msec() < deadline: await app.get_tree().process_frame
	assert(workflow.pid <= 0)
	assert(workflow.progress.text == "Complete: build", workflow.progress.text)
	assert(workflow.state.has("package"))
	workflow.open_review("context")
	assert(not workflow.review_window.content.data.is_empty())
	workflow.review_window.hide()
	workflow.tabs.current_tab = 4
	print("AUTHORING_UI_OK: background build, saved project, embedded repair viewer, package ready")
	await app.get_tree().process_frame
	for argument in OS.get_cmdline_user_args():
		if argument.begins_with("--snapshot="): await app._snapshot(argument.trim_prefix("--snapshot=")); return
	app.get_tree().quit()
