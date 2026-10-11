extends VBoxContainer

signal preview_requested(profile: Dictionary)
signal project_repairs_saved(document: Dictionary)
signal remove_current_requested
signal action_feedback(message: String)
var undo_history: Array = []
var undo_button: Button
var remove_button: Button
var managed_project := false
var saved_spans: Array = []
var preparation: Dictionary = {}
var source_hash := ""
var first: SpinBox
var last: SpinBox
var status: Label
var picker: FileDialog
var proposal: Dictionary = {}

func _ready() -> void:
	var heading := Label.new()
	heading.text = "Remove source points and rejoin"
	add_child(heading)
	var quick := HBoxContainer.new(); add_child(quick)
	remove_button = Button.new(); remove_button.text = "Remove slider point (Del)"; quick.add_child(remove_button)
	remove_button.tooltip_text = "Removes only the point selected on the bottom slider; ignores From/Through."
	remove_button.pressed.connect(func(): remove_current_requested.emit())
	undo_button = Button.new(); undo_button.text = "Undo removal (Ctrl+Z)"; undo_button.disabled = true; quick.add_child(undo_button)
	undo_button.pressed.connect(undo_removal)
	var row := HBoxContainer.new()
	add_child(row)
	for caption in ["From", "Through"]:
		var label := Label.new()
		label.text = caption
		row.add_child(label)
		var spin := SpinBox.new()
		spin.min_value = 1
		spin.max_value = 10000
		spin.step = 1
		row.add_child(spin)
		if caption == "From": first = spin
		else: last = spin
	var range_help := Label.new(); range_help.text = "Range only: inclusive From / Through point numbers"; add_child(range_help)
	for caption in ["Preview selected range", "Save manual repair"]:
		var button := Button.new()
		button.text = caption
		if managed_project and caption == "Save manual repair": button.text = "Save route edits"
		button.pressed.connect(func():
			if caption == "Preview selected range": preview()
			elif managed_project: project_repairs_saved.emit({"spans":current_spans()})
			elif proposal.is_empty(): status.text = "Preview the range before saving."
			else: picker.popup_centered_ratio(0.7))
		add_child(button)
	status = Label.new()
	status.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	status.text = "Inclusive source-point numbers from the bottom slider. Original measurements are retained."
	add_child(status)
	picker = FileDialog.new(); picker.display_mode = FileDialog.DISPLAY_LIST
	picker.access = FileDialog.ACCESS_FILESYSTEM
	picker.file_mode = FileDialog.FILE_MODE_SAVE_FILE
	picker.filters = PackedStringArray(["*.json ; Manual repair"])
	picker.file_selected.connect(save)
	add_child(picker)

func configure(data: Dictionary, fingerprint: String) -> void:
	preparation = data
	source_hash = fingerprint
	proposal = {}
	undo_history.clear()
	undo_button.disabled = true
	first.max_value = data.points.size()
	last.max_value = first.max_value
	if data.has("reviewed_profile"):
		status.text = "A repair is already loaded. Removed points remain in the source slider. Open the original corrected preparation to author a replacement."

func invalidate() -> void:
	source_hash = ""
	proposal = {}
	undo_history.clear()
	undo_button.disabled = true

func preview() -> void:
	remove_range(int(first.value)-1, int(last.value)-1)
	action_feedback.emit(status.text)

func current_spans() -> Array:
	return proposal.spans if not proposal.is_empty() else saved_spans

func remove_current(index: int) -> void:
	remove_range(index, index)
	action_feedback.emit(status.text)

func remove_range(begin: int, end: int) -> void:
	if source_hash.is_empty() or not preparation.has("corrected_profile") or preparation.has("reviewed_profile"):
		status.text = "Open an unchanged corrected preparation (before repair) to author a removal."
		return
	var points: Array = preparation.corrected_profile.points
	if begin < 0 or end < begin or end >= points.size():
		status.text = "Choose point numbers within this route."; return
	var removed := {}
	for span in current_spans():
		for i in range(points.size()):
			var sample = points[i].source_sample
			if sample > span.start_source_sample and sample < span.end_source_sample or span.method == "trim_start" and sample == span.start_source_sample or span.method == "trim_end" and sample == span.end_source_sample: removed[i] = true
	var changed := false
	for i in range(begin, end+1):
		if not removed.has(i): changed = true
		removed[i] = true
	if not changed:
		status.text = "This point/range is already removed."; return
	if points.size()-removed.size() < 2:
		status.text = "Keep at least two route points."; return
	# Consecutive removals become one connector between the surviving neighbors.
	var spans: Array = []
	var index := 0
	while index < points.size():
		if not removed.has(index): index += 1; continue
		var first_index := index
		while removed.has(index+1): index += 1
		if first_index == 0 or index == points.size()-1:
			spans.append({"method":"trim_start" if first_index==0 else "trim_end", "reason":"Explicit endpoint removal", "start_source_sample":points[0 if first_index==0 else first_index-1].source_sample, "end_source_sample":points[index+1 if first_index==0 else index].source_sample})
			index += 1; continue
		var candidate := make_preview(preparation, first_index+1, index+1, true)
		if candidate.has("error"): status.text = candidate.error; return
		spans.append(candidate.span)
		index += 1
	var result := combined_preview(preparation, spans)
	if result.has("error"):
		status.text = result.error
		return
	undo_history.append(current_spans().duplicate(true))
	undo_button.disabled = false
	proposal = {"kind":"openride.profile-repairs","schema_version":1,"preparation_sha256":source_hash,"spans":spans}
	preview_requested.emit(result.profile)
	status.text = "Point removal previewed. Ctrl+Z undoes it; Save route edits keeps the draft." if managed_project else "Point removal previewed. Save manual repair to keep it."

func undo_removal() -> void:
	if undo_history.is_empty(): return
	var spans: Array = undo_history.pop_back()
	proposal = {"kind":"openride.profile-repairs","schema_version":1,"preparation_sha256":source_hash,"spans":spans}
	undo_button.disabled = undo_history.is_empty()
	preview_requested.emit(combined_preview(preparation, spans).profile)
	status.text = "Last removal undone. Save commits the current draft."
	action_feedback.emit(status.text)

static func combined_preview(data: Dictionary, spans: Array, validate := true) -> Dictionary:
	var original: Array = data.corrected_profile.points
	var removed := {}; var joined := {}; var reports: Array = []
	for span in spans:
		var a := -1; var b := -1
		for i in range(original.size()):
			if original[i].source_sample == span.start_source_sample: a = i
			if original[i].source_sample == span.end_source_sample: b = i
		if a < 0 or b <= a: return {"error":"Repair span no longer matches source points."}
		if span.method in ["trim_start", "trim_end"]:
			var cut: Array = []
			for i in range(a if span.method=="trim_start" else a+1, b if span.method=="trim_start" else b+1):
				removed[i] = true; cut.append(original[i].source_sample)
			var report: Dictionary = span.duplicate(true)
			report.merge({"removed_source_samples":cut,"connector_length_m":0.0,"max_deviation_m":0.0,"replaced_breaks":[]})
			reports.append(report); continue
		var result := make_preview(data, a+2, b, span.method=="manual_remove")
		if result.has("error"):
			if validate: return result
			var report: Dictionary = span.duplicate(true)
			report["removed_source_samples"] = []
			for i in range(a+1,b): report.removed_source_samples.append(original[i].source_sample)
			reports.append(report)
		else: reports.append(result.profile.repairs[0])
		joined[b] = true
		for i in range(a+1, b): removed[i] = true
	var points: Array = []
	for i in range(original.size()):
		if removed.has(i): continue
		var point: Dictionary = original[i].duplicate(true)
		if joined.has(i): point.break_before = false; point["repair_connector"] = true
		points.append(point)
	if points.size() < 2: return {"error":"Keep at least two route points."}
	points[0].break_before = true
	return {"profile": profile_from_points(points, reports)}

func save(path: String) -> void:
	if proposal.is_empty(): return
	if FileAccess.file_exists(path):
		status.text = "Choose a new filename; existing files are preserved."
		return
	var file := FileAccess.open(path,FileAccess.WRITE)
	if file == null:
		status.text = "Cannot save repair."
		return
	file.store_string(JSON.stringify(proposal,"  "))
	file.close()
	status.text = "Saved manual repair. Regenerate with repair-profile.py."

static func make_preview(data: Dictionary, first_number: int, last_number: int, manual := false) -> Dictionary:
	var original: Array = data.corrected_profile.points
	if first_number < 2 or last_number < first_number or last_number >= original.size():
		return {"error":"Keep the first and last source points; choose an inclusive interior range."}
	var start := first_number-2
	var end := last_number
	var choices := {}
	for choice in data.corrections.road_choices: choices[int(choice.source_sample)] = choice.way_id
	var way: Variant = choices.get(int(original[start].source_sample))
	var end_way: Variant = choices.get(int(original[end].source_sample))
	var a: Array = original[start].position
	var b: Array = original[end].position
	var length := Vector2(b[0]-a[0],b[2]-a[2]).length()
	if length < 1 or length > 100:
		return {"error":"Cannot remove points %d–%d: surviving points %d and %d are %.1f m apart (allowed 1–100 m). Adjust the range or road choices." % [first_number,last_number,start+1,end+1,length]}
	var removed: Array = []
	var breaks: Array = []
	var deviation := 0.0
	for index in range(start,end+1):
		var point: Dictionary = original[index]
		if not manual and (way == null or choices.get(int(point.source_sample)) != way):
			return {"error":"Choose the same road for the surviving neighbors before rejoining."}
		for value in point.position:
			if value == null or not is_finite(float(value)):
				return {"error":"Cannot join missing or invalid coordinates."}
		var offset := Vector2(point.position[0]-a[0],point.position[2]-a[2])
		var delta := Vector2(b[0]-a[0],b[2]-a[2])
		deviation = maxf(deviation,(offset-delta*clampf(offset.dot(delta)/delta.length_squared(),0,1)).length())
		if index > start and index < end: removed.append(point.source_sample)
		if index > start and point.break_before: breaks.append(point.source_sample)
	var limit := 25.0 if manual else 10.0
	if deviation > limit:
		return {"error":"Cannot remove points %d–%d: deviation %.1f m exceeds %.0f m. Select a smaller range or adjust road choices." % [first_number,last_number,deviation,limit]}
	var span := {"method":"manual_remove" if manual else "replace_with_chord","reason":"Explicit manual source-point removal and rejoin","start_source_sample":original[start].source_sample,"end_source_sample":original[end].source_sample}
	var report := span.duplicate()
	report.merge({"removed_source_samples":removed,"replaced_breaks":breaks,"connector_length_m":length,"max_deviation_m":deviation})
	var points: Array = []
	for index in range(original.size()):
		if index > start and index < end: continue
		var point: Dictionary = original[index].duplicate(true)
		if index == end:
			point.break_before = false
			point["repair_connector"] = true
		points.append(point)
	return {"span":span,"profile":profile_from_points(points,[report])}

static func profile_from_points(points: Array, reports: Array) -> Dictionary:
	var vertices: Array = []
	var indices: Array = []
	var findings: Array = []
	var distance := 0.0
	for index in range(points.size()):
		var point: Dictionary = points[index]
		point.grade_percent = null
		if index > 0:
			var previous: Dictionary = points[index-1]
			var p: Array = previous.position
			var q: Array = point.position
			var step := Vector2(q[0]-p[0],q[2]-p[2]).length()
			if point.break_before or step < 1 or step > 100 or p[1] == null or q[1] == null:
				findings.append({"source_sample":point.source_sample,"issue":"disconnected_or_invalid_segment"})
			else:
				distance += step
				point.grade_percent = 100.0*(q[1]-p[1])/step
				if absf(point.grade_percent)>15:
					findings.append({"source_sample":point.source_sample,"issue":"grade_over_15_percent"})
				var side := [-3.0*(q[2]-p[2])/step,0,3.0*(q[0]-p[0])/step]
				var base := vertices.size()
				for pos in [p,q]:
					for sign_value in [1,-1]: vertices.append([pos[0]+sign_value*side[0],pos[1],pos[2]+sign_value*side[2]])
				indices.append_array([base,base+1,base+2,base+1,base+3,base+2])
		point.distance_m = distance
	return {"points":points,"mesh":{"vertices":vertices,"indices":indices,"width_m":6},"findings":findings,"repairs":reports,"validated":false}
