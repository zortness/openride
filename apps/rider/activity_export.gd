extends "res://screen.gd"

var bridge: RefCounted
var picker: FileDialog
var destination_picker: FileDialog
var source_label: Label
var result_label: Label
var export_button: Button
var open_button: Button
var source := ""
var manifest := ""
var manifest_button: Button
var busy := false
var output_directory := ""

func _ready() -> void:
	title = "OpenRide / Activity exports"
	size = Vector2i(820, 580)
	close_requested.connect(hide)
	bridge = ClassDB.instantiate("ActivityExporter")
	var margin := MarginContainer.new()
	margin.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	for side in ["left", "right", "top", "bottom"]:
		margin.add_theme_constant_override("margin_" + side, 24)
	add_child(margin)
	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", 16)
	margin.add_child(column)
	var intro := Label.new()
	intro.text = "Export a completed or interrupted trainer session\nFIT / TCX for analysis; GPX for virtual tracks; CSV for raw data."
	column.add_child(intro)
	var select_button := Button.new()
	select_button.text = "Choose recorded session..."
	column.add_child(select_button)
	source_label = Label.new()
	source_label.text = "Select session.jsonl from a session folder."
	source_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	column.add_child(source_label)
	manifest_button = Button.new()
	manifest_button.text = "Older ride: choose original manifest (optional)..."
	column.add_child(manifest_button)
	var manifest_picker := FileDialog.new(); manifest_picker.display_mode = FileDialog.DISPLAY_LIST
	manifest_picker.access = FileDialog.ACCESS_FILESYSTEM
	manifest_picker.file_mode = FileDialog.FILE_MODE_OPEN_FILE
	manifest_picker.filters = PackedStringArray(["*.json ; Original package manifest"])
	add_child(manifest_picker)
	manifest_button.pressed.connect(func():
		if not busy: manifest_picker.popup_centered_ratio(0.8))
	manifest_picker.file_selected.connect(func(path: String):
		if not busy:
			manifest = path
			manifest_button.text = "Original manifest: " + path.get_file())
	export_button = Button.new()
	export_button.text = "Export activity..."
	export_button.disabled = true
	column.add_child(export_button)
	var quick_export := Button.new(); quick_export.text = "Export to my Activities folder"; column.add_child(quick_export)
	quick_export.pressed.connect(func():
		if source.is_empty() or busy: return
		var parent := ProjectSettings.globalize_path("user://exports")
		DirAccess.make_dir_recursive_absolute(parent)
		begin_export(source,parent.path_join("activity-%d-%d" % [int(Time.get_unix_time_from_system()),Time.get_ticks_usec()])))
	result_label = Label.new()
	result_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	result_label.text = "Exports include FIT, TCX and CSV. GPX requires the saved geographic origin. Older rides can use their original package manifest.\nOriginal recordings and existing exports are preserved."
	column.add_child(result_label)
	open_button = Button.new()
	open_button.text = "Open export folder"
	open_button.disabled = true
	open_button.pressed.connect(func(): OS.shell_open(output_directory))
	column.add_child(open_button)
	picker = FileDialog.new(); picker.display_mode = FileDialog.DISPLAY_LIST
	picker.access = FileDialog.ACCESS_FILESYSTEM
	picker.file_mode = FileDialog.FILE_MODE_OPEN_FILE
	picker.filters = PackedStringArray(["*.jsonl ; Session journal"])
	picker.current_dir = ProjectSettings.globalize_path("user://sessions")
	add_child(picker)
	picker.file_selected.connect(func(path: String):
		if busy: return
		select_session(path))
	select_button.pressed.connect(func():
		if not busy: picker.popup_centered_ratio(0.8))
	destination_picker = FileDialog.new(); destination_picker.display_mode = FileDialog.DISPLAY_LIST
	destination_picker.access = FileDialog.ACCESS_FILESYSTEM
	destination_picker.file_mode = FileDialog.FILE_MODE_OPEN_DIR
	add_child(destination_picker)
	export_button.pressed.connect(func(): destination_picker.popup_centered_ratio(0.8))
	destination_picker.dir_selected.connect(func(parent: String):
		var folder := "activity-%s-%s" % [str(Time.get_unix_time_from_system()).replace(".", "-"), str(Time.get_ticks_usec())]
		begin_export(source, parent.path_join(folder)))

func select_session(journal:String) -> void:
	if busy:return
	source=journal;source_label.text=source;export_button.disabled=false
	manifest=""
	manifest_button.text="Older ride: choose original manifest (optional)..."

func begin_export(journal: String, destination: String) -> void:
	if busy: return
	if bridge.start_export(journal, destination, manifest):
		busy = true
		export_button.disabled = true
		open_button.disabled = true
		result_label.text = "Exporting..."

func _process(_delta: float) -> void:
	if not busy: return
	var raw: String = bridge.poll_export()
	if raw.is_empty(): return
	busy = false
	export_button.disabled = source.is_empty()
	var result = JSON.parse_string(raw)
	if not result is Dictionary or not result.get("ok", false):
		result_label.text = "Export failed: " + str(result.get("message", "Unknown export error") if result is Dictionary else "Invalid worker response")
		return
	var report: Dictionary = result.report
	output_directory = report.output_directory
	open_button.disabled = false
	result_label.text = "Exported %d FIT records and %d CSV samples.\nElapsed: %.3f s / timer running: %.3f s\n%s\n%s" % [report.fit_records, report.csv_samples, report.elapsed_ms / 1000.0, report.timer_ms / 1000.0, "Completed session" if report.completed_session else "RECOVERED / INCOMPLETE SESSION", output_directory]
	if report.get("virtual_distance_m")!=null:result_label.text+="\nVirtual distance: %.2f m" % report.virtual_distance_m
	# The full precision and recovery notes are always retained in report.json.
	result_label.text += "\nTCX: %d points / %s\nVirtual coordinates are derived, not outdoor GPS. See report.json for notes." % [report.tcx_records, ("GPX: %d points" % report.gpx_records) if report.geographic_source != null else "GPX omitted: geographic origin unavailable"]
