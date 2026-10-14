extends "res://screen.gd"
## Native project operations share the public CLI and its JSON-lines contract.
signal legacy_requested
signal preview_requested(path: String)
var project_path := ""
var snapshot: Dictionary = {}
var worker: Thread
var busy := false
var pending_path := ""
var pending_action := ""
var cancel_path := ""
var project_name: LineEdit
var overview: Label
var feedback: Label
var activity: Control
var rename_button: Button
var cancel_button: Button
var controls: Array[Button] = []
var intake: VBoxContainer
var repairs: VBoxContainer
var maps: VBoxContainer
var course: VBoxContainer
var build: VBoxContainer
var scenery: VBoxContainer
var map_poll := 0.0
var stages: TabContainer
var job_queue:Array[Dictionary]=[]

func _ready() -> void:
	title = "Ride Project"
	var margin := MarginContainer.new()
	margin.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	for side in ["left", "right", "top", "bottom"]: margin.add_theme_constant_override("margin_" + side, 24)
	add_child(margin)
	var box := VBoxContainer.new(); box.size_flags_horizontal = Control.SIZE_EXPAND_FILL; box.size_flags_vertical = Control.SIZE_EXPAND_FILL
	box.add_theme_constant_override("separation", 12); margin.add_child(box)
	text(box, "Road ride project", 26)
	var row := HBoxContainer.new(); row.add_theme_constant_override("separation", 12); box.add_child(row)
	project_name = LineEdit.new(); project_name.placeholder_text = "Ride project name"; project_name.max_length = 120; project_name.size_flags_horizontal=Control.SIZE_EXPAND_FILL; row.add_child(project_name)
	add_button(row, "New Ride Project…", func(): choose("create"))
	add_button(row, "Open Ride Project…", func(): choose("open"))
	rename_button = add_button(row, "Save name", func(): run_job("rename", project_path))
	overview = text(box, "Choose a new or empty folder, or open an existing project.", 15)
	var status_row := HBoxContainer.new(); status_row.add_theme_constant_override("separation", 8); box.add_child(status_row)
	activity = preload("res://activity_indicator.gd").new(); status_row.add_child(activity)
	feedback = text(status_row, "Ready"); feedback.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	cancel_button = add_button(box, "Cancel operation", request_cancel)
	stages = TabContainer.new(); stages.size_flags_horizontal = Control.SIZE_EXPAND_FILL; stages.size_flags_vertical=Control.SIZE_EXPAND_FILL; box.add_child(stages)
	stages.add_theme_font_size_override("font_size",17)
	for state in ["panel","tab_selected","tab_unselected","tab_hovered"]:
		var style:=StyleBoxFlat.new();style.bg_color=Color("18343c") if state=="tab_selected" else Color("10232a")
		style.content_margin_left=16;style.content_margin_right=16;style.content_margin_top=12;style.content_margin_bottom=12
		stages.add_theme_stylebox_override(state,style)
	var recording := stage_tab("1  Recording")
	var roads := stage_tab("2  Roads")
	var preview := stage_tab("3  Preview")
	var detail := stage_tab("4  Scenery & share")
	for stage in [recording,roads,preview,detail]: stage.add_theme_constant_override("separation",16)
	intake = preload("res://ride_project_import.gd").new(); recording.add_child(intake); intake.setup(self)
	var advanced := foldout(recording,"Advanced: recording repairs and legacy tools")
	repairs = preload("res://ride_project_repairs.gd").new(); advanced.add_child(repairs); repairs.setup(self)
	add_button(advanced,"Legacy section builder",func():legacy_requested.emit())
	text(advanced,"Road inference uses the original reference recording. Legacy repairs and builds remain preserved separately.",14)
	maps = preload("res://ride_project_maps.gd").new(); roads.add_child(maps); maps.setup(self)
	course = preload("res://ride_project_course.gd").new(); roads.add_child(course); course.setup(self)
	text(preview,"A course made from roads and terrain",24)
	text(preview,"Confirm uncertain roads first. The generated elevation profile follows terrain; your recording stays unchanged.",16)
	build = preload("res://ride_project_build.gd").new(); preview.add_child(build); build.setup(self)
	scenery = preload("res://ride_project_scenery.gd").new(); detail.add_child(scenery); scenery.setup(self)
	add_button(detail,"Build updated preview",func():stages.current_tab=2)
	add_button(detail,"Export ride ZIP",func():run_job("export-package",project_path))
	refresh()

func stage_tab(caption:String) -> VBoxContainer:
	var scroll:=ScrollContainer.new();scroll.name=caption;scroll.horizontal_scroll_mode=ScrollContainer.SCROLL_MODE_DISABLED;scroll.follow_focus=true;stages.add_child(scroll)
	var body:=VBoxContainer.new();body.size_flags_horizontal=Control.SIZE_EXPAND_FILL;scroll.add_child(body);return body

func foldout(parent: Node, caption: String) -> VBoxContainer:
	var toggle := CheckButton.new(); toggle.text = caption; parent.add_child(toggle)
	var body := VBoxContainer.new(); body.add_theme_constant_override("separation",10); parent.add_child(body); body.hide()
	toggle.toggled.connect(func(value: bool): body.visible = value)
	return body

func text(parent: Node, value: String, font_size := 17) -> Label:
	var item := Label.new(); item.text = value; item.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	item.add_theme_font_size_override("font_size", font_size); parent.add_child(item); return item

func add_button(parent: Node, caption: String, callback: Callable) -> Button:
	var item := Button.new(); item.text = caption; item.custom_minimum_size.y = 42
	item.pressed.connect(callback); parent.add_child(item); controls.append(item); return item

func choose(action: String) -> void:
	if action == "create" and project_name.text.strip_edges().is_empty():
		feedback.text = "Enter a name for the new Ride Project first."; project_name.grab_focus(); return
	var dialog := FileDialog.new(); dialog.access = FileDialog.ACCESS_FILESYSTEM
	dialog.file_mode = FileDialog.FILE_MODE_OPEN_DIR
	dialog.title = "Choose a new or empty project folder" if action == "create" else "Open Ride Project folder"
	if not project_path.is_empty(): dialog.current_dir = project_path
	add_child(dialog)
	dialog.dir_selected.connect(func(path: String): run_job(action, path); dialog.queue_free())
	dialog.canceled.connect(dialog.queue_free); dialog.popup_centered_ratio(0.75)

func executable() -> String:
	if OS.has_feature("editor"): return ProjectSettings.globalize_path("res://bin/openride-author.exe")
	return OS.get_executable_path().get_base_dir().path_join("authoring/openride-author.exe")

func prepare_roads() -> void:
	if busy:return
	job_queue=[{"action":"acquire","extra":PackedStringArray(["--layer","streets"])},{"action":"acquire","extra":PackedStringArray(["--layer","terrain"])},{"action":"infer-roads","extra":PackedStringArray()}]
	run_job("plan-maps",project_path)

func run_job(action: String, path: String, extra := PackedStringArray()) -> void:
	if busy: return
	if not FileAccess.file_exists(executable()):
		job_queue.clear();feedback.text = "Native authoring tool is missing. Rebuild or reinstall OpenRide."; return
	pending_path = path
	pending_action = action
	cancel_path = ProjectSettings.globalize_path("user://author-cancel-%s-%s" % [OS.get_process_id(), Time.get_ticks_usec()])
	var args := PackedStringArray(["project", action, path, "--cancel-file", cancel_path])
	if action in ["create", "rename"]: args.append_array(["--name", project_name.text])
	if action in ["rename", "import", "select-sources", "repair", "plan-maps", "acquire", "build-course", "infer-roads", "reset-roads", "choose-road", "edit-course", "archive-edits", "build-package", "export-package", "edit-scenery"]: args.append_array(["--expected-revision", str(snapshot.get("revision", ""))])
	args.append_array(extra)
	worker = Thread.new(); busy = true; feedback.text = "Opening project…" if action in ["open", "status"] else "Saving project…"
	if action == "import": feedback.text = "Importing and comparing complete recordings…"
	if action == "build-package": feedback.text = "Building terrain and roads…"
	if action == "infer-roads": feedback.text = "Following the recording through connected roads…"
	var tool := executable()
	var error := worker.start(func():
		var output: Array = []
		var exit_code := OS.execute(tool, args, output, true, false)
		return {"exit_code": exit_code, "output": "\n".join(output)})
	if error != OK: job_queue.clear();busy = false; worker = null; feedback.text = "Could not start native authoring worker."
	refresh()

func request_cancel() -> void:
	job_queue.clear()
	if not busy: return
	var file := FileAccess.open(cancel_path, FileAccess.WRITE)
	if file == null: feedback.text = "Could not request cancellation."; return
	file.store_string("cancel"); file.close()
	feedback.text = "Cancellation requested. An already saved project remains saved."

func _process(delta: float) -> void:
	if not busy: return
	if worker.is_alive():
		map_poll += delta
		if pending_action == "build-package" and map_poll > 1.0:
			map_poll = 0.0
			var progress_path := project_path.path_join("jobs/build-progress.json")
			if FileAccess.file_exists(progress_path):
				var progress = JSON.parse_string(FileAccess.get_file_as_string(progress_path))
				if progress is Dictionary: feedback.text = "Building terrain and roads · %d / %d cells" % [progress.chunks, progress.total_cells]
		if pending_action == "acquire" and map_poll > 1.0:
			map_poll = 0.0
			var file := FileAccess.open(project_path.path_join("maps/index.json"), FileAccess.READ)
			if file != null and file.get_length() <= 2097152:
				var parser := JSON.new()
				if parser.parse(file.get_as_text()) == OK and parser.data is Dictionary:
					var plan: Dictionary = parser.data
					feedback.text = "Downloading maps · Topography %d/%d · Streets %d/%d · Imagery %d/%d" % [plan.terrain_ready, plan.tiles.size(), plan.streets_ready, plan.tiles.size(), plan.get("imagery_ready", 0), plan.tiles.size()]
		return
	var result: Dictionary = worker.wait_to_finish(); worker = null; busy = false
	DirAccess.remove_absolute(cancel_path)
	var terminal: Dictionary = {}
	for line in str(result.output).split("\n"):
		if line.strip_edges().is_empty(): continue
		var parser := JSON.new()
		if parser.parse(line) != OK: terminal = {}; break
		var event = parser.data
		if event is Dictionary and event.get("protocol_version") == 1 and event.get("event") == "result": terminal = event
	if result.exit_code == 0 and terminal.get("status") == "complete" and terminal.get("snapshot") is Dictionary:
		snapshot = terminal.snapshot; project_path = pending_path
		project_name.text = str(snapshot.project.name)
		var config := ConfigFile.new(); config.load("user://authoring.cfg")
		config.set_value("authoring", "last_ride_project", project_path); config.save("user://authoring.cfg")
		feedback.text = "Project opened." if pending_action in ["open", "status"] else "Project saved."
		intake.populate()
		repairs.populate()
		maps.populate()
		course.populate()
		scenery.populate()
		build.populate()
		if pending_action=="select-sources":stages.current_tab=1
	else:
		job_queue.clear()
		feedback.text = str(terminal.get("error", {}).get("message", "Native authoring did not return a valid result. Check installation and folder access."))
	refresh()
	if not job_queue.is_empty():
		var next:Dictionary=job_queue.pop_front();run_job(next.action,project_path,next.extra)

func refresh() -> void:
	activity.set_active(busy)
	for item in controls: item.disabled = busy
	rename_button.disabled = busy or snapshot.is_empty()
	project_name.editable = not busy
	cancel_button.visible = busy; cancel_button.disabled = not busy
	if not snapshot.is_empty():
		overview.text = "%s · %s" % [snapshot.project.name, "Import a recording to begin." if snapshot.project.stage == "empty" else "Recording → Roads → Preview → Scenery & share"]
		overview.tooltip_text = project_path
	intake.refresh()
	repairs.refresh()
	maps.refresh()
	course.refresh()
	scenery.refresh()
	build.refresh()

func _exit_tree() -> void:
	if worker != null and worker.is_started():
		request_cancel(); worker.wait_to_finish()
	if not cancel_path.is_empty(): DirAccess.remove_absolute(cancel_path)
