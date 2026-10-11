extends VBoxContainer

signal preview_requested(profile: Dictionary)
signal project_repairs_saved(document: Dictionary)
signal scan_requested
var managed_project := false
var saved_spans: Array = []
var expected_hash := ""
var report: Dictionary = {}
var candidates: ItemList
var details: Label
var picker: FileDialog
var source_points: Array = []
var preparation: Dictionary = {}

func _ready() -> void:
	var row := VBoxContainer.new()
	add_child(row)
	for action in ["Find stops / GPS jitter" if managed_project else "Load jitter review", "Preview candidate", "Save selected repairs"]:
		var button := Button.new()
		button.text = action
		row.add_child(button)
		button.pressed.connect(func():
			if action == "Find stops / GPS jitter":
				details.text = "Saving draft and scanning the imported recording..."
				scan_requested.emit()
			elif action == "Preview candidate":
				preview_selected()
			elif action == "Save selected repairs" and managed_project:
				save_selected("")
			else:
				picker.file_mode = FileDialog.FILE_MODE_OPEN_FILE if action == "Load jitter review" else FileDialog.FILE_MODE_SAVE_FILE
				picker.popup_centered_ratio(0.7))
	candidates = ItemList.new()
	candidates.select_mode = ItemList.SELECT_MULTI
	candidates.custom_minimum_size.y = 42
	candidates.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	candidates.multi_selected.connect(func(index: int, _selected: bool): show_details(index))
	add_child(candidates)
	details = Label.new()
	details.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	details.text = "Find stops / GPS jitter scans timestamps in the imported ride. Review each suggestion before saving." if managed_project else "Load a source-bound jitter report."
	add_child(details)
	picker = FileDialog.new(); picker.display_mode = FileDialog.DISPLAY_LIST
	picker.access = FileDialog.ACCESS_FILESYSTEM
	picker.filters = PackedStringArray(["*.json ; Review or repair document"])
	picker.file_selected.connect(func(path: String):
		if picker.file_mode == FileDialog.FILE_MODE_OPEN_FILE: load_report(path)
		else: save_selected(path))
	add_child(picker)

func invalidate() -> void:
	expected_hash = ""
	report.clear()
	candidates.clear()
	details.text = "Road edits changed. Find stops / GPS jitter again to refresh suggestions." if managed_project else "Reload a matching jitter report."

func load_report(path: String) -> void:
	var file := FileAccess.open(path, FileAccess.READ)
	if file == null or file.get_length() > 16*1024*1024:
		details.text = "  Cannot open bounded jitter report."
		return
	var parsed: Variant = JSON.parse_string(file.get_as_text())
	if not parsed is Dictionary or parsed.get("kind") != "openride.jitter-review" or parsed.get("schema_version") != 1 or expected_hash.is_empty() or parsed.get("preparation_sha256") != expected_hash or not parsed.get("candidates") is Array:
		details.text = "  Report source conflict. Reopen the unchanged corrected preparation and its matching report."
		return
	for entry in parsed.candidates:
		if not entry is Dictionary or not entry.get("span") is Dictionary or not entry.get("preview") is Dictionary:
			details.text = "  Invalid review candidate."
			return
	report = parsed
	candidates.clear()
	for entry in report.candidates:
		var first_number := -1
		var last_number := -1
		for index in range(source_points.size()):
			if source_points[index].source_sample == entry.cluster_start_source_sample: first_number = index+1
			if source_points[index].source_sample == entry.cluster_end_source_sample: last_number = index+1
		candidates.add_item("Source points %d–%d / %.0f s / %s" % [first_number,last_number,entry.duration_s,str(entry.confidence).replace("_", " ")])
	details.text = "  %d candidates / %d rejected. Select candidates to review; Ctrl-click for multiple." % [report.candidates.size(),report.get("rejected", []).size()]
	if report.candidates.is_empty():
		details.text = "No removable stop clusters found. Use Remove slider point or From / Through for manual cleanup."
		if not report.get("rejected", []).is_empty(): details.text += " First rejected cluster: " + str(report.rejected[0].reason)

func show_details(index: int) -> void:
	var entry: Dictionary = report.candidates[index]
	var speed := "absent" if entry.recorded_speed_median_mps == null else "%.2f m/s" % entry.recorded_speed_median_mps
	details.text = "  GPS path %.1f m / net %.1f m / median %.2f m/s / recorded speed %s / %d recording gaps >60 s" % [entry.gps_path_m,entry.net_progress_m,entry.median_gps_speed_mps,speed,entry.recording_gaps_over_60s]

func preview_selected() -> void:
	var selected := candidates.get_selected_items()
	if selected.is_empty():
		details.text = "  Select a candidate first."
		return
	show_details(selected[0])
	if managed_project and not preparation.is_empty():
		var spans: Array = saved_spans.duplicate(true)
		for index in selected: spans.append(report.candidates[index].span)
		var result: Dictionary = preload("res://manual_repair.gd").combined_preview(preparation,spans)
		if result.has("error"): details.text = result.error; return
		preview_requested.emit(result.profile)
	else: preview_requested.emit(report.candidates[selected[0]].preview)

func save_selected(path: String) -> void:
	var selected := candidates.get_selected_items()
	if selected.is_empty() or expected_hash.is_empty():
		details.text = "  Select reviewed candidates first."
		return
	if FileAccess.file_exists(path):
		details.text = "  Choose a new filename; existing files are preserved."
		return
	var spans: Array = saved_spans.duplicate(true) if managed_project else []
	for index in selected:
		spans.append(report.candidates[index].span)
	if managed_project:
		project_repairs_saved.emit({"kind":"openride.profile-repairs","schema_version":1,"preparation_sha256":expected_hash,"spans":spans})
		return
	var file := FileAccess.open(path,FileAccess.WRITE)
	if file == null:
		details.text = "  Cannot save selected repairs."
		return
	file.store_string(JSON.stringify({"kind":"openride.profile-repairs","schema_version":1,"preparation_sha256":expected_hash,"spans":spans},"  "))
	file.close()
	details.text = "  Selected repairs saved. Run repair-profile.py to regenerate; original activity is unchanged."
