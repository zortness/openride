extends "res://screen.gd"
const Plot = preload("res://plot.gd")
const CLASSIFICATIONS = ["unresolved", "keep_break", "stationary_pause", "missing_travel"]
var bridge: RefCounted = ClassDB.instantiate("FullRideReviewer")
var review := {}
var directory := ""
var pending_directory := ""
var busy := false
var last_operation_ok := false
var dirty := false
var updating := false
var view_source: OptionButton
var geometry: OptionButton
var elevation: OptionButton
var comparison: TextEdit
var map: Control
var profile: Control
var gaps: ItemList
var classification: OptionButton
var note: LineEdit
var detail: Label
var status: Label
var location: Label
var save_button: Button
var active_gap := ""
var visible_gaps: Array = []
var full_preview: Control

func _ready() -> void:
	title = "Full ride review"
	var scroll := ScrollContainer.new(); scroll.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT); scroll.follow_focus=true; add_child(scroll)
	var box := VBoxContainer.new(); box.size_flags_horizontal=Control.SIZE_EXPAND_FILL; box.add_theme_constant_override("separation",8); scroll.add_child(box)
	label(box,"Full ride review — compare recordings before building scenery")
	var row := HBoxContainer.new(); box.add_child(row)
	button(row,"New review from TCX / GPX",choose_sources)
	button(row,"Open saved review",choose_review)
	save_button=button(row,"Save decisions",save_review)
	button(row,"Discard unsaved changes",func():
		if busy or directory.is_empty(): return
		var confirm := ConfirmationDialog.new(); confirm.dialog_text="Discard unsaved review decisions and reload?"; add_child(confirm)
		confirm.confirmed.connect(func(): submit({"action":"open","directory":directory}); confirm.queue_free())
		confirm.canceled.connect(confirm.queue_free); confirm.popup_centered())
	button(box,"Open full-route sparse preview (no 5 km limit)",open_preview)
	location=label(box,"Sources and review decisions are private. New review creates a subfolder in the folder you choose.")
	comparison=TextEdit.new(); comparison.editable=false; comparison.custom_minimum_size.y=100; box.add_child(comparison)
	row=HBoxContainer.new(); box.add_child(row)
	label(row,"View"); view_source=OptionButton.new(); row.add_child(view_source); view_source.item_selected.connect(show_source)
	label(row,"Geometry"); geometry=OptionButton.new(); row.add_child(geometry); geometry.item_selected.connect(func(index: int): choose_role("geometry_source",geometry,index))
	label(row,"Elevation"); elevation=OptionButton.new(); row.add_child(elevation); elevation.item_selected.connect(func(index: int): choose_role("elevation_source",elevation,index))
	for choice in [view_source,geometry,elevation]: choice.fit_to_longest_item=false; choice.clip_text=true; choice.size_flags_horizontal=Control.SIZE_EXPAND_FILL
	row=HBoxContainer.new(); box.add_child(row)
	map=Plot.new(); profile=Plot.new(); profile.profile=true
	for plot in [map,profile]:
		plot.custom_minimum_size=Vector2(300,190); plot.size_flags_horizontal=Control.SIZE_EXPAND_FILL; row.add_child(plot)
		plot.sample_picked.connect(show_sample)
	detail=label(box,"Select a source or click either plot. Orange markers preserve source breaks.")
	gaps=ItemList.new(); gaps.custom_minimum_size.y=110; box.add_child(gaps); gaps.item_selected.connect(select_gap)
	row=HBoxContainer.new(); box.add_child(row)
	classification=OptionButton.new(); row.add_child(classification)
	for text in ["Unresolved", "Keep break", "Recording pause (preview may bridge)", "Missing travel / needs repair"]: classification.add_item(text)
	classification.item_selected.connect(func(_index: int): edit_gap())
	note=LineEdit.new(); note.placeholder_text="Review note"; note.max_length=400; note.size_flags_horizontal=Control.SIZE_EXPAND_FILL; row.add_child(note); note.text_changed.connect(func(_text: String): edit_gap())
	label(box,"Save decisions before previewing. The full-route preview bridges reviewed pauses up to 5 m with consistent elevation; other breaks remain visible. Scenery preparation comes later.")
	status=label(box,"Ready — this review is not a rideable package.")
	refresh_enabled()

func label(parent: Node, text: String) -> Label:
	var result := Label.new(); result.text=text
	result.autowrap_mode=TextServer.AUTOWRAP_OFF if parent is HBoxContainer else TextServer.AUTOWRAP_WORD_SMART
	parent.add_child(result); return result

func button(parent: Node, text: String, callback: Callable) -> Button:
	var result := Button.new(); result.text=text; result.pressed.connect(callback); parent.add_child(result); return result

func open_preview() -> void:
	if busy: status.text="Wait for the current operation."; return
	if dirty: status.text="Save decisions before opening the full-route preview."; return
	if review.is_empty(): status.text="Open a saved review first."; return
	if review.document.geometry_source==null or review.document.elevation_source==null:
		status.text="Select Geometry and Elevation sources, then Save decisions before previewing."; return
	var shell := get_tree().get_first_node_in_group("app_shell")
	if not is_instance_valid(full_preview):
		full_preview=preload("res://full_route_preview.gd").new(); shell.register(full_preview,"Full-route sparse preview")
		full_preview.review_changed.connect(func(path: String):
			if path==directory and not busy and not dirty: submit({"action":"open","directory":directory}))
	shell.navigate(full_preview); full_preview.load_review(directory,review.revision)

func can_replace() -> bool:
	if busy: status.text="Wait for the current review operation."; return false
	if dirty: status.text="Save or discard your decisions before opening another review."; return false
	return true

func choose_sources() -> void:
	if not can_replace(): return
	var picker := FileDialog.new(); picker.access=FileDialog.ACCESS_FILESYSTEM; picker.file_mode=FileDialog.FILE_MODE_OPEN_FILES; picker.filters=PackedStringArray(["*.tcx,*.gpx ; Recording variants for one ride"]); add_child(picker)
	picker.files_selected.connect(func(paths: PackedStringArray):
		picker.queue_free()
		var folder := FileDialog.new(); folder.access=FileDialog.ACCESS_FILESYSTEM; folder.file_mode=FileDialog.FILE_MODE_OPEN_DIR; add_child(folder)
		folder.dir_selected.connect(func(parent: String):
			create_review(paths,parent.path_join("review-%d-%d" % [int(Time.get_unix_time_from_system()),Time.get_ticks_usec()])); folder.queue_free())
		folder.canceled.connect(folder.queue_free); folder.popup_centered_ratio(0.7))
	picker.canceled.connect(picker.queue_free); picker.popup_centered_ratio(0.7)

func choose_review() -> void:
	if not can_replace(): return
	var picker := FileDialog.new(); picker.access=FileDialog.ACCESS_FILESYSTEM; picker.file_mode=FileDialog.FILE_MODE_OPEN_FILE; picker.filters=PackedStringArray(["review.json ; Full ride review"]); add_child(picker)
	picker.file_selected.connect(func(path: String): open_review(path.get_base_dir()); picker.queue_free())
	picker.canceled.connect(picker.queue_free); picker.popup_centered_ratio(0.7)

func create_review(paths: PackedStringArray, destination: String) -> void:
	if can_replace(): submit({"action":"create","directory":destination,"paths":Array(paths)})

func open_review(path: String) -> void:
	if can_replace(): submit({"action":"open","directory":path})

func save_review() -> void:
	if busy or review.is_empty(): return
	var document: Dictionary=review.document.duplicate(true)
	# Godot parses JSON numbers as floats; preserve the native integer schema field.
	document.schema_version=int(document.schema_version)
	# Native save retains repair values; source/gap editing does not rewrite them.
	document.erase("repairs")
	submit({"action":"save","directory":directory,"revision":review.revision,"document":document})

func submit(request: Dictionary) -> void:
	if busy: return
	if not bridge.start_review(JSON.stringify(request)): status.text="Review worker is busy"; return
	busy=true; last_operation_ok=false; pending_directory=request.directory; status.text="Working… original recordings remain unchanged."; refresh_enabled()

func _process(_delta: float) -> void:
	if not busy: return
	var raw: String=bridge.poll_review()
	if raw.is_empty(): return
	busy=false
	var response: Dictionary=JSON.parse_string(raw)
	if not response.ok: status.text=str(response.message); refresh_enabled(); return
	last_operation_ok=true
	review=response.review; directory=pending_directory; dirty=false
	populate()
	var automatic_count := 0
	for repair in review.document.get("repairs",{}).values():
		if repair.proposal.kind=="automatic_grade_blend" and repair.status=="accepted": automatic_count+=1
	status.text="Saved review. %d automatic elevation repairs applied. Inspect or undo them in Full-route preview. Original files remain unchanged." % automatic_count

func populate() -> void:
	updating=true; view_source.clear(); geometry.clear(); elevation.clear(); comparison.text=""
	for choice in [geometry,elevation]: choice.add_item("Not selected"); choice.set_item_metadata(0,null)
	for index in range(review.activities.size()):
		var a: Dictionary=review.activities[index]
		view_source.add_item(a.source_name)
		for choice in [geometry,elevation]: choice.add_item(a.format+" "+a.source_name); choice.set_item_metadata(index+1,a.source_sha256)
		var count := 0
		for gap in review.gaps:
			if gap.source_sha256==a.source_sha256: count+=1
		comparison.text += "%s | %d samples | recorded %s | geometry %.2f km (gaps excluded) | %d breaks | %d diagnostics\n" % [a.source_name,a.samples.size(),"--" if a.recorded_distance_m==null else "%.2f km" % (a.recorded_distance_m/1000.0),a.route_distance_m/1000.0,count,a.diagnostics.size()]
	for pair in [[geometry,"geometry_source"],[elevation,"elevation_source"]]:
		for index in range(pair[0].item_count):
			if pair[0].get_item_metadata(index)==review.document[pair[1]]: pair[0].select(index)
	location.text=directory+" / review.json"; updating=false; show_source(0); refresh_enabled()

func choose_role(role: String, choice: OptionButton, index: int) -> void:
	if updating or busy or review.is_empty(): return
	review.document[role]=choice.get_item_metadata(index); changed()

func show_source(index: int) -> void:
	if review.is_empty(): return
	view_source.select(index)
	var a: Dictionary=review.activities[index]
	for plot in [map,profile]: plot.configure(a.samples,0,a.samples.size()-1,0)
	gaps.clear(); visible_gaps.clear(); active_gap=""
	for gap in review.gaps:
		if gap.source_sha256==a.source_sha256:
			visible_gaps.append(gap)
			gaps.add_item("Sample %d → %d | %.2f km | %s" % [gap.before_sample,gap.after_sample,gap.distance_m/1000.0,str(review.document.decisions.get(gap.id,{}).get("classification","unresolved"))])
	show_sample(0); refresh_enabled()

func show_sample(index: int) -> void:
	if review.is_empty(): return
	var a: Dictionary=review.activities[view_source.selected]; var s: Dictionary=a.samples[index]
	for plot in [map,profile]: plot.cursor=index; plot.queue_redraw()
	var messages: Array=[]
	for diagnostic in a.diagnostics:
		if diagnostic.sample==index or diagnostic.sample==null: messages.append(diagnostic.message)
	var timestamp := "missing" if s.time_ms==null else Time.get_datetime_string_from_unix_time(int(s.time_ms/1000.0))+" UTC"
	detail.text="%s | sample %d / segment %d | %.2f km | height %s m | %s\n%s" % [a.format,index,s.segment,s.route_distance_m/1000.0,str(s.elevation_m),timestamp,"; ".join(messages)]

func select_gap(index: int) -> void:
	var gap: Dictionary=visible_gaps[index]; active_gap=gap.id
	updating=true
	var decision: Dictionary=review.document.decisions.get(active_gap,{"classification":"unresolved","note":""})
	classification.select(CLASSIFICATIONS.find(decision.classification)); note.text=decision.note; updating=false
	show_sample(int(gap.after_sample))
	detail.text += "\nGap: %s | elapsed %s ms | endpoint separation %s m (not a proposed connector)" % [", ".join(gap.reasons),str(gap.elapsed_ms),str(gap.separation_m)]
	refresh_enabled()

func edit_gap() -> void:
	if updating or busy or active_gap.is_empty(): return
	review.document.decisions[active_gap]={"classification":CLASSIFICATIONS[classification.selected],"note":note.text}
	var index := gaps.get_selected_items()[0] if not gaps.get_selected_items().is_empty() else -1
	if index >= 0:
		var gap: Dictionary=visible_gaps[index]
		gaps.set_item_text(index,"Sample %d → %d | %.2f km | %s" % [gap.before_sample,gap.after_sample,gap.distance_m/1000.0,CLASSIFICATIONS[classification.selected]])
	changed()

func changed() -> void:
	dirty=true; status.text="Unsaved decisions — Save decisions to keep them."; refresh_enabled()

func refresh_enabled() -> void:
	save_button.disabled=busy or review.is_empty() or not dirty
	for choice in [view_source,geometry,elevation]: choice.disabled=busy or review.is_empty()
	classification.disabled=busy or active_gap.is_empty(); note.editable=not busy and not active_gap.is_empty()
