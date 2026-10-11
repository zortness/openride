extends "res://screen.gd"

signal export_requested(path: String)
var directory := "user://sessions"
var entries: Array = []
var listing: ItemList
var summary: Label
var chart: Control
var export_button: Button
var queue: Array[String] = []
var file: FileAccess
var current: Dictionary = {}
var selected_path := ""

func _ready() -> void:
	title = "Activities"
	var box := VBoxContainer.new(); box.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT); add_child(box)
	var heading := Label.new(); heading.text = "YOUR RIDES"; heading.add_theme_font_size_override("font_size",28); box.add_child(heading)
	var refresh_button := Button.new(); refresh_button.text = "Refresh recordings"; refresh_button.pressed.connect(refresh); box.add_child(refresh_button)
	listing = ItemList.new(); listing.size_flags_vertical = Control.SIZE_EXPAND_FILL; listing.custom_minimum_size.y = 180; box.add_child(listing)
	listing.item_selected.connect(select)
	summary = Label.new(); summary.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART; box.add_child(summary)
	chart = PowerChart.new(); chart.custom_minimum_size.y = 160; box.add_child(chart)
	export_button = Button.new(); export_button.text = "Export selected activity"; export_button.disabled = true; box.add_child(export_button)
	export_button.pressed.connect(func(): if not selected_path.is_empty(): export_requested.emit(selected_path))

func refresh() -> void:
	if file != null: file.close(); file = null
	entries.clear(); queue.clear(); listing.clear(); export_button.disabled = true
	chart.samples = []; chart.queue_redraw()
	var folders := DirAccess.get_directories_at(directory); folders.sort(); folders.reverse()
	for folder in folders:
		var path := directory.path_join(folder).path_join("session.jsonl")
		if FileAccess.file_exists(path): queue.append(path)
	summary.text = "Reading local recordings…" if not queue.is_empty() else "No recorded activities yet. Finished and interrupted rides will appear here."

func _process(_delta: float) -> void:
	# Bounded work per frame; never block navigation on long journals.
	var deadline := Time.get_ticks_usec()+3000
	while Time.get_ticks_usec()<deadline:
		if file == null:
			if queue.is_empty(): return
			var path: String = queue.pop_front()
			current = {"path":path,"name":path.get_base_dir().get_file(),"elapsed":0.0,"distance":0.0,"complete":false,"reason":"","route":false,"samples":[],"invalid":0,"utc":0.0}
			file = FileAccess.open(path,FileAccess.READ)
			if file == null: continue
			if file.get_length()>128*1024*1024:
				current.reason = "Journal exceeds 128 MiB review limit"; finish_file(); continue
		if file.eof_reached(): finish_file(); continue
		var line := file.get_line()
		if line.is_empty(): continue
		var row: Variant = JSON.parse_string(line)
		if not row is Dictionary: current.invalid += 1; continue
		current.elapsed = maxf(current.elapsed,float(row.get("elapsed_ms",0))/1000.0)
		if row.get("event") == "session_check_created": current.utc = float(row.get("utc_unix_ms",0))/1000.0
		if row.get("event") == "route_session_config": current.route = true
		if row.get("event") == "session_check_complete": current.complete = true; current.reason = str(row.get("reason","Session finished"))
		if row.get("event") == "virtual_state":
			current.distance = maxf(current.distance,float(row.get("distance_m",0)))
			if row.get("power_w") != null and (current.samples.is_empty() or float(row.get("elapsed_ms",0))/1000.0-current.samples[-1][0]>=2):
				current.samples.append([float(row.get("elapsed_ms",0))/1000.0,float(row.power_w)])
				if current.samples.size()>4000: current.samples = current.samples.slice(0,current.samples.size(),2)
		elif row.get("event")=="indoor_bike_data" and not current.route:
			var decoded: Dictionary = row.get("decoded",{}).get("Ok",{})
			if decoded.get("power_w")!=null and (current.samples.is_empty() or current.elapsed-current.samples[-1][0]>=2):
				current.samples.append([current.elapsed,float(decoded.power_w)])
				if current.samples.size()>4000: current.samples=current.samples.slice(0,current.samples.size(),2)

func finish_file() -> void:
	file.close(); file = null
	entries.append(current)
	entries.sort_custom(func(a,b): return a.utc>b.utc)
	listing.clear()
	var selection := 0
	for i in range(entries.size()):
		var entry: Dictionary = entries[i]
		var stamp: String = Time.get_datetime_string_from_unix_time(int(entry.utc)).replace("T"," ")+" UTC" if entry.utc>0 else entry.name
		var state: String = "Finished" if entry.complete else "Interrupted / incomplete"
		if not entry.route: state = "Trainer check · "+state
		listing.add_item("%s  ·  %s  ·  %.1f km  ·  %d:%02d" % [stamp,state,entry.distance/1000,int(entry.elapsed)/60,int(entry.elapsed)%60])
		if entry.path==selected_path: selection=i
	listing.select(selection); select(selection)

func select(index: int) -> void:
	var entry: Dictionary = entries[index]; selected_path = entry.path
	var reason: String = {"route_complete":"Route completed","user_finish":"Ride ended"}.get(entry.reason,"Recording finished" if entry.complete else "Interrupted recording")
	summary.text = "%s · %.1f km · %d:%02d elapsed%s\nPower (W) over elapsed time" % [reason,entry.distance/1000,int(entry.elapsed)/60,int(entry.elapsed)%60," · %d unreadable records" % entry.invalid if entry.invalid else ""]
	chart.samples = entry.samples; chart.queue_redraw(); export_button.disabled = false

class PowerChart extends Control:
	var samples: Array = []
	func _draw() -> void:
		draw_rect(Rect2(Vector2.ZERO,size),Color("12242e"))
		if samples.size()<2: return
		var peak := 1.0
		for s in samples: peak = maxf(peak,s[1])
		var duration: float = maxf(1,samples[-1][0]-samples[0][0]); var points := PackedVector2Array()
		for s in samples: points.append(Vector2(12+(size.x-24)*(s[0]-samples[0][0])/duration,size.y-12-(size.y-24)*s[1]/peak))
		draw_polyline(points,Color("65dfd0"),2,true)
