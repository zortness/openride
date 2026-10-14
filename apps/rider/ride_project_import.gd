extends VBoxContainer
const Plot = preload("res://plot.gd")
var owner_screen: Control
var review: Dictionary = {}
var import_button: Button
var save_button: Button
var choices: Array[OptionButton] = []
var comparison: Label
var detail: Label
var map: Control
var profile: Control
var log_toggle: CheckButton
var log_view: TextEdit
var content: VBoxContainer

func setup(screen: Control) -> void:
	owner_screen = screen; add_theme_constant_override("separation",10)
	import_button = screen.add_button(self,"Import recording…",choose_sources)
	screen.text(self,"Choose GPX / TCX files from the same road ride. We use the recording to infer roads; its original data stays intact.",16)
	content=VBoxContainer.new();content.add_theme_constant_override("separation",10);add_child(content)
	var reference_row:=HBoxContainer.new();content.add_child(reference_row)
	var diagnostics:VBoxContainer=screen.foldout(content,"Recording details and source comparison")
	comparison=screen.text(diagnostics,"")
	for caption in ["View source","Reference recording","Legacy elevation source"]:
		var row:=HBoxContainer.new()
		if caption=="Reference recording": reference_row.add_child(row); row.size_flags_horizontal=Control.SIZE_EXPAND_FILL
		else: diagnostics.add_child(row)
		var label:Label=screen.text(row,caption,15);label.custom_minimum_size.x=155
		var choice:=OptionButton.new();choice.fit_to_longest_item=false;choice.clip_text=true;choice.size_flags_horizontal=Control.SIZE_EXPAND_FILL;row.add_child(choice);choices.append(choice)
	choices[0].item_selected.connect(show_source)
	save_button=screen.add_button(reference_row,"Use recording",func():choices[2].select(choices[1].selected);save_choices())
	map=Plot.new();map.custom_minimum_size=Vector2(240,220);content.add_child(map);map.sample_picked.connect(show_sample)
	profile=Plot.new();profile.profile=true;profile.custom_minimum_size=Vector2(240,160);diagnostics.add_child(profile);profile.sample_picked.connect(show_sample)
	detail=screen.text(diagnostics,"",15)
	screen.add_button(diagnostics,"Save legacy source choices",save_choices)
	screen.text(diagnostics,"Plots show the original recording. Road-first courses generate their own elevations from terrain; legacy elevation choices do not define the new course.",14)
	log_toggle=CheckButton.new();log_toggle.text="Show import log";diagnostics.add_child(log_toggle)
	log_view=TextEdit.new();log_view.editable=false;log_view.custom_minimum_size.y=150;diagnostics.add_child(log_view);log_view.hide()
	log_toggle.toggled.connect(func(value:bool):log_view.visible=value)
	content.hide()

func choose_sources() -> void:
	var picker := FileDialog.new(); picker.access = FileDialog.ACCESS_FILESYSTEM
	picker.file_mode = FileDialog.FILE_MODE_OPEN_FILES; picker.filters = PackedStringArray(["*.gpx,*.tcx ; Recording variants"])
	add_child(picker)
	picker.files_selected.connect(func(files: PackedStringArray): import_sources(files); picker.queue_free())
	picker.canceled.connect(picker.queue_free); picker.popup_centered_ratio(0.75)

func import_sources(files: PackedStringArray) -> void:
	var args := PackedStringArray()
	for path in files: args.append_array(["--source", path])
	owner_screen.run_job("import", owner_screen.project_path, args)

func populate() -> void:
	review = owner_screen.snapshot.get("review", {})
	content.visible = not review.is_empty()
	log_toggle.set_pressed_no_signal(false); log_view.hide()
	if review.is_empty(): return
	for choice in choices: choice.clear()
	for choice in [choices[1], choices[2]]: choice.add_item("Choose a source"); choice.set_item_metadata(0, "")
	comparison.text = ""
	for activity in review.activities:
		choices[0].add_item(activity.source_name)
		for choice in [choices[1], choices[2]]:
			choice.add_item(activity.format + " · " + activity.source_name); choice.set_item_metadata(choice.item_count - 1, activity.source_sha256)
		var missing_positions := 0; var missing_heights := 0; var missing_times := 0; var breaks := 0
		for sample in activity.samples:
			if sample.latitude == null or sample.longitude == null: missing_positions += 1
			if sample.elevation_m == null: missing_heights += 1
			if sample.time_ms == null: missing_times += 1
		for gap in review.gaps:
			if gap.source_sha256 == activity.source_sha256: breaks += 1
		comparison.text += "%s · %d samples · %.2f km geometry (gaps excluded) · recorded %s\n%d gaps · missing position %d / elevation %d / timestamp %d · %d diagnostics\n" % [activity.source_name, activity.samples.size(), activity.route_distance_m / 1000.0, "—" if activity.recorded_distance_m == null else "%.2f km" % (activity.recorded_distance_m / 1000.0), breaks, missing_positions, missing_heights, missing_times, activity.diagnostics.size()]
	for pair in [[choices[1], "geometry_source"], [choices[2], "elevation_source"]]:
		for index in range(pair[0].item_count):
			if pair[0].get_item_metadata(index) == review.document[pair[1]]: pair[0].select(index)
	if choices[1].selected == 0 and choices[1].item_count > 1: choices[1].select(1)
	var file := FileAccess.open(owner_screen.project_path.path_join("logs/import.log"), FileAccess.READ)
	log_view.text = file.get_buffer(mini(file.get_length(), 65536)).get_string_from_utf8() if file != null else "Import log unavailable; source diagnostics are retained in the review."
	show_source(0)

func refresh() -> void:
	import_button.disabled = owner_screen.busy or owner_screen.snapshot.is_empty() or owner_screen.snapshot.project.stage != "empty"
	save_button.disabled = owner_screen.busy or review.is_empty()
	for choice in choices: choice.disabled = owner_screen.busy

func save_choices() -> void:
	var geometry := str(choices[1].get_selected_metadata()); var elevation := str(choices[2].get_selected_metadata())
	if geometry.is_empty() or elevation.is_empty(): owner_screen.feedback.text = "Choose both geometry and elevation sources."; return
	owner_screen.run_job("select-sources", owner_screen.project_path, PackedStringArray(["--geometry", geometry, "--elevation", elevation]))

func show_source(index: int) -> void:
	if review.is_empty(): return
	choices[0].select(index)
	var activity: Dictionary = review.activities[index]
	for plot in [map, profile]: plot.configure(activity.samples, 0, activity.samples.size() - 1, 0)
	show_sample(0)

func show_sample(index: int) -> void:
	var activity: Dictionary = review.activities[choices[0].selected]
	var sample: Dictionary = activity.samples[index]
	for plot in [map, profile]: plot.cursor = index; plot.queue_redraw()
	detail.text = "Sample %d · segment %d · %.3f km · elevation %s m · time %s\nLatitude %s · longitude %s" % [index, sample.segment, sample.route_distance_m / 1000.0, str(sample.elevation_m), "missing" if sample.time_ms == null else Time.get_datetime_string_from_unix_time(int(sample.time_ms / 1000.0)) + " UTC", str(sample.latitude), str(sample.longitude)]
