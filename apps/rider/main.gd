extends Control

const Plot = preload("res://plot.gd")
var bridge: RefCounted
var importing := false
var library: Dictionary = {"groups": [], "errors": []}
var activity: Dictionary = {}
var route_document: Dictionary = {}
var rides: ItemList
var sources: OptionButton
var metrics: Label
var details: Label
var findings: ItemList
var status: Label
var map_view: Control
var elevation: Control
var start_sample: SpinBox
var end_sample: SpinBox
var cursor: HSlider
var import_dialog: FileDialog
var route_dialog: FileDialog
var export_dialog: FileDialog
var export_button: Button
var open_button: Button
var updating := false
var imported_paths: Array[String] = []
var verification: RefCounted
var content_library: Control
var scenery_window: Control
var device_monitor: Control
var activity_export: Control
var route_workflow: Control
var ride_project: Control
var shell: Control
var source_screen: Control
var full_ride_review: Control

func _ready() -> void:
	_build_ui()
	shell = preload("res://app_shell.gd").new(); add_child(shell); shell.add_to_group("app_shell"); shell.configure(self,source_screen)
	if OS.get_cmdline_user_args().has("--verify-ride-project"):
		verification = preload("res://ride_project_verification.gd").new(); verification.run.call_deferred(self); return
	if OS.get_cmdline_user_args().has("--verify-full-preview"):
		verification = preload("res://full_route_verification.gd").new(); verification.run.call_deferred(self); return
	if OS.get_cmdline_user_args().has("--verify-source-review"):
		verification = preload("res://source_review_verification.gd").new(); verification.run.call_deferred(self); return
	if OS.get_cmdline_user_args().has("--verify-ux"):
		verification = preload("res://ux_verification.gd").new(); verification.run.call_deferred(self); return
	if OS.get_cmdline_user_args().has("--verify-discovery"):
		verification = preload("res://discovery_verification.gd").new(); verification.run.call_deferred(get_tree()); return
	if OS.get_cmdline_user_args().has("--verify-authoring"):
		verification = preload("res://authoring_verification.gd").new()
		verification.run.call_deferred(self); return
	var content_directory:=""
	var stream_package:=""
	for argument in OS.get_cmdline_user_args():
		if argument.begins_with("--content-directory="):content_directory=argument.trim_prefix("--content-directory=")
		if argument.begins_with("--stream-package="):stream_package=argument.trim_prefix("--stream-package=")
	if not content_directory.is_empty():content_library.directory=content_directory
	if not stream_package.is_empty():
		_open_scenery.call_deferred(stream_package);return
	if OS.get_cmdline_user_args().has("--verify-content-library"):
		_verify_content.call_deferred();return
	if ClassDB.class_exists("RideInspector"):
		bridge = ClassDB.instantiate("RideInspector")
		status.text = "Ready • offline • Rust core connected"
	else:
		status.text = "Native extension unavailable. Build the Rust bridge using tools/build.ps1."
		open_button.disabled = true
	for argument in OS.get_cmdline_user_args():
		if argument.begins_with("--device-capture="):
			device_monitor.open_capture(argument.trim_prefix("--device-capture="))
			device_monitor.popup_centered()
		if argument.begins_with("--preview-json="):
			apply_library(JSON.parse_string(FileAccess.get_file_as_string(argument.trim_prefix("--preview-json="))))
		elif argument.begins_with("--import=") and bridge != null:
			import_paths(PackedStringArray([argument.trim_prefix("--import=")]))
		elif argument.begins_with("--snapshot="):
			_snapshot.call_deferred(argument.trim_prefix("--snapshot="))
	if OS.get_cmdline_user_args().has("--verify-rpv"):
		verification = preload("res://verification.gd").new()
		verification.run.call_deferred(self)
	if OS.get_cmdline_user_args().has("--verify-trainer-ui"):
		verification = preload("res://trainer_verification.gd").new()
		verification.run.call_deferred(get_tree())
	if OS.get_cmdline_user_args().has("--verify-activity-export"):
		verification = preload("res://activity_export_verification.gd").new()
		verification.run.call_deferred(get_tree())
	

func _open_route_library() -> void:
	content_library.refresh();content_library.popup_centered(Vector2i(720,460))

func _snapshot(path: String) -> void:
	while importing:
		await get_tree().process_frame
	await get_tree().process_frame
	await get_tree().process_frame
	await RenderingServer.frame_post_draw
	var error := get_viewport().get_texture().get_image().save_png(path)
	print("SNAPSHOT_RESULT=", error)
	get_tree().quit(error)

func _label(text: String, font_size := 16) -> Label:
	var label := Label.new()
	label.text = text
	label.add_theme_font_size_override("font_size", font_size)
	return label

func _button(text: String, callback: Callable) -> Button:
	var button := Button.new()
	button.text = text
	button.custom_minimum_size.y = 38
	button.pressed.connect(callback)
	return button

func _build_ui() -> void:
	# Keep the scene's checkbox textures; project themes load before cold imports.
	var app_theme: Theme = theme.duplicate()
	app_theme.default_font_size = 15
	app_theme.set_color("font_color", "Label", Color("dde8ed"))
	theme = app_theme
	var margin := MarginContainer.new()
	source_screen = margin
	margin.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	for side in ["left", "right", "top", "bottom"]:
		margin.add_theme_constant_override("margin_" + side, 22)
	add_child(margin)
	var root := VBoxContainer.new()
	root.add_theme_constant_override("separation", 12)
	margin.add_child(root)
	var header := HBoxContainer.new()
	root.add_child(header)
	var brand := _label("OPENRIDE  /  Route studio", 27)
	brand.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	header.add_child(brand)
	open_button = _button("Import activities…", func(): import_dialog.popup_centered_ratio(0.75))
	header.add_child(open_button)
	header.add_child(_button("Open route…", func(): route_dialog.popup_centered_ratio(0.75)))
	device_monitor = preload("res://device_monitor.gd").new()
	device_monitor.visible = false
	add_child(device_monitor)
	header.add_child(_button("Device monitor", func(): device_monitor.popup_centered()))
	var trainer_session := preload("res://trainer_session.gd").new()
	trainer_session.visible = false
	add_child(trainer_session)
	header.add_child(_button("Trainer session", func(): trainer_session.popup_centered()))
	activity_export = preload("res://activity_export.gd").new()
	activity_export.visible = false
	add_child(activity_export)
	# Use a second row so the route controls still fit the desktop baseline window.
	var secondary:=HBoxContainer.new();root.add_child(secondary)
	secondary.add_child(_button("Activity exports", func(): activity_export.popup_centered()))
	content_library=preload("res://content_library.gd").new();add_child(content_library)
	content_library.package_selected.connect(_open_scenery)
	content_library.ride_selected.connect(func(path:String):_open_scenery(path,true))
	secondary.add_child(_button("Offline routes", func():
		content_library.refresh();content_library.popup_centered(Vector2i(720,460))))
	route_workflow = preload("res://route_workflow.gd").new()
	route_workflow.visible = false
	add_child(route_workflow)
	route_workflow.preview_requested.connect(_open_scenery)
	route_workflow.installed.connect(content_library.refresh)
	secondary.add_child(_button("Build a route", func():
		route_workflow.library_directory = content_library.directory
		route_workflow.popup_centered()))
	root.add_child(_label("Inspect a ride. Keep the source. Shape the route.", 16))
	var split := HSplitContainer.new()
	split.size_flags_vertical = Control.SIZE_EXPAND_FILL
	root.add_child(split)
	var side := VBoxContainer.new()
	side.custom_minimum_size.x = 270
	split.add_child(side)
	side.add_child(_label("ACTIVITY LIBRARY", 13))
	rides = ItemList.new()
	rides.size_flags_vertical = Control.SIZE_EXPAND_FILL
	rides.item_selected.connect(_select_group)
	side.add_child(rides)
	side.add_child(_label("Source export", 13))
	sources = OptionButton.new()
	sources.item_selected.connect(_select_source)
	side.add_child(sources)
	var note := _label("Paired exports stay together.\nEach source retains its own values.\n\nClick a plot to inspect a sample.", 13)
	note.modulate = Color("95acb8")
	side.add_child(note)
	var content := VBoxContainer.new()
	content.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	split.add_child(content)
	metrics = _label("Import TCX, GPX or KML files to begin", 19)
	content.add_child(metrics)
	map_view = Plot.new()
	map_view.custom_minimum_size.y = 260
	map_view.size_flags_vertical = Control.SIZE_EXPAND_FILL
	map_view.sample_picked.connect(_pick_sample)
	content.add_child(map_view)
	content.add_child(_label("SOURCE ELEVATION  •  uncorrected, not ready for trainer control", 13))
	elevation = Plot.new()
	elevation.profile = true
	elevation.custom_minimum_size.y = 145
	elevation.sample_picked.connect(_pick_sample)
	content.add_child(elevation)
	var range_row := HBoxContainer.new()
	content.add_child(range_row)
	range_row.add_child(_label("Route range (sample indexes)", 13))
	start_sample = SpinBox.new()
	end_sample = SpinBox.new()
	for spin in [start_sample, end_sample]:
		spin.step = 1
		spin.custom_minimum_size.x = 110
		spin.value_changed.connect(_range_changed)
		range_row.add_child(spin)
	range_row.add_child(_button("Start here", func(): start_sample.value = cursor.value))
	range_row.add_child(_button("End here", func(): end_sample.value = cursor.value))
	export_button = _button("Export route…", _export_dialog)
	export_button.disabled = true
	range_row.add_child(export_button)
	cursor = HSlider.new()
	cursor.step = 1
	cursor.value_changed.connect(_cursor_changed)
	content.add_child(cursor)
	details = _label("No sample selected", 13)
	content.add_child(details)
	content.add_child(_label("DATA QUALITY  •  select a finding to inspect its sample", 13))
	findings = ItemList.new()
	findings.custom_minimum_size.y = 110
	findings.item_selected.connect(func(index):
		var sample = findings.get_item_metadata(index)
		if sample != null: _pick_sample(int(sample)))
	content.add_child(findings)
	status = _label("Starting…", 13)
	status.modulate = Color("56d6bd")
	root.add_child(status)
	import_dialog = _file_dialog(FileDialog.FILE_MODE_OPEN_FILES, PackedStringArray(["*.tcx,*.gpx,*.kml ; Garmin activity exports"]))
	import_dialog.files_selected.connect(import_paths)
	route_dialog = _file_dialog(FileDialog.FILE_MODE_OPEN_FILE, PackedStringArray(["*.orroute ; OpenRide route"]))
	route_dialog.file_selected.connect(open_route)
	export_dialog = _file_dialog(FileDialog.FILE_MODE_SAVE_FILE, PackedStringArray(["*.orroute ; OpenRide route manifest", "*.gpx ; GPX track (no biometrics)"]))
	export_dialog.file_selected.connect(export_selection)

func _file_dialog(mode: FileDialog.FileMode, filters: PackedStringArray) -> FileDialog:
	var dialog := FileDialog.new(); dialog.display_mode = FileDialog.DISPLAY_LIST
	dialog.access = FileDialog.ACCESS_FILESYSTEM
	dialog.file_mode = mode
	dialog.filters = filters
	add_child(dialog)
	return dialog

func import_paths(paths: PackedStringArray) -> void:
	if bridge == null or importing:
		return
	for path in paths:
		if not imported_paths.has(path): imported_paths.append(path)
	status.text = "Inspecting %d files…" % imported_paths.size()
	open_button.disabled = true
	var batch_json := JSON.stringify(imported_paths)
	importing = bridge.start_import(batch_json)
	if not importing:
		open_button.disabled = false
		status.text = "An import is already running."

func _process(_delta: float) -> void:
	if importing:
		var response: String = bridge.poll_import()
		if response.is_empty(): return
		var result = JSON.parse_string(response)
		importing = false
		open_button.disabled = false
		apply_library(result)

func apply_library(result) -> void:
	if not result is Dictionary or not result.has("groups"):
		status.text = "Import failed: invalid native response"
		return
	# The worker reindexes the session's source paths; no input file is modified.
	library = result
	route_document = {}
	rides.clear()
	for group in library.groups:
		var preferred: Dictionary = group.sources[int(group.preferred_source)]
		rides.add_item(preferred.source_name.get_basename().replace("_kurt_", " · ").replace("_", " ") + "  [%d]" % group.sources.size())
	status.text = "%d rides • %d import errors • original files unchanged" % [library.groups.size(), library.errors.size()]
	if not library.groups.is_empty():
		rides.select(0)
		_select_group(0)
	else:
		activity = {}
		sources.clear()
		metrics.text = "No activities imported"
		details.text = "No sample selected"
		findings.clear()
		export_button.disabled = true
		map_view.configure([], 0, 0, 0)
		elevation.configure([], 0, 0, 0)
	for error in library.errors:
		findings.add_item("IMPORT ERROR: " + str(error))
		findings.set_item_tooltip(findings.item_count-1, str(error))

func _select_group(index: int) -> void:
	sources.clear()
	var group: Dictionary = library.groups[index]
	for source in group.sources:
		sources.add_item(source.format + " · " + source.source_name)
	sources.select(int(group.preferred_source))
	_select_source(int(group.preferred_source))

func _select_source(index: int) -> void:
	var selected := rides.get_selected_items()
	if selected.is_empty(): return
	route_document = {}
	activity = library.groups[selected[0]].sources[index]
	_show_activity()

func _show_activity() -> void:
	updating = true
	var last: int = activity.samples.size()-1
	start_sample.max_value = last
	end_sample.max_value = last
	start_sample.value = 0
	end_sample.value = last
	cursor.max_value = last
	cursor.value = 0
	metrics.text = "Recorded: %s km    GPS: %.3f km    %d samples    %d laps" % [_value(activity.get("recorded_distance_m"), 1000.0), float(activity.route_distance_m)/1000.0, activity.samples.size(), activity.laps.size()]
	findings.clear()
	for finding in activity.diagnostics:
		var text: String = ("#%d · " % int(finding.sample) if finding.sample != null else "") + finding.message
		findings.add_item(text)
		findings.set_item_metadata(findings.item_count-1, finding.sample)
		findings.set_item_tooltip(findings.item_count-1, text)
	updating = false
	_range_changed(0)
	_cursor_changed(0)

func _value(value, divisor := 1.0) -> String:
	return "—" if value == null else "%.3f" % (float(value)/divisor)

func _pick_sample(index: int) -> void:
	cursor.value = index

func _cursor_changed(value: float) -> void:
	if updating or activity.is_empty(): return
	var i := int(value)
	var s: Dictionary = activity.samples[i]
	var timestamp := "No timestamp"
	if s.get("time_ms") != null:
		timestamp = Time.get_datetime_string_from_unix_time(int(float(s.time_ms)/1000.0)) + " UTC"
	details.text = "#%d  ·  %s  ·  lap %d / segment %d\nElevation %s m  ·  HR %s bpm  ·  cadence %s rpm  ·  power %s W  ·  speed %s m/s\nElapsed %s s  ·  recorded timer %s s  ·  gaps excluded from GPS distance" % [i, timestamp, int(s.get("lap",0))+1, int(s.get("segment",0))+1, _value(s.get("elevation_m")), _value(s.get("heart_rate_bpm")), _value(s.get("cadence_rpm")), _value(s.get("power_w")), _value(s.get("speed_mps")), _value(activity.get("elapsed_s")), _value(activity.get("timer_s"))]
	_redraw_plots()

func _range_changed(_value: float) -> void:
	if updating: return
	export_button.disabled = bridge == null or activity.is_empty() or start_sample.value >= end_sample.value or not route_document.is_empty()
	_redraw_plots()

func _redraw_plots() -> void:
	if activity.is_empty(): return
	map_view.configure(activity.samples, int(start_sample.value), int(end_sample.value), int(cursor.value))
	elevation.configure(activity.samples, int(start_sample.value), int(end_sample.value), int(cursor.value))

func _export_dialog() -> void:
	export_dialog.current_file = "route.orroute"
	export_dialog.popup_centered_ratio(0.75)

func export_selection(path: String) -> void:
	if bridge == null or activity.is_empty(): return
	var result = JSON.parse_string(bridge.export_route(activity.source_sha256, int(start_sample.value), int(end_sample.value), path))
	status.text = result.message

func open_route(path: String) -> void:
	if bridge == null: return
	var result = JSON.parse_string(bridge.open_route(path))
	if not result.ok:
		status.text = result.message
		return
	route_document = result.route
	rides.deselect_all()
	sources.clear()
	var samples: Array = []
	var segment := 0
	for p in route_document.points:
		if p.break_before: segment += 1
		samples.append({"latitude": p.latitude, "longitude": p.longitude, "elevation_m":p.elevation_m, "route_distance_m":p.distance_m, "break_before":p.break_before, "segment":segment-1})
	activity = {"samples":samples, "laps":[], "route_distance_m":samples[-1].route_distance_m, "diagnostics":[{"sample":null,"message":route_document.elevation_provenance}]}
	_show_activity()
	status.text = "Opened route: %s • source samples %d–%d • read-only preview" % [route_document.name, int(route_document.source_range[0]), int(route_document.source_range[1])]

func _open_scenery(path:String,training:bool=false) -> void:
	if is_instance_valid(scenery_window) and scenery_window.active_ride(): shell.show_ride_menu(); return
	if is_instance_valid(scenery_window): scenery_window.queue_free()
	scenery_window = preload("res://world_screen.gd").new()
	scenery_window.title = "Ride" if training else "Route preview"
	scenery_window.training = training
	shell.register(scenery_window,scenery_window.title)
	var runtime = preload("res://scenery/stream_preview.gd").new()
	runtime.root_path = path; runtime.riding_view = true; runtime.training_entry = training
	runtime.export_requested.connect(func(journal: String):
		activity_export.select_session(journal); shell.navigate(activity_export))
	scenery_window.set_content(runtime)
	shell.show_world(scenery_window)

func _verify_content() -> void:
	content_library.refresh()
	var valid:=0
	var invalid:=0
	var first:=-1
	for i in range(content_library.entries.size()):
		if content_library.entries[i].has("error"):invalid+=1
		else:
			valid+=1
			if first<0:first=i
	if first<0:
		push_error("No valid library package for verification");get_tree().quit(1);return
	content_library.listing.select(first)
	print("CONTENT_LIBRARY_OK valid=",valid," invalid=",invalid)
	content_library.open_selected(not OS.get_cmdline_user_args().has("--verify-ride-entry"))
