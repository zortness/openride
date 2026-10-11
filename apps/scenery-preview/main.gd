extends Node3D
signal project_corrections_saved(document: Dictionary)
var managed_project := false

var camera: Camera3D
var road: MeshInstance3D
var marker: MeshInstance3D
var info: Label
var slider: HSlider
var repair_feedback: Label
var last_selected_source := 0
var retained_markers: MultiMeshInstance3D
var picker: FileDialog
var data: Dictionary = {}
var center := Vector3.ZERO
var radius := 600.0
var yaw := 0.5
var pitch := 0.8
var automated := false
var terrain_view: MeshInstance3D
var warning: Label
var osm_view: MeshInstance3D
var attribution: Label
var correction_road: MeshInstance3D
var road_choice: OptionButton
var height_offset: SpinBox
var correction_status: Label
var correction_picker: FileDialog
var edits: Dictionary = {}
var preparation_hash := ""
var draft_points: Array = []
var propagation_undo: Dictionary = {}
var undo_propagation: Button
var repaired_road: MeshInstance3D
var repair_toggle: CheckButton
var repair_info: Label
var jitter_review: VBoxContainer
var active_repair: Dictionary = {}
var inspector: PanelContainer
var inspector_tabs: TabContainer
var inspector_button: Button
var point_summary: Label
var package_library: AcceptDialog
var manual_repair: VBoxContainer
var route_geometry: MeshInstance3D
var route_geometry_toggle: CheckButton
var route_geometry_info: Label
var traversal = preload("res://route_traversal.gd").new()
var ride_mode: CheckButton
var play_route: Button
var preview_speed: SpinBox
var follow_route: CheckButton
var saved_source_index := 0.0
var embedded := false

func _ready() -> void:
	camera = Camera3D.new()
	camera.far = 20000.0
	add_child(camera)
	road = MeshInstance3D.new()
	add_child(road)
	correction_road = MeshInstance3D.new()
	correction_road.material_override = _material(Color("ef83d2"))
	correction_road.material_override.no_depth_test = true
	add_child(correction_road)
	repaired_road = MeshInstance3D.new()
	retained_markers = MultiMeshInstance3D.new()
	add_child(retained_markers)
	repaired_road.material_override = _material(Color("ffd65b"))
	repaired_road.material_override.no_depth_test = true
	add_child(repaired_road)
	route_geometry = MeshInstance3D.new()
	route_geometry.material_override = _material(Color("78aaff"))
	route_geometry.material_override.no_depth_test = true
	add_child(route_geometry)
	terrain_view = MeshInstance3D.new()
	add_child(terrain_view)
	osm_view = MeshInstance3D.new()
	add_child(osm_view)
	var light := DirectionalLight3D.new()
	light.rotation_degrees = Vector3(-50, -35, 0)
	light.light_energy = 1.2
	add_child(light)
	var environment := WorldEnvironment.new()
	environment.environment = Environment.new()
	environment.environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.environment.ambient_light_color = Color("a1b7cd")
	environment.environment.ambient_light_energy = 0.5
	add_child(environment)
	marker = MeshInstance3D.new()
	var ball := SphereMesh.new()
	ball.radius = 8.0
	ball.height = 16.0
	marker.mesh = ball
	marker.material_override = _material(Color("ffc36a"))
	marker.visible = false
	add_child(marker)
	var layer := CanvasLayer.new()
	add_child(layer)
	var overlay_theme := Theme.new()
	overlay_theme.default_font_size = 14
	var panel := PanelContainer.new()
	panel.theme = overlay_theme
	panel.set_anchors_and_offsets_preset(Control.PRESET_TOP_WIDE)
	layer.add_child(panel)
	var box := VBoxContainer.new()
	panel.add_child(box)
	var title := Label.new()
	title.text = "  OPENRIDE / Scenery preparation"
	title.add_theme_font_size_override("font_size", 25)
	box.add_child(title)
	warning = Label.new()
	warning.text = "  ROAD PROFILE PREVIEW · Unvalidated activity elevation · No terrain or trainer connection"
	warning.modulate = Color("ffc36a")
	box.add_child(warning)
	var button := Button.new()
	button.text = "Open preparation.json…"
	button.pressed.connect(func(): picker.popup_centered_ratio(0.7))
	box.add_child(button)
	var toggles := VBoxContainer.new()
	box.add_child(toggles)
	var terrain_toggle := CheckButton.new()
	terrain_toggle.text = "Terrain"
	terrain_toggle.button_pressed = true
	terrain_toggle.toggled.connect(func(enabled: bool): terrain_view.visible = enabled)
	toggles.add_child(terrain_toggle)
	var overlay := CheckButton.new()
	overlay.text = "Road overlay (ignores terrain occlusion)"
	overlay.button_pressed = true
	overlay.toggled.connect(func(enabled: bool):
		if road.material_override != null:
			road.material_override.no_depth_test = enabled
		correction_road.material_override.no_depth_test = enabled
		repaired_road.material_override.no_depth_test = enabled
		route_geometry.material_override.no_depth_test = enabled
	)
	toggles.add_child(overlay)
	road.set_meta("overlay_button", overlay)
	var osm_toggle := CheckButton.new()
	osm_toggle.text = "OSM road context"
	osm_toggle.button_pressed = true
	osm_toggle.toggled.connect(func(enabled: bool): osm_view.visible = enabled)
	toggles.add_child(osm_toggle)
	attribution = Label.new()
	attribution.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_WIDE)
	attribution.position.y -= 28
	attribution.mouse_filter = Control.MOUSE_FILTER_IGNORE
	layer.add_child(attribution)
	info = Label.new()
	info.text = "  Open a builder output. Drag the background to orbit; mouse wheel to zoom."
	box.add_child(info)
	slider = HSlider.new()
	slider.step = 1.0
	slider.value_changed.connect(_select_point)
	box.add_child(slider)
	var edit_row := VBoxContainer.new()
	box.add_child(edit_row)
	road_choice = OptionButton.new()
	road_choice.fit_to_longest_item = false
	road_choice.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	road_choice.item_selected.connect(_choose_road)
	edit_row.add_child(road_choice)
	var missing_button := Button.new()
	missing_button.text = "Next original-position point"
	missing_button.pressed.connect(_next_missing_choice)
	edit_row.add_child(missing_button)
	var offset_label := Label.new()
	offset_label.text = "Elevation offset (m):"
	edit_row.add_child(offset_label)
	height_offset = SpinBox.new()
	height_offset.min_value = -100
	height_offset.max_value = 100
	height_offset.step = 0.1
	height_offset.value_changed.connect(func(_v: float): _refresh_corrections(); _select_point(slider.value))
	edit_row.add_child(height_offset)
	for action in ["Save corrections", "Load corrections"]:
		if managed_project and action == "Load corrections": continue
		var edit_button := Button.new()
		edit_button.text = "Save route edits" if managed_project and action == "Save corrections" else action
		edit_button.pressed.connect(func():
			if data.is_empty(): return
			if managed_project:
				project_corrections_saved.emit({"schema_version":1,"kind":"openride.scenery-corrections","preparation_sha256":preparation_hash,"elevation_offset_m":height_offset.value,"road_choices":edits.values()})
				return
			correction_picker.file_mode = FileDialog.FILE_MODE_SAVE_FILE if action == "Save corrections" else FileDialog.FILE_MODE_OPEN_FILE
			correction_picker.popup_centered_ratio(0.7))
		edit_row.add_child(edit_button)
	correction_status = Label.new()
	correction_status.text = "  Cyan: source / Pink: edited draft. Changes are unvalidated; no trainer connection."
	box.add_child(correction_status)
	var propagation_row := VBoxContainer.new()
	box.add_child(propagation_row)
	var propagate := Button.new()
	propagate.text = "Extend road selection both ways"
	propagate.tooltip_text = "Fill consecutive points with one matching road candidate. Stops at breaks, gaps, ambiguities or conflicting choices."
	propagate.pressed.connect(_propagate_road)
	propagation_row.add_child(propagate)
	undo_propagation = Button.new()
	undo_propagation.text = "Undo extension"
	undo_propagation.disabled = true
	undo_propagation.pressed.connect(func():
		edits = propagation_undo.duplicate(true)
		undo_propagation.disabled = true
		_refresh_corrections()
		_select_point(slider.value)
		correction_status.text = "  Extension undone. Save to a new file to retain changes.")
	propagation_row.add_child(undo_propagation)
	repair_toggle = CheckButton.new()
	repair_toggle.text = "Show repair proposal (gold)"
	repair_toggle.disabled = true
	repair_toggle.toggled.connect(func(enabled: bool):
		repaired_road.visible = enabled
		road.visible = not enabled
		retained_markers.visible = enabled
		correction_road.visible = not enabled and (not edits.is_empty() or height_offset.value != 0.0)
		_select_point(slider.value))
	propagation_row.add_child(repair_toggle)
	var inspect_repair := Button.new()
	inspect_repair.text = "Inspect repair"
	inspect_repair.pressed.connect(func():
		if repair_toggle.disabled: return
		repair_toggle.button_pressed = true
		var span: Dictionary = active_repair.repairs[0]
		for index in range(data.points.size()):
			if data.points[index].source_sample == span.start_source_sample:
				slider.value = index
				var p: Array = draft_points[index].position
				center = Vector3(p[0],p[1],p[2])
				radius = 65.0
				_update_camera()
				break)
	propagation_row.add_child(inspect_repair)
	repair_info = Label.new()
	box.add_child(repair_info)
	jitter_review = preload("res://jitter_review.gd").new()
	jitter_review.managed_project = managed_project
	box.add_child(jitter_review)
	jitter_review.preview_requested.connect(_show_repair)
	manual_repair = preload("res://manual_repair.gd").new()
	manual_repair.managed_project = managed_project
	box.add_child(manual_repair)
	manual_repair.preview_requested.connect(_show_repair)
	manual_repair.preview_requested.connect(func(_profile: Dictionary):
		if managed_project: jitter_review.invalidate())
	manual_repair.remove_current_requested.connect(_remove_current_point)
	manual_repair.action_feedback.connect(func(message: String): repair_feedback.text = message)
	# Keep authoring tools off the map until the user opens the inspector.
	inspector = panel
	panel.set_anchors_and_offsets_preset(Control.PRESET_RIGHT_WIDE)
	panel.offset_left = -400
	panel.offset_right = -12
	panel.offset_top = 60
	panel.offset_bottom = -100
	inspector_tabs = TabContainer.new()
	panel.add_child(inspector_tabs)
	var details_page := _inspector_page("Details")
	var edits_page := _inspector_page("Road edits")
	route_geometry_toggle = CheckButton.new()
	route_geometry_toggle.text = "Generated route geometry (blue)"
	route_geometry_toggle.disabled = true
	route_geometry_toggle.toggled.connect(func(enabled: bool):
		route_geometry.visible = enabled
		correction_road.visible = not enabled and (not edits.is_empty() or height_offset.value != 0.0)
		if enabled: repair_toggle.button_pressed = false)
	edits_page.add_child(route_geometry_toggle)
	route_geometry_info = Label.new()
	route_geometry_info.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	edits_page.add_child(route_geometry_info)
	var speed_label := Label.new()
	speed_label.text = "Preview speed (km/h)"
	edits_page.add_child(speed_label)
	preview_speed = SpinBox.new()
	preview_speed.min_value = 1
	preview_speed.max_value = 60
	preview_speed.value = 25
	edits_page.add_child(preview_speed)
	follow_route = CheckButton.new()
	follow_route.text = "Follow route preview camera"
	follow_route.toggled.connect(func(enabled: bool):
		if enabled and ride_mode.button_pressed:
			radius = 90
			_seek_route(traversal.distance_m))
	edits_page.add_child(follow_route)
	var repairs_page := _inspector_page("Repairs")
	for control in [warning, info, toggles]: control.reparent(details_page)
	for control in [edit_row, propagate, undo_propagation, correction_status]: control.reparent(edits_page)
	for control in [repair_toggle, inspect_repair, repair_info, manual_repair, jitter_review]: control.reparent(repairs_page)
	for label in [warning, info, correction_status, repair_info]:
		label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	var toolbar := HBoxContainer.new()
	toolbar.theme = overlay_theme
	toolbar.set_anchors_and_offsets_preset(Control.PRESET_TOP_WIDE)
	toolbar.offset_left = 12
	toolbar.offset_right = -12
	toolbar.offset_top = 10
	toolbar.add_theme_constant_override("separation", 12)
	layer.add_child(toolbar)
	title.reparent(toolbar)
	title.text = "OPENRIDE  /  Route preview"
	title.add_theme_font_size_override("font_size", 18)
	title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	button.reparent(toolbar)
	button.text = "Open route"
	button.custom_minimum_size.y = 40
	inspector_button = Button.new()
	inspector_button.text = "Show tools"
	inspector_button.custom_minimum_size = Vector2(120,40)
	inspector_button.toggle_mode = true
	inspector_button.toggled.connect(func(show: bool):
		panel.visible = show
		inspector_button.text = "Hide tools" if show else "Show tools")
	toolbar.add_child(inspector_button)
	package_library = preload("res://package_library.gd").new()
	package_library.theme = overlay_theme
	package_library.package_selected.connect(load_preparation)
	add_child(package_library)
	var library_button := Button.new()
	library_button.text = "Library"
	library_button.pressed.connect(func(): package_library.refresh(); package_library.popup_centered(Vector2i(680,440)))
	toolbar.add_child(library_button)
	panel.visible = false
	var navigation := VBoxContainer.new()
	navigation.theme = overlay_theme
	navigation.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_WIDE)
	navigation.offset_left = 16
	navigation.offset_right = -16
	navigation.offset_top = -88
	navigation.offset_bottom = -34
	layer.add_child(navigation)
	point_summary = Label.new()
	point_summary.text = "Drag to orbit · Scroll to zoom · Preview only"
	var navigation_row := HBoxContainer.new()
	navigation.add_child(navigation_row)
	ride_mode = CheckButton.new()
	ride_mode.text = "Route distance"
	ride_mode.disabled = true
	ride_mode.toggled.connect(_set_ride_mode)
	navigation_row.add_child(ride_mode)
	play_route = Button.new()
	play_route.text = "Play"
	play_route.disabled = true
	play_route.pressed.connect(func():
		if not ride_mode.button_pressed: ride_mode.button_pressed = true
		if traversal.distance_m >= traversal.length_m: _seek_route(0)
		traversal.playing = not traversal.playing
		play_route.text = "Pause" if traversal.playing else "Play")
	navigation_row.add_child(play_route)
	var focus_button := Button.new()
	focus_button.text = "Focus point (F)"
	focus_button.pressed.connect(_focus_point)
	navigation_row.add_child(focus_button)
	var camera_help := Label.new()
	camera_help.text = "Shift-drag / middle-drag: pan"
	camera_help.add_theme_font_size_override("font_size", 12)
	navigation_row.add_child(camera_help)
	point_summary.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	point_summary.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	navigation_row.add_child(point_summary)
	repair_feedback = Label.new()
	repair_feedback.add_theme_font_size_override("font_size", 13)
	repair_feedback.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	navigation.add_child(repair_feedback)
	slider.reparent(navigation)
	slider.custom_minimum_size.y = 28
	box.queue_free()
	attribution.add_theme_font_size_override("font_size", 12)
	attribution.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	attribution.offset_top = -22
	attribution.offset_bottom = -2
	correction_picker = FileDialog.new(); correction_picker.display_mode = FileDialog.DISPLAY_LIST
	correction_picker.access = FileDialog.ACCESS_FILESYSTEM
	correction_picker.filters = PackedStringArray(["*.json ; Scenery corrections"])
	correction_picker.file_selected.connect(_correction_file)
	add_child(correction_picker)
	picker = FileDialog.new(); picker.display_mode = FileDialog.DISPLAY_LIST
	picker.access = FileDialog.ACCESS_FILESYSTEM
	picker.file_mode = FileDialog.FILE_MODE_OPEN_FILE
	picker.filters = PackedStringArray(["*.json ; Scenery preparation"])
	picker.file_selected.connect(load_preparation)
	add_child(picker)
	if embedded: return
	var input_path := ""
	var snapshot := ""
	var package_path := ""
	for argument in OS.get_cmdline_user_args():
		if argument.begins_with("--content-directory="):
			package_library.directory = argument.trim_prefix("--content-directory=")
		if argument.begins_with("--package="):
			package_path = argument.trim_prefix("--package=")
		if argument.begins_with("--preparation="):
			input_path = argument.trim_prefix("--preparation=")
		if argument.begins_with("--snapshot="):
			snapshot = argument.trim_prefix("--snapshot=")
	automated = OS.get_cmdline_user_args().has("--verify") or not snapshot.is_empty()
	if not package_path.is_empty():
		var package_result: Dictionary = package_library.validate(package_path)
		if package_result.has("error"):
			_fail(package_result.error)
			return
		input_path = package_result.path
		print("PACKAGE_LOAD_OK ", package_result.name, " / ", package_result.version)
	if OS.get_cmdline_user_args().has("--show-library"):
		package_library.refresh()
		package_library.popup_centered(Vector2i(680,440))
	if not input_path.is_empty():
		load_preparation(input_path)
	if OS.get_cmdline_user_args().has("--verify-library"):
		package_library.refresh()
		if package_library.entries.is_empty() or package_library.entries[0].has("error"):
			_fail("Expected valid package in test library")
			return
		package_library.listing.select(0)
		package_library.confirmed.emit()
		if data.is_empty():
			_fail("Library selection failed to load package")
			return
		print("LIBRARY_DISCOVERY_SELECTION_OK")
	for argument in OS.get_cmdline_user_args():
		if argument.begins_with("--verify-manual-save="):
			manual_repair.first.value = 5
			manual_repair.last.value = 31
			manual_repair.preview()
			var manual_path := argument.trim_prefix("--verify-manual-save=")
			manual_repair.save(manual_path)
			if active_repair.get("points", []).size() != 54 or not FileAccess.file_exists(manual_path):
				_fail("Manual remove/rejoin failed")
				return
			print("MANUAL_REPAIR_PREVIEW_SAVE_OK removed=27 retained=54")
		if argument.begins_with("--jitter-review="):
			jitter_review.load_report(argument.trim_prefix("--jitter-review="))
		if argument.begins_with("--verify-jitter-save="):
			if jitter_review.report.get("candidates", []).is_empty():
				_fail("No jitter candidates loaded")
				return
			jitter_review.candidates.select(0)
			jitter_review.preview_selected()
			var save_path := argument.trim_prefix("--verify-jitter-save=")
			jitter_review.save_selected(save_path)
			if repaired_road.mesh == null or not FileAccess.file_exists(save_path):
				_fail("Jitter preview/save failed")
				return
			print("JITTER_REVIEW_PREVIEW_SAVE_OK")
	if automated and not data.is_empty():
		for argument in OS.get_cmdline_user_args():
			if argument.begins_with("--show-tools="):
				inspector_button.button_pressed = true
				inspector_tabs.current_tab = int(argument.trim_prefix("--show-tools="))
		if data.has("corrected_profile"):
			for index in range(draft_points.size()):
				var expected: Dictionary = data.corrected_profile.points[index]
				var actual: Dictionary = draft_points[index]
				if (expected.grade_percent == null) != (actual.grade_percent == null):
					_fail("Generated profile grade availability mismatch")
					return
				if expected.grade_percent != null and absf(expected.grade_percent - actual.grade_percent) > 0.001:
					_fail("Generated profile grade differs from preview")
					return
			print("CORRECTED_PROFILE_PARITY_OK points=", draft_points.size())
		if data.has("reviewed_profile"):
			if repaired_road.mesh == null:
				_fail("Missing repair proposal geometry")
				return
			print("REPAIR_PREVIEW_OK points=", data.reviewed_profile.points.size(), " findings=", data.reviewed_profile.findings.size())
		if data.has("route_profile"):
			if route_geometry.mesh == null:
				_fail("Generated route mesh missing")
				return
			print("ROUTE_GEOMETRY_OK mode=",data.route_profile.mode," points=",data.route_profile.points.size())
			if OS.get_cmdline_user_args().has("--verify-traversal"):
				if ride_mode.disabled:
					_fail("Road traversal unavailable")
					return
				ride_mode.button_pressed = true
				traversal.playing = true
				traversal.advance(10,36)
				_seek_route(traversal.distance_m)
				if absf(traversal.distance_m-100)>0.001:
					_fail("Traversal distance mismatch")
					return
				traversal.playing = false
				print("ROAD_TRAVERSAL_UI_OK distance=100m")
		slider.value = floor(slider.max_value / 2.0)
		for argument in OS.get_cmdline_user_args():
			if argument == "--verify-propagation":
				edits.clear()
				_choose_road(1)
				var seed_edits := edits.duplicate(true)
				_propagate_road()
				if edits.size() <= 1 or undo_propagation.disabled:
					_fail("Propagation did not extend the seed")
					return
				undo_propagation.pressed.emit()
				if edits != seed_edits:
					_fail("Propagation undo did not restore seed")
					return
				print("PROPAGATION_UI_UNDO_OK")
			if argument.begins_with("--verify-corrections="):
				var correction_path := argument.trim_prefix("--verify-corrections=")
				if road_choice.item_count < 2 or road_choice.is_item_disabled(1):
					_fail("Verification needs projected road candidates")
					return
				road_choice.select(1)
				road_choice.item_selected.emit(1)
				height_offset.value = 10.0
				correction_picker.file_mode = FileDialog.FILE_MODE_SAVE_FILE
				_correction_file(correction_path)
				edits.clear()
				height_offset.value = 0.0
				correction_picker.file_mode = FileDialog.FILE_MODE_OPEN_FILE
				_correction_file(correction_path)
				if edits.size() != 1 or height_offset.value != 10.0 or correction_road.mesh == null:
					_fail("Correction save/load verification failed")
					return
				print("CORRECTIONS_ROUNDTRIP_OK choices=1 offset=10")
		await get_tree().process_frame
		if not snapshot.is_empty():
			await RenderingServer.frame_post_draw
			var error := get_viewport().get_texture().get_image().save_png(snapshot)
			if error != OK:
				_fail("Screenshot failed")
				return
		print("SCENERY_PREVIEW_OK points=", data.points.size(), " triangles=", data.preview_mesh.indices.size() / 3)
		get_tree().quit()
	elif automated:
		_fail("No preparation loaded")
	_update_camera()

func _material(color: Color) -> StandardMaterial3D:
	var material := StandardMaterial3D.new()
	material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	material.albedo_color = color
	material.cull_mode = BaseMaterial3D.CULL_DISABLED
	return material

func _inspector_page(page_name: String) -> VBoxContainer:
	var scroll := ScrollContainer.new()
	scroll.name = page_name
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	inspector_tabs.add_child(scroll)
	var margin := MarginContainer.new()
	margin.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	for edge in ["left", "right", "top", "bottom"]:
		margin.add_theme_constant_override("margin_" + edge, 12)
	scroll.add_child(margin)
	var contents := VBoxContainer.new()
	contents.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	contents.add_theme_constant_override("separation", 10)
	margin.add_child(contents)
	return contents

func _fail(message: String) -> void:
	info.text = "  " + message
	if point_summary != null:
		point_summary.text = message
	if automated:
		push_error(message)
		get_tree().quit(1)

func load_preparation(path: String) -> void:
	var file := FileAccess.open(path, FileAccess.READ)
	if file == null or file.get_length() > 16 * 1024 * 1024:
		_fail("Cannot open preparation (limit 16 MiB).")
		return
	var parsed: Variant = JSON.parse_string(file.get_as_text())
	if not parsed is Dictionary or parsed.get("kind") != "openride.scenery-preparation" or parsed.get("schema_version") != 1 or parsed.get("trainer_ready") != false:
		_fail("Unsupported scenery preparation file.")
		return
	var mesh_data: Variant = parsed.get("preview_mesh")
	if not mesh_data is Dictionary or not mesh_data.get("vertices") is Array or not mesh_data.get("indices") is Array or not parsed.get("points") is Array:
		_fail("Missing preview geometry or profile.")
		return
	if mesh_data.vertices.size() > 100000 or mesh_data.indices.size() > 150000 or mesh_data.indices.size() % 3 != 0 or parsed.points.size() < 2:
		_fail("Invalid preview geometry size.")
		return
	var vertices := PackedVector3Array()
	for value in mesh_data.vertices:
		if not _valid_position(value):
			_fail("Invalid preview vertex.")
			return
		vertices.append(Vector3(value[0], value[1], value[2]))
	var indices := PackedInt32Array()
	for index in mesh_data.indices:
		if not (index is float or index is int) or not is_finite(float(index)) or index != floor(index) or index < 0 or index >= vertices.size():
			_fail("Invalid triangle index.")
			return
		indices.append(int(index))
	for point in parsed.points:
		if not point is Dictionary or not point.get("position") is Array or point.position.size() != 3 or not (point.get("distance_m") is float or point.get("distance_m") is int):
			_fail("Invalid profile point.")
			return
		if point.position[1] != null and not _valid_position(point.position):
			_fail("Invalid profile coordinates.")
			return
	var arrays := []
	arrays.resize(Mesh.ARRAY_MAX)
	arrays[Mesh.ARRAY_VERTEX] = vertices
	arrays[Mesh.ARRAY_INDEX] = indices
	var mesh := ArrayMesh.new()
	if not indices.is_empty(): mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
	elif parsed.has("terrain"):
		# Display-only drape for KML without heights. Original nulls remain null.
		var outline := PackedVector3Array()
		var comparison: Array = parsed.terrain.get("comparison", [])
		for i in range(1, mini(parsed.points.size(), comparison.size())):
			if parsed.points[i].break_before or comparison[i].get("terrain_elevation_m") == null or comparison[i-1].get("terrain_elevation_m") == null: continue
			for j in [i-1, i]:
				var p: Array = parsed.points[j].position
				outline.append(Vector3(p[0], float(comparison[j].terrain_elevation_m) - float(parsed.origin_lat_lon_elevation[2]) + 0.5, p[2]))
		if not outline.is_empty():
			var line_arrays := []; line_arrays.resize(Mesh.ARRAY_MAX); line_arrays[Mesh.ARRAY_VERTEX] = outline
			mesh.add_surface_from_arrays(Mesh.PRIMITIVE_LINES, line_arrays)
	road.mesh = mesh
	road.material_override = _material(Color("6fddcb"))
	road.material_override.no_depth_test = road.get_meta("overlay_button").button_pressed
	terrain_view.mesh = null
	osm_view.mesh = null
	attribution.text = ""
	warning.text = "  ROAD PROFILE PREVIEW · Unvalidated activity elevation · No terrain or trainer connection"
	if parsed.has("terrain"):
		var terrain_mesh := _terrain_mesh(parsed.terrain)
		if terrain_mesh == null:
			_fail("Invalid terrain geometry.")
			return
		terrain_view.mesh = terrain_mesh
		var ground := StandardMaterial3D.new()
		ground.albedo_color = Color("506b55")
		ground.roughness = 1.0
		terrain_view.material_override = ground
		warning.text = "  TERRAIN CONTEXT · Activity elevation unchanged / vertical reference unverified · No trainer connection"
		if parsed.terrain.get("source_snapshot", {}).get("synthetic_fixture", false):
			warning.text = "  SYNTHETIC TEST TERRAIN · Pipeline verification only · Not real geography"
		else:
			attribution.text = "  Terrain: USGS 3DEP"
	if parsed.has("osm_context"):
		var context: Variant = parsed.osm_context
		if not context is Dictionary or context.get("kind") != "openride.osm-context" or context.get("schema_version") != 1:
			_fail("Unsupported OSM context.")
			return
		var lines: Variant = context.get("display_lines", {}).get("vertices", [])
		if not lines is Array or lines.size() > 200000 or lines.size() % 2 != 0:
			_fail("Invalid OSM line geometry.")
			return
		var line_vertices := PackedVector3Array()
		for position in lines:
			if not _valid_position(position):
				_fail("Invalid OSM coordinate.")
				return
			line_vertices.append(Vector3(position[0], position[1], position[2]))
		if not line_vertices.is_empty():
			var line_arrays := []
			line_arrays.resize(Mesh.ARRAY_MAX)
			line_arrays[Mesh.ARRAY_VERTEX] = line_vertices
			var line_mesh := ArrayMesh.new()
			line_mesh.add_surface_from_arrays(Mesh.PRIMITIVE_LINES, line_arrays)
			osm_view.mesh = line_mesh
			osm_view.material_override = _material(Color("dca959"))
		attribution.text += "  |  OSM: © OpenStreetMap contributors · ODbL 1.0 · openstreetmap.org/copyright"
	var bounds := mesh.get_aabb() if not vertices.is_empty() else terrain_view.get_aabb()
	center = bounds.get_center()
	radius = maxf(60.0, bounds.size.length() * 0.9)
	data = parsed
	preparation_hash = parsed.get("corrections", {}).get("preparation_sha256", FileAccess.get_sha256(path))
	edits.clear()
	undo_propagation.disabled = true
	for choice in parsed.get("corrections", {}).get("road_choices", []):
		edits[str(int(choice.source_sample))] = choice
	height_offset.set_value_no_signal(parsed.get("corrections", {}).get("elevation_offset_m", 0.0))
	_refresh_corrections()
	jitter_review.expected_hash = FileAccess.get_sha256(path)
	jitter_review.source_points = data.points
	manual_repair.configure(data,FileAccess.get_sha256(path))
	if parsed.has("reviewed_profile"):
		_show_repair(parsed.reviewed_profile)
	if parsed.has("route_profile"):
		var route_mesh := _terrain_mesh(parsed.route_profile)
		if route_mesh == null:
			_fail("Invalid generated route geometry")
			return
		route_geometry.mesh = route_mesh
		route_geometry_toggle.disabled = false
		route_geometry_toggle.button_pressed = true
		route_geometry_info.text = "%s mode · %d generated points · %.1f m · %d findings. Cyan: original; slider inspects source points." % [parsed.route_profile.mode,parsed.route_profile.points.size(),parsed.route_profile.points[-1].distance_m,parsed.route_profile.findings.size()]
		for gap in parsed.route_profile.get("road_gap_connectors", []):
			route_geometry_info.text += "\nStraight join: points %d–%d / %.1f m" % [gap.start_point,gap.end_point,gap.length_m]
		if not parsed.route_profile.get("fallback_reasons", []).is_empty():
			route_geometry_info.text += "\nGPS fallback: " + "; ".join(parsed.route_profile.fallback_reasons)
		var traversal_error: String = traversal.configure(parsed.route_profile)
		ride_mode.disabled = not traversal_error.is_empty()
		play_route.disabled = ride_mode.disabled
		if not traversal_error.is_empty(): route_geometry_info.text += "\n" + traversal_error
	slider.max_value = data.points.size() - 1
	slider.value = 0
	_select_point(0)
	_update_camera()

func _valid_position(value: Variant) -> bool:
	if not value is Array or value.size() != 3:
		return false
	for number in value:
		if not (number is float or number is int) or not is_finite(float(number)) or absf(float(number)) > 100000.0:
			return false
	return true

func _select_point(value: float) -> void:
	if data.is_empty():
		return
	if ride_mode.button_pressed:
		_seek_route(value)
		return
	if repair_toggle.button_pressed and not active_repair.is_empty():
		var retained := {}
		for p in active_repair.points: retained[int(p.source_sample)] = true
		var target := int(value)
		var direction := -1 if target < last_selected_source else 1
		while target >= 0 and target < data.points.size() and not retained.has(int(data.points[target].source_sample)):
			target += direction
		if target < 0 or target >= data.points.size():
			target = int(value)
			while target >= 0 and target < data.points.size() and not retained.has(int(data.points[target].source_sample)): target -= direction
			if target < 0 or target >= data.points.size(): return
		value = target
		slider.set_value_no_signal(value)
	last_selected_source = int(value)
	manual_repair.remove_button.text = "Remove slider point %d (Del)" % (int(value)+1)
	var point: Dictionary = data.points[int(value)]
	point_summary.text = "Source point %d / %d  ·  %.0f m  ·  Drag to orbit / scroll to zoom  ·  Preview only" % [int(value)+1,data.points.size(),point.distance_m]
	_populate_choices(int(value))
	marker.visible = point.position[1] != null
	if marker.visible:
		marker.position = Vector3(point.position[0], point.position[1] + 8.0, point.position[2])
	elif data.has("terrain"):
		var comparison: Array = data.terrain.get("comparison", [])
		if int(value) < comparison.size() and comparison[int(value)].get("terrain_elevation_m") != null:
			marker.visible = true
			marker.position = Vector3(point.position[0], float(comparison[int(value)].terrain_elevation_m) - float(data.origin_lat_lon_elevation[2]) + 8.0, point.position[2])
	info.text = "  %.0f m into section · point %d/%d · drag to orbit / wheel to zoom / slider to inspect" % [point.distance_m, int(value) + 1, data.points.size()]
	var height_text := "missing" if point.get("elevation_m") == null else "%.1f m" % point.elevation_m
	var grade_text := "unavailable" if point.get("raw_grade_percent") == null else "%.2f%%" % point.raw_grade_percent
	info.text += "\n  Elevation: %s · raw grade: %s · %d source findings" % [height_text, grade_text, data.get("findings", []).size()]
	if data.has("terrain"):
		var comparison: Array = data.terrain.get("comparison", [])
		if int(value) < comparison.size():
			var sample: Dictionary = comparison[int(value)]
			var terrain_height := "missing" if sample.get("terrain_elevation_m") == null else "%.1f m" % sample.terrain_elevation_m
			var delta := "unavailable" if sample.get("activity_minus_terrain_m") == null else "%+.1f m" % sample.activity_minus_terrain_m
			info.text += "\n  Terrain: %s · activity minus terrain: %s (not a correction)" % [terrain_height, delta]
	if data.has("osm_context"):
		var matches: Array = data.osm_context.get("matches", [])
		if int(value) < matches.size():
			var match_data: Dictionary = matches[int(value)]
			var candidates: Array = match_data.get("candidates", [])
			if candidates.is_empty():
				info.text += "\n  No OSM highway candidate within 30 m"
			else:
				info.text += "\n  Nearby OSM candidates · %s · select in Road edits" % match_data.status
				for index in range(candidates.size()):
					var alternative: Dictionary = candidates[index]
					var road_name: String = alternative.get("name") if alternative.get("name") != null else "Unnamed"
					info.text += "\n    %d. %s (%s) · %.1f m · OSM way %s" % [index + 1, road_name, alternative.highway, alternative.distance_m, alternative.way_id]
	if draft_points.size() == data.points.size():
		var draft: Dictionary = draft_points[int(value)]
		var grade := "unavailable" if draft.grade_percent == null else "%.2f%%" % draft.grade_percent
		info.text += "\n  Edited draft grade: %s · offset: %+.1f m · %d explicit road choices" % [grade, height_offset.value, edits.size()]
		if draft.position[1] != null:
			marker.position = Vector3(draft.position[0], draft.position[1] + 8.0, draft.position[2])
	if repair_toggle.button_pressed and repaired_road.mesh != null:
		point_summary.text = "Repaired route: %d / %d points retained · Source point %d · %.0f m" % [active_repair.points.size(), data.points.size(), int(value)+1, point.distance_m]
		for span in active_repair.repairs:
			if point.source_sample in span.removed_source_samples:
				point_summary.text = "Source point %d / %d · Removed from repair proposal (original retained)" % [int(value)+1,data.points.size()]
				marker.visible = false
			if point.source_sample >= span.start_source_sample and point.source_sample <= span.end_source_sample:
				info.text += "\n  Proposal replaces samples %d–%d with a %.2f m connector; endpoint elevations preserved." % [span.start_source_sample,span.end_source_sample,span.connector_length_m]
				break

func _candidates(index: int) -> Array:
	var matches: Array = data.get("osm_context", {}).get("matches", [])
	return matches[index].get("candidates", []) if index < matches.size() else []

func _next_missing_choice() -> void:
	if data.is_empty(): return
	if ride_mode.button_pressed: ride_mode.button_pressed = false
	var retained := {}
	var review: Dictionary = active_repair if repair_toggle.button_pressed and not active_repair.is_empty() else data.get("reviewed_profile", {})
	for point in review.get("points", data.points): retained[str(int(point.source_sample))] = true
	var missing: Array[int] = []
	for i in range(data.points.size()):
		var sample := str(int(data.points[i].source_sample))
		if retained.has(sample) and not edits.has(sample): missing.append(i)
	if missing.is_empty():
		correction_status.text = "All retained points have road choices. Save corrections, then rebuild."; return
	var target := missing[0]
	for index in missing:
		if index > int(slider.value): target = index; break
	slider.value = target
	_select_point(target)
	_focus_point()
	correction_status.text = "%d points keep their original positions (valid). Source sample %d is selected." % [missing.size(), int(data.points[target].source_sample)]

func _focus_point() -> void:
	if not marker.visible: return
	center = marker.position - Vector3.UP * 8.0
	radius = minf(radius, 90.0)
	_update_camera()

func _pan_camera(motion: Vector2) -> void:
	var metres_per_pixel := 2.0 * radius * tan(deg_to_rad(camera.fov) / 2.0) / maxf(1.0, get_viewport().get_visible_rect().size.y)
	center += (-camera.global_basis.x * motion.x + camera.global_basis.y * motion.y) * metres_per_pixel
	_update_camera()

func _populate_choices(index: int) -> void:
	road_choice.clear()
	road_choice.add_item("Keep source position (this point)")
	var selected: Dictionary = edits.get(str(int(data.points[index].source_sample)), {})
	var candidates := _candidates(index)
	for candidate in candidates:
		road_choice.add_item("%s / %s / way %s" % [candidate.get("name") if candidate.get("name") != null else "Unnamed", candidate.highway, candidate.way_id])
		var item := road_choice.item_count - 1
		road_choice.set_item_disabled(item, not candidate.has("projected_xz"))
		if selected.get("way_id") == candidate.way_id and selected.get("node_ids") == candidate.node_ids:
			road_choice.select(item)

func _choose_road(item: int) -> void:
	undo_propagation.disabled = true
	var index := int(slider.value)
	var sample: int = int(data.points[index].source_sample)
	if item == 0:
		edits.erase(str(sample))
	else:
		var candidate: Dictionary = _candidates(index)[item - 1]
		edits[str(sample)] = {"source_sample": sample, "way_id": candidate.way_id, "node_ids": candidate.node_ids}
	_refresh_corrections()
	_select_point(slider.value)
	correction_status.text = "Unsaved road choice. Save route edits commits road choices and point removals together." if managed_project else "Unsaved draft. Save corrections to keep changes."

func _propagate_road() -> void:
	if data.is_empty() or not data.has("osm_context"): return
	var plan: Dictionary = preload("res://road_propagation.gd").plan(data.points, data.osm_context.matches, edits, int(slider.value))
	if not plan.choices.is_empty():
		propagation_undo = edits.duplicate(true)
		undo_propagation.disabled = false
		for choice in plan.choices:
			edits[str(int(choice.source_sample))] = choice
		_refresh_corrections()
		_select_point(slider.value)
	correction_status.text = "  Extended to %d additional points. %s" % [plan.choices.size(), "; ".join(plan.stops)]

func _refresh_corrections() -> void:
	if data.is_empty(): return
	traversal.playing = false
	if ride_mode.button_pressed: ride_mode.button_pressed = false
	ride_mode.disabled = true
	play_route.disabled = true
	route_geometry.mesh = null
	route_geometry.visible = false
	route_geometry_toggle.set_pressed_no_signal(false)
	route_geometry_toggle.disabled = true
	route_geometry_info.text = "Regenerate route geometry after changing corrections." if data.has("route_profile") else ""
	jitter_review.invalidate()
	if not managed_project: manual_repair.invalidate()
	active_repair = {}
	road.visible = true
	retained_markers.visible = false
	repaired_road.mesh = null
	repaired_road.visible = false
	repair_toggle.set_pressed_no_signal(false)
	repair_toggle.disabled = true
	repair_info.text = "  Repairs require regeneration after changing corrections." if data.has("reviewed_profile") else ""
	draft_points = data.points.duplicate(true)
	var vertices := PackedVector3Array()
	for index in range(draft_points.size()):
		var point: Dictionary = draft_points[index]
		var selection: Dictionary = edits.get(str(int(point.source_sample)), {})
		for candidate in _candidates(index):
			if selection.get("way_id") == candidate.way_id and selection.get("node_ids") == candidate.node_ids and candidate.has("projected_xz"):
				point.position[0] = candidate.projected_xz[0]
				point.position[2] = candidate.projected_xz[1]
		if point.position[1] != null: point.position[1] += height_offset.value
		point["grade_percent"] = null
		if index == 0 or point.break_before or point.position[1] == null: continue
		var previous: Dictionary = draft_points[index - 1]
		if previous.position[1] == null: continue
		var a := Vector3(previous.position[0], previous.position[1], previous.position[2])
		var b := Vector3(point.position[0], point.position[1], point.position[2])
		var length := Vector2(b.x-a.x, b.z-a.z).length()
		if length < 1.0 or length > 100.0: continue
		point.grade_percent = 100.0 * (b.y-a.y)/length
		var side := Vector3(-3.0*(b.z-a.z)/length, 0, 3.0*(b.x-a.x)/length)
		vertices.append_array(PackedVector3Array([a+side,a-side,b+side,a-side,b-side,b+side]))
	correction_road.mesh = null
	if not vertices.is_empty():
		var arrays := []
		arrays.resize(Mesh.ARRAY_MAX)
		arrays[Mesh.ARRAY_VERTEX] = vertices
		var mesh := ArrayMesh.new()
		mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
		correction_road.mesh = mesh
	correction_road.visible = not edits.is_empty() or height_offset.value != 0.0
	if managed_project:
		var draft: Dictionary = data.duplicate(true)
		draft.erase("reviewed_profile")
		draft["corrected_profile"] = manual_repair.profile_from_points(draft_points.duplicate(true), [])
		draft["corrections"] = {"road_choices":edits.values()}
		manual_repair.preparation = draft
		manual_repair.source_hash = preparation_hash
		var spans: Array = manual_repair.current_spans()
		var result: Dictionary = manual_repair.combined_preview(draft, spans)
		if result.has("error"):
			# Preserve removal intent and display the new positions while the user repairs a conflict.
			_show_repair(manual_repair.combined_preview(draft, spans, false).profile)
			repair_feedback.text = "Draft needs adjustment: " + str(result.error)
		else:
			_show_repair(result.profile)
			repair_feedback.text = "Road choices and removals are shown together. Save route edits to keep them."

func _show_repair(profile: Dictionary) -> void:
	traversal.playing = false
	if ride_mode.button_pressed: ride_mode.button_pressed = false
	ride_mode.disabled = true; play_route.disabled = true
	route_geometry.visible = false; route_geometry_toggle.disabled = true
	route_geometry_info.text = "Rebuild route geometry after point removals."
	if profile.get("repairs", []).is_empty():
		active_repair = {}; repaired_road.mesh = null; repaired_road.visible = false
		road.visible = true; retained_markers.visible = false
		repair_toggle.set_pressed_no_signal(false); repair_toggle.disabled = true
		correction_road.visible = true
		repair_info.text = "No point removals in the current draft."
		_select_point(slider.value)
		return
	if ride_mode.button_pressed: ride_mode.button_pressed = false
	var geometry := _terrain_mesh(profile)
	if geometry == null or not profile.get("repairs") is Array or profile.repairs.is_empty():
		_fail("Invalid repair preview")
		return
	active_repair = profile
	repaired_road.mesh = geometry
	repair_toggle.disabled = false
	repair_toggle.button_pressed = true
	repaired_road.visible = true
	road.visible = false
	retained_markers.visible = true
	var dots := MultiMesh.new()
	dots.transform_format = MultiMesh.TRANSFORM_3D
	var dot := SphereMesh.new(); dot.radius = 0.7; dot.height = 1.4
	dot.material = _material(Color("fff0a0"))
	dot.material.no_depth_test = true
	var positions: Array = []
	for point in profile.points:
		if _valid_position(point.position): positions.append(point.position)
	dots.mesh = dot; dots.instance_count = positions.size()
	for i in range(positions.size()):
		var pos: Array = positions[i]
		dots.set_instance_transform(i, Transform3D(Basis.IDENTITY, Vector3(pos[0],pos[1]+0.8,pos[2])))
	retained_markers.multimesh = dots
	correction_road.visible = false
	repair_info.text = "  REPAIR PROPOSAL: %d retained points / %d findings. Toggle to compare with pink un-repaired profile." % [profile.points.size(),profile.findings.size()]
	_select_point(slider.value)

func _remove_current_point() -> void:
	if ride_mode.button_pressed:
		manual_repair.status.text = "Switch off Route distance to select a source point."
		repair_feedback.text = manual_repair.status.text; return
	var selected := int(slider.value)
	var before: int = manual_repair.undo_history.size()
	manual_repair.remove_current(selected)
	if manual_repair.undo_history.size() > before:
		repair_feedback.text = "Removed source point %d · %d/%d points retained · Ctrl+Z to undo · Save manual repair to keep changes" % [selected+1, active_repair.points.size(), data.points.size()]

func _set_ride_mode(enabled: bool) -> void:
	traversal.playing = false
	play_route.text = "Play"
	road_choice.disabled = enabled
	if enabled:
		saved_source_index = slider.value
		slider.step = 0.1
		slider.max_value = traversal.length_m
		route_geometry_toggle.button_pressed = true
		if follow_route.button_pressed: radius = 90
		_seek_route(traversal.distance_m)
	else:
		slider.step = 1
		slider.max_value = maxi(0,data.get("points",[]).size()-1)
		saved_source_index = clampf(saved_source_index,0,slider.max_value)
		slider.set_value_no_signal(saved_source_index)
		_select_point(saved_source_index)

func _seek_route(value: float) -> void:
	var sample: Dictionary = traversal.seek(value)
	if sample.is_empty(): return
	slider.set_value_no_signal(traversal.distance_m)
	marker.visible = true
	marker.position = sample.position + Vector3.UP*8
	point_summary.text = "%.1f / %.1f m · %.1f m elevation · %+.2f%% grade · Simulated / no trainer" % [sample.distance_m,traversal.length_m,sample.elevation_m,sample.grade_percent]
	if follow_route.button_pressed:
		center = sample.position
		_update_camera()

func _process(delta: float) -> void:
	if traversal.playing and ride_mode != null and ride_mode.button_pressed:
		traversal.advance(delta,preview_speed.value)
		_seek_route(traversal.distance_m)
		if not traversal.playing: play_route.text = "Replay"

func _correction_file(path: String) -> void:
	if correction_picker.file_mode == FileDialog.FILE_MODE_SAVE_FILE:
		# Never overwrite a preparation or an existing corrections file.
		if FileAccess.file_exists(path):
			correction_status.text = "  Choose a new filename; existing files are preserved."
			return
		var file := FileAccess.open(path, FileAccess.WRITE)
		if file == null:
			correction_status.text = "  Cannot save corrections."
			return
		file.store_string(JSON.stringify({"schema_version": 1, "kind": "openride.scenery-corrections", "preparation_sha256": preparation_hash, "elevation_offset_m": height_offset.value, "road_choices": edits.values()}, "  "))
		file.close()
		correction_status.text = "  Saved corrections: " + path
		return
	var file := FileAccess.open(path, FileAccess.READ)
	if file == null or file.get_length() > 1024 * 1024:
		correction_status.text = "  Cannot read corrections (limit 1 MiB)."
		return
	var parsed: Variant = JSON.parse_string(file.get_as_text())
	if not parsed is Dictionary or parsed.get("kind") != "openride.scenery-corrections" or parsed.get("schema_version") != 1 or parsed.get("preparation_sha256") != preparation_hash:
		correction_status.text = "  Source conflict or unsupported corrections; current edits preserved."
		return
	var offset: Variant = parsed.get("elevation_offset_m")
	if not (offset is float or offset is int) or not is_finite(float(offset)) or absf(float(offset)) > 100 or not parsed.get("road_choices") is Array:
		correction_status.text = "  Invalid corrections; current edits preserved."
		return
	var loaded := {}
	for choice in parsed.road_choices:
		var found := false
		if choice is Dictionary:
			for index in range(data.points.size()):
				if choice.get("source_sample") != data.points[index].source_sample: continue
				for candidate in _candidates(index):
					if candidate.has("projected_xz") and choice.get("way_id") == candidate.way_id and choice.get("node_ids") == candidate.node_ids:
						found = true
		if not found or loaded.has(str(int(choice.source_sample))):
			correction_status.text = "  Candidate conflict; current edits preserved."
			return
		loaded[str(int(choice.source_sample))] = choice
	edits = loaded
	undo_propagation.disabled = true
	height_offset.set_value_no_signal(offset)
	_refresh_corrections()
	_select_point(slider.value)
	correction_status.text = "  Loaded corrections. Cyan: source / Pink: unvalidated draft."

func _terrain_mesh(terrain: Variant) -> ArrayMesh:
	if not terrain is Dictionary or not terrain.get("mesh") is Dictionary:
		return null
	var geometry: Dictionary = terrain.mesh
	if not geometry.get("vertices") is Array or not geometry.get("indices") is Array:
		return null
	if geometry.vertices.is_empty() or geometry.vertices.size() > 65536 or geometry.indices.is_empty() or geometry.indices.size() > 400000 or geometry.indices.size() % 3 != 0:
		return null
	var vertices := PackedVector3Array()
	for position in geometry.vertices:
		if not _valid_position(position):
			return null
		vertices.append(Vector3(position[0], position[1], position[2]))
	var indices := PackedInt32Array()
	for index in geometry.indices:
		if not (index is float or index is int) or not is_finite(float(index)) or index != floor(index) or index < 0 or index >= vertices.size():
			return null
		indices.append(int(index))
	var arrays := []
	arrays.resize(Mesh.ARRAY_MAX)
	arrays[Mesh.ARRAY_VERTEX] = vertices
	arrays[Mesh.ARRAY_INDEX] = indices
	var mesh := ArrayMesh.new()
	mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
	var surface := SurfaceTool.new()
	surface.create_from(mesh, 0)
	surface.generate_normals()
	return surface.commit()

func _update_camera() -> void:
	camera.position = center + Vector3(sin(yaw) * cos(pitch), sin(pitch), cos(yaw) * cos(pitch)) * radius
	camera.look_at(center)

func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventKey and event.pressed and not event.echo:
		var focus := get_viewport().gui_get_focus_owner()
		if not (focus is LineEdit or focus is TextEdit):
			if event.keycode == KEY_DELETE:
				_remove_current_point(); get_viewport().set_input_as_handled(); return
			if event.keycode == KEY_Z and event.ctrl_pressed:
				manual_repair.undo_removal(); get_viewport().set_input_as_handled(); return
	if event is InputEventKey and event.pressed and not event.echo and event.keycode == KEY_F:
		_focus_point()
	if event is InputEventMouseMotion:
		if event.button_mask & MOUSE_BUTTON_MASK_MIDDLE or (event.shift_pressed and event.button_mask & MOUSE_BUTTON_MASK_LEFT):
			_pan_camera(event.relative)
		elif event.button_mask & MOUSE_BUTTON_MASK_LEFT:
			yaw -= event.relative.x * 0.006
			pitch = clampf(pitch + event.relative.y * 0.006, 0.1, 1.5)
			_update_camera()
	if event is InputEventMouseButton and event.pressed:
		if event.button_index == MOUSE_BUTTON_WHEEL_UP:
			radius = maxf(2.0, radius * 0.85)
		elif event.button_index == MOUSE_BUTTON_WHEEL_DOWN:
			radius = minf(15000.0, radius / 0.85)
		_update_camera()
