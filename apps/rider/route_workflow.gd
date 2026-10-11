extends "res://screen.gd"

signal preview_requested(path: String)
signal installed
var project_path := ""
var state: Dictionary = {}
var pid := -1
var elapsed := 0.0
var reviewed_package := ""
var location: Label
var progress: Label
var log_view: TextEdit
var source: LineEdit
var python: LineEdit
var start: SpinBox
var finish: SpinBox
var quality: OptionButton
var geometry: OptionButton
var heights: OptionButton
var aerial: CheckBox
var connect_gaps: CheckBox
var confirmed: CheckBox
var tabs: TabContainer
var actions: Array[Button] = []
var requirements: Dictionary = {}
var review_window: Control
var library_directory := ""
var step_status: Array[Label] = []

func _ready() -> void:
	title = "OpenRide / Build a route"
	size = Vector2i(940, 740)
	min_size = Vector2i(800, 650)
	close_requested.connect(hide)
	var margin := MarginContainer.new()
	margin.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	for side in ["left", "right", "top", "bottom"]: margin.add_theme_constant_override("margin_" + side, 18)
	add_child(margin)
	var box := VBoxContainer.new(); margin.add_child(box)
	var row := HBoxContainer.new(); box.add_child(row)
	button(row, "New project folder", func(): choose(FileDialog.FILE_MODE_OPEN_DIR, [], func(path: String):
		switch_project(path)))
	button(row, "Open project", func(): choose(FileDialog.FILE_MODE_OPEN_FILE, ["project.json ; Route project"], func(path: String):
		switch_project(path.get_base_dir())))
	location = label(box, "Choose an empty folder to keep this route and its downloaded sources.")
	tabs = TabContainer.new(); tabs.size_flags_vertical = Control.SIZE_EXPAND_FILL; box.add_child(tabs)
	var page := page_named("1 Import")
	label(page, "Import a GPX, TCX, KML LineString, or saved .orroute.")
	row = HBoxContainer.new(); page.add_child(row)
	source = LineEdit.new(); source.placeholder_text = "Source file"; source.size_flags_horizontal = Control.SIZE_EXPAND_FILL; row.add_child(source)
	button(row, "Browse", func(): choose(FileDialog.FILE_MODE_OPEN_FILE, ["*.gpx,*.tcx,*.kml,*.orroute ; Route sources"], func(path: String): source.text = path))
	label(page, "Choose a section in metres along the recording (up to 5 km per project).")
	row = HBoxContainer.new(); page.add_child(row)
	label(row, "From"); start = spin(row, 0); label(row, "To"); finish = spin(row, 5000)
	button(page, "Import section", func(): job("import", {"source": source.text, "start_m": start.value, "end_m": finish.value}), "new")
	label(page, "For sample-by-sample selection, export a section from Route studio first.\nKML ground-clamped routes need terrain heights in step 4. FIT import is not supported.")
	page = page_named("2 Map sources")
	label(page, "These actions use the internet. Downloads stay in this project for offline use.")
	button(page, "Acquire terrain", func(): job("terrain"), "imported")
	button(page, "Acquire streets, buildings and land cover", func(): job("osm"), "terrain")
	label(page, "Terrain: USGS 3DEP (US coverage). Streets and buildings: OpenStreetMap.\nA failed stage can be retried without downloading completed sources again.")
	page = page_named("3 Imagery")
	label(page, "Aerial colour makes the terrain more recognizable. This step is optional.")
	quality = OptionButton.new(); page.add_child(quality)
	quality.add_item("2048 x 2048 / 16 small requests", 4)
	quality.add_item("1024 x 1024 / 4 small requests", 2)
	quality.add_item("512 x 512 / 1 request", 1)
	button(page, "Acquire / resume aerial imagery", func(): job("imagery", {"grid": quality.get_selected_id()}), "terrain")
	label(page, "USGS NAIP (US coverage). Completed 512-pixel tiles are reused on retry.\nChanging quality uses a separate cache. Photography dates vary by location.")
	page = page_named("4 Repair")
	label(page, "Edit road choices, heights and point removals together. Save route edits keeps all changes.\nIn Show tools → Repairs, Find stops / GPS jitter scans the imported recording.")
	button(page, "Edit route / road matches / remove points", func(): open_review("context"), "context")
	row = HBoxContainer.new(); page.add_child(row)
	geometry = OptionButton.new(); geometry.add_item("Follow selected roads", 0); geometry.add_item("Use corrected GPS points", 1); row.add_child(geometry)
	heights = OptionButton.new(); heights.add_item("Use corrected recorded heights", 0); heights.add_item("Project heights onto terrain", 1); row.add_child(heights)
	label(page, "Default points keep their recorded positions. Selected sections follow roads.\nInspect the connections in preview; gaps and invalid grades still need repair.")
	connect_gaps = CheckBox.new(); connect_gaps.text = "Connect short gaps between selected roads"; page.add_child(connect_gaps)
	connect_gaps.tooltip_text = "Insert straight connectors up to 100 m where selected roads do not share OSM nodes. Inspect these joins in preview."
	aerial = CheckBox.new(); aerial.text = "Use downloaded aerial imagery"; aerial.button_pressed = true; page.add_child(aerial)
	button(page, "Build preview", func(): job("build", {"geometry": "road" if geometry.selected == 0 else "gps", "elevation": "recorded" if heights.selected == 0 else "terrain", "use_imagery": aerial.button_pressed, "connect_gaps":connect_gaps.button_pressed}), "context")
	page = page_named("5 Preview")
	button(page, "Inspect generated profile", func(): open_review("profile"), "profile")
	button(page, "Open riding preview", preview, "package")
	confirmed = CheckBox.new(); confirmed.text = "I have inspected this preview and want it in Offline routes"; page.add_child(confirmed)
	button(page, "Add to Offline routes", func():
		if reviewed_package != str(state.get("package", "")) or not confirmed.button_pressed:
			progress.text = "Open the current riding preview and confirm your review first."; return
		job("install", {"library": library_directory, "reviewed_package": reviewed_package}), "package")
	label(page, "Packages contain scenery and route data and need no downloads while riding.\nAuthoring previews remain marked unvalidated; trainer limits still apply.")
	page = page_named("Settings")
	label(page, "Python executable (3.10 or newer; used for map preparation)")
	python = LineEdit.new(); page.add_child(python)
	var config := ConfigFile.new(); config.load("user://authoring.cfg")
	var candidate := OS.get_environment("LOCALAPPDATA").path_join("Python/bin/python.exe")
	python.text = str(config.get_value("authoring", "python", candidate if FileAccess.file_exists(candidate) else "python"))
	python.text_changed.connect(func(value: String): config.set_value("authoring", "python", value); config.save("user://authoring.cfg"))
	label(page, "Map tools and content-builder are supplied alongside the desktop app.\nKeep the project folder to edit, retry downloads or rebuild later.")
	progress = label(box, "Ready")
	log_view = TextEdit.new(); log_view.editable = false; log_view.custom_minimum_size.y = 120; log_view.add_theme_font_size_override("font_size", 12); box.add_child(log_view)
	refresh()

func label(parent: Node, text: String) -> Label:
	var result := Label.new(); result.text = text; result.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART; parent.add_child(result); return result

func page_named(caption: String) -> VBoxContainer:
	var result := VBoxContainer.new(); result.name = caption; result.add_theme_constant_override("separation", 12); tabs.add_child(result)
	step_status.append(label(result, ""))
	return result

func spin(parent: Node, value: float) -> SpinBox:
	var result := SpinBox.new(); result.max_value = 1000000; result.value = value; result.custom_minimum_size.x = 150; parent.add_child(result); return result

func button(parent: Node, caption: String, callback: Callable, required := "") -> void:
	var result := Button.new(); result.text = caption; result.custom_minimum_size.y = 36; result.pressed.connect(callback); parent.add_child(result); actions.append(result); requirements[result] = required

func choose(mode: FileDialog.FileMode, filters: Array, callback: Callable) -> void:
	var dialog := FileDialog.new(); dialog.display_mode = FileDialog.DISPLAY_LIST; dialog.access = FileDialog.ACCESS_FILESYSTEM; dialog.file_mode = mode; dialog.filters = PackedStringArray(filters); add_child(dialog)
	if not project_path.is_empty(): dialog.current_dir = project_path
	if mode == FileDialog.FILE_MODE_OPEN_DIR: dialog.dir_selected.connect(callback)
	else: dialog.file_selected.connect(callback)
	dialog.canceled.connect(dialog.queue_free); dialog.visibility_changed.connect(func(): if not dialog.visible: dialog.queue_free())
	dialog.popup_centered_ratio(0.75)

func data_at(path: String) -> Dictionary:
	if not FileAccess.file_exists(path): return {}
	var value = JSON.parse_string(FileAccess.get_file_as_string(path))
	return value if value is Dictionary else {}

func reload_project() -> void:
	state = data_at(project_path.path_join("project.json"))
	if not state.is_empty():
		var preferences := ConfigFile.new(); preferences.load("user://authoring.cfg")
		preferences.set_value("authoring","last_project",project_path); preferences.save("user://authoring.cfg")
	var process := data_at(project_path.path_join("process.json"))
	pid = int(process.get("pid", -1))
	if pid > 0 and not OS.is_process_running(pid): pid = -1
	location.text = project_path + (" / " + str(state.get("name", "New route")))
	source.text = str(state.get("source_path", state.get("source_filename", "")))
	source.tooltip_text = "Retained route: " + project_path.path_join(str(state.get("route_source", "")))
	source.editable = not state.has("imported")
	start.value = state.get("interval_m", [0, 5000])[0]; finish.value = state.get("interval_m", [0, 5000])[1]
	start.editable = not state.has("imported"); finish.editable = not state.has("imported")
	geometry.select(1 if state.get("geometry") == "gps" else 0)
	heights.select(1 if state.get("elevation") == "terrain" else 0)
	connect_gaps.button_pressed = state.get("connect_gaps",false)
	refresh()

func refresh() -> void:
	if step_status.size() >= 5:
		step_status[0].text = "Imported: " + str(state.get("source_filename", state.get("name", "Saved route"))) if state.has("imported") else "Not imported"
		step_status[1].text = "Terrain: %s  /  Streets, buildings, land cover: %s" % ["Ready" if state.has("terrain") else "Not acquired", "Ready" if state.has("context") else "Not acquired"]
		step_status[2].text = "Aerial imagery: Ready" if state.has("imagery") else "Optional / not acquired"
		var editing: Dictionary = state.get("editing_status", {})
		step_status[3].text = "%s / %d saved removal spans / %d points keep original positions" % ["Corrections saved" if state.has("corrected") else "Using original positions", int(editing.get("repair_spans", 0)), int(editing.get("kept_original_points", editing.get("missing_road_choices", 0)))] if state.has("context") else "Acquire map sources to begin editing"
		step_status[4].text = "Preview ready" if state.has("package") else "Preview not built / needs rebuild"
		var done := [state.has("imported"), state.has("context"), state.has("imagery"), state.has("corrected"), state.has("package")]
		var captions := ["1 Import", "2 Map sources", "3 Imagery", "4 Repair", "5 Preview"]
		for i in range(5): tabs.set_tab_title(i, captions[i] + (" ✓" if done[i] else ""))
	for action in actions:
		var requirement: String = requirements[action]
		action.disabled = pid > 0 or (requirement == "new" and (project_path.is_empty() or state.has("imported"))) or (requirement not in ["", "new"] and not state.has(requirement))
	if is_instance_valid(confirmed) and reviewed_package != str(state.get("package", "")): confirmed.button_pressed = false

func job(action: String, extra: Dictionary = {}) -> void:
	if pid > 0 or project_path.is_empty(): return
	var tool_root := OS.get_executable_path().get_base_dir().path_join("authoring")
	var builder := tool_root.path_join("content-builder.exe")
	if OS.has_feature("editor"):
		tool_root = ProjectSettings.globalize_path("res://../../tools")
		builder = ProjectSettings.globalize_path("res://bin/content-builder.exe")
	var request := {"project": project_path, "action": action, "builder": builder}
	request.merge(extra, true)
	var request_path := project_path.path_join("request.json")
	var file := FileAccess.open(request_path, FileAccess.WRITE)
	if file == null: progress.text = "Cannot write to this project folder."; return
	file.store_string(JSON.stringify(request)); file.close()
	# Clear previous completion marker so a failed launch cannot look successful.
	DirAccess.remove_absolute(project_path.path_join("job.json"))
	pid = OS.create_process(python.text, PackedStringArray([tool_root.path_join("route-workflow.py"), request_path]), false)
	if pid < 0: progress.text = "Could not start Python. Check its executable in Settings."; return
	file = FileAccess.open(project_path.path_join("process.json"), FileAccess.WRITE)
	if file != null: file.store_string(JSON.stringify({"pid": pid})); file.close()
	progress.text = "Running: " + action + "..."; refresh()

func _process(delta: float) -> void:
	if pid <= 0: return
	elapsed += delta
	if elapsed < 0.5: return
	elapsed = 0
	var lines := FileAccess.get_file_as_string(project_path.path_join("job.log")) if FileAccess.file_exists(project_path.path_join("job.log")) else "Starting Python..."
	log_view.text = lines.right(6000); log_view.scroll_vertical = log_view.get_line_count()
	if OS.is_process_running(pid): return
	pid = -1
	var result := data_at(project_path.path_join("job.json"))
	progress.text = "Complete: " + str(result.get("action", "")) if result.get("status") == "complete" else "Failed: " + str(result.get("error", "Python did not finish. Check Settings and the project log."))
	reload_project()
	if result.get("status") == "complete":
		var next := {"import": 1, "terrain": 1, "osm": 2, "imagery": 3, "corrections": 3, "repairs": 3, "build": 4}
		if next.has(result.action): tabs.current_tab = next[result.action]
		if result.action == "install": installed.emit()
		if result.action == "jitter":
			open_review("context")
			var editor = review_window.content
			editor.inspector_button.button_pressed = true
			editor.inspector_tabs.current_tab = 2
	elif is_instance_valid(review_window):
		review_window.popup_centered()
		review_window.content.repair_feedback.text = progress.text

func preview() -> void:
	reviewed_package = str(state.package)
	preview_requested.emit(project_path.path_join(reviewed_package))

func open_review(key: String) -> void:
	if is_instance_valid(review_window):
		if review_window.get_meta("project_state","") == project_path+JSON.stringify(state)+key:
			review_window.popup_centered(); return
		review_window.queue_free()
	review_window = preload("res://world_screen.gd").new(); review_window.title = "Review route"
	add_child(review_window)
	review_window.open_requested.connect(func(screen: Control): get_tree().get_first_node_in_group("app_shell").show_world(screen))
	var editor = preload("res://editor/main.gd").new(); editor.embedded = true; editor.managed_project = true; review_window.set_content(editor)
	var load_key := key
	if key in ["context", "corrected"]:
		load_key = "reviewed" if state.has("reviewed") else "corrected" if state.has("corrected") else "context"
	editor.load_preparation(project_path.path_join(str(state[load_key])).path_join("preparation.json"))
	editor.project_corrections_saved.connect(func(document: Dictionary):
		job("edits", {"document": document, "spans":editor.manual_repair.current_spans()}); review_window.hide())
	editor.manual_repair.managed_project = true
	if state.has("corrected"):
		var base_path := project_path.path_join(str(state.corrected)).path_join("preparation.json")
		editor.manual_repair.configure(data_at(base_path), FileAccess.get_sha256(base_path))
	editor.manual_repair.saved_spans = data_at(project_path.path_join("repairs.json")).get("spans", [])
	if key == "corrected" and state.has("reviewed"):
		var saved := data_at(project_path.path_join(str(state.reviewed)).path_join("preparation.json"))
		editor._show_repair(saved.reviewed_profile)
		editor.manual_repair.status.text = "%d saved removal spans loaded. New removals are added when saved." % editor.manual_repair.saved_spans.size()
	editor.manual_repair.project_repairs_saved.connect(func(document: Dictionary):
		job("edits", {"document": editor_document(editor), "spans":document.spans}); review_window.hide())
	editor.jitter_review.managed_project = true
	editor.jitter_review.saved_spans = editor.manual_repair.saved_spans
	editor.jitter_review.project_repairs_saved.connect(func(document: Dictionary):
		job("edits", {"document": editor_document(editor), "spans":document.spans}); review_window.hide())
	editor.jitter_review.scan_requested.connect(func():
		job("jitter", {"document":editor_document(editor), "spans":editor.manual_repair.current_spans()}); review_window.hide())
	# Reapply current choices over saved removals, enabling immediate editing even before the first save.
	if key != "profile": editor._refresh_corrections()
	if state.has("corrected"):
		var corrected_path := project_path.path_join(str(state.corrected)).path_join("preparation.json")
		editor.jitter_review.expected_hash = FileAccess.get_sha256(corrected_path)
		editor.jitter_review.preparation = data_at(corrected_path)
	if state.has("jitter"): editor.jitter_review.load_report(project_path.path_join(str(state.jitter)))
	editor.correction_picker.current_dir = project_path; editor.correction_picker.current_file = "corrections.json"
	editor.correction_picker.file_selected.connect(func(path: String):
		if editor.correction_picker.file_mode == FileDialog.FILE_MODE_SAVE_FILE and key == "context" and FileAccess.file_exists(path): job("corrections", {"corrections": path}))
	for component in [editor.manual_repair, editor.jitter_review]:
		component.picker.current_dir = project_path; component.picker.current_file = "repairs.json"
		component.picker.file_selected.connect(func(path: String):
			if component.picker.file_mode == FileDialog.FILE_MODE_SAVE_FILE and key == "corrected" and FileAccess.file_exists(path): job("repairs", {"repairs": path}))
	review_window.set_meta("project_state",project_path+JSON.stringify(state)+key)
	review_window.popup_centered()

func editor_document(editor: Node) -> Dictionary:
	return {"schema_version":1,"kind":"openride.scenery-corrections","preparation_sha256":editor.preparation_hash,"elevation_offset_m":editor.height_offset.value,"road_choices":editor.edits.values()}

func has_unsaved_edits() -> bool:
	if not is_instance_valid(review_window) or review_window.content==null: return false
	var editor = review_window.content
	var saved: Dictionary = editor.data.get("corrections",{})
	var choices := {}
	for choice in saved.get("road_choices",[]): choices[str(int(choice.source_sample))]=choice
	return editor.edits!=choices or editor.height_offset.value!=saved.get("elevation_offset_m",0.0) or editor.manual_repair.current_spans()!=editor.manual_repair.saved_spans

func switch_project(path: String) -> void:
	var apply := func():
		if is_instance_valid(review_window): review_window.queue_free()
		project_path=path; state={}; reviewed_package=""; reload_project(); job("open")
	if has_unsaved_edits():
		var confirm := ConfirmationDialog.new(); confirm.dialog_text="Discard unsaved route edits and open another project? Back to the editor to save them first."; confirm.ok_button_text="Discard and open"; add_child(confirm)
		confirm.confirmed.connect(apply); confirm.confirmed.connect(confirm.queue_free); confirm.canceled.connect(confirm.queue_free); confirm.popup_centered()
	else: apply.call()
