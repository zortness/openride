extends "res://screen.gd"
signal package_selected(path: String)
signal ride_selected(path:String)
signal refreshed
var directory := ""
var entries: Array = []
var listing: ItemList
var location: Label
var picker: FileDialog
var ride_button: Button
var package_worker: Thread
var package_action := ""
var package_preview := true

func get_ok_button() -> Button:
	return ride_button

func _ready() -> void:
	title="Offline routes"
	ride_button = Button.new(); ride_button.text = "Ride selected route"
	get_ok_button().disabled=true
	var config:=ConfigFile.new()
	var default_path:=OS.get_executable_path().get_base_dir().path_join("content") if not OS.has_feature("editor") else ProjectSettings.globalize_path("user://content")
	config.load("user://content-library.cfg")
	directory=config.get_value("library","directory",default_path)
	var box:=VBoxContainer.new(); box.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT); add_child(box)
	var heading := Label.new(); heading.text = "CHOOSE YOUR ROUTE"; heading.add_theme_font_size_override("font_size",28); box.add_child(heading)
	location=Label.new();location.custom_minimum_size.x=620;location.autowrap_mode=TextServer.AUTOWRAP_WORD_SMART;box.add_child(location)
	var row:=HBoxContainer.new();box.add_child(row)
	for caption in ["Choose folder","Refresh"]:
		var button:=Button.new();button.text=caption;button.custom_minimum_size.y=40;row.add_child(button)
		button.pressed.connect(func():
			if caption=="Choose folder":picker.popup_centered_ratio(0.7)
			else:refresh())
	var open_zip := Button.new(); open_zip.text = "Open ZIP…"; open_zip.custom_minimum_size.y = 40; row.add_child(open_zip); open_zip.pressed.connect(choose_zip)
	var export_zip := Button.new(); export_zip.text = "Export selected ZIP…"; export_zip.custom_minimum_size.y = 40; row.add_child(export_zip); export_zip.pressed.connect(choose_export)
	listing=ItemList.new();listing.custom_minimum_size=Vector2(650,300); listing.size_flags_vertical=Control.SIZE_EXPAND_FILL; box.add_child(listing)
	listing.item_selected.connect(func(index:int):get_ok_button().disabled=entries[index].has("error"))
	listing.item_activated.connect(func(index:int):
		if not entries[index].has("error"):open_selected())
	var actions := HBoxContainer.new(); box.add_child(actions); actions.add_child(ride_button)
	ride_button.custom_minimum_size.y = 48; ride_button.pressed.connect(open_selected)
	var preview := Button.new(); preview.text = "Preview selected route"; preview.custom_minimum_size.y = 48; actions.add_child(preview); preview.pressed.connect(func(): open_selected(true))
	picker=FileDialog.new(); picker.display_mode = FileDialog.DISPLAY_LIST;picker.access=FileDialog.ACCESS_FILESYSTEM;picker.file_mode=FileDialog.FILE_MODE_OPEN_DIR;add_child(picker)
	picker.dir_selected.connect(func(path:String):
		directory=path
		config.set_value("library","directory",directory);config.save("user://content-library.cfg")
		refresh())

func open_selected(preview:bool=false) -> void:
	var selected:=listing.get_selected_items()
	if selected.is_empty():return
	if str(entries[selected[0]].path).to_lower().ends_with(".zip"):
		install_zip(entries[selected[0]].path, preview); return
	var entry:Dictionary=validate(entries[selected[0]].path)
	if entry.has("error"):
		refresh();popup_centered();return
	if preview:package_selected.emit(entry.path)
	else:ride_selected.emit(entry.path)

func choose_zip() -> void:
	var dialog := FileDialog.new(); dialog.access = FileDialog.ACCESS_FILESYSTEM; dialog.file_mode = FileDialog.FILE_MODE_OPEN_FILE
	dialog.filters = PackedStringArray(["*.zip ; Ride ZIP packages"]); add_child(dialog)
	dialog.file_selected.connect(func(path: String): install_zip(path, true); dialog.queue_free())
	dialog.canceled.connect(dialog.queue_free); dialog.popup_centered_ratio(0.75)

func choose_export() -> void:
	var selected := listing.get_selected_items()
	if selected.is_empty(): return
	var path: String = entries[selected[0]].path
	if path.to_lower().ends_with(".zip"): location.text = "This ride is already a ZIP. Share the selected file:\n" + path; return
	var dialog := FileDialog.new(); dialog.access = FileDialog.ACCESS_FILESYSTEM; dialog.file_mode = FileDialog.FILE_MODE_SAVE_FILE
	dialog.filters = PackedStringArray(["*.zip ; Ride ZIP packages"]); dialog.current_file = "ride.zip"; add_child(dialog)
	dialog.file_selected.connect(func(destination: String): run_package("pack", path, destination, false); dialog.queue_free())
	dialog.canceled.connect(dialog.queue_free); dialog.popup_centered_ratio(0.75)

func install_zip(path: String, preview := true) -> void:
	run_package("install", path, ProjectSettings.globalize_path("user://package-cache"), preview)

func run_package(action: String, source: String, destination: String, preview: bool) -> void:
	if package_worker != null: return
	var executable := ProjectSettings.globalize_path("res://bin/openride-author.exe") if OS.has_feature("editor") else OS.get_executable_path().get_base_dir().path_join("authoring/openride-author.exe")
	package_action = action; package_preview = preview
	package_worker = Thread.new(); location.text = "Validating and %s ZIP package…" % ("exporting" if action == "pack" else "opening")
	var error := package_worker.start(func():
		var output: Array = []; var code := OS.execute(executable, PackedStringArray(["package", action, source, destination]), output, true, false)
		return {"code": code, "output": "\n".join(output)})
	if error != OK: package_worker = null; location.text = "Could not start native ZIP tool."

func _process(_delta: float) -> void:
	if package_worker == null or package_worker.is_alive(): return
	var result: Dictionary = package_worker.wait_to_finish(); package_worker = null
	var terminal: Dictionary = {}
	for line in str(result.output).split("\n"):
		if line.strip_edges().is_empty(): continue
		var parser := JSON.new()
		if parser.parse(line) != OK: terminal = {}; break
		var value = parser.data
		if value is Dictionary and value.get("protocol_version") == 1 and value.get("event") == "result": terminal = value
	if result.code != 0 or terminal.get("status") != "complete":
		location.text = str(terminal.get("error", {}).get("message", "Native ZIP operation failed.")); return
	var package: Dictionary = terminal.package
	if package_action == "pack": location.text = "ZIP exported:\n" + str(package.path); return
	var entry := validate(package.path)
	if entry.has("error"): location.text = entry.error; return
	location.text = "ZIP opened from the managed offline cache."
	if package_preview: package_selected.emit(entry.path)
	else: ride_selected.emit(entry.path)

func _exit_tree() -> void:
	if package_worker != null and package_worker.is_started(): package_worker.wait_to_finish()

func refresh() -> void:
	var selected_path := ""
	var selected := listing.get_selected_items()
	if not selected.is_empty() and selected[0]<entries.size(): selected_path = entries[selected[0]].path
	entries.clear();listing.clear();get_ok_button().disabled=true
	location.text="Drop route folders or ZIP packages into:\n"+directory
	if not DirAccess.dir_exists_absolute(directory):
		location.text+="\nFolder not found. Create it or choose an existing folder.";refreshed.emit();return
	var folders:=DirAccess.get_directories_at(directory);folders.sort()
	if FileAccess.file_exists(directory.path_join("manifest.json")):
		var entry := validate(directory); entry["path"] = directory; entries.append(entry)
		listing.add_item(entry.error if entry.has("error") else entry.name); listing.set_item_disabled(listing.item_count-1,entry.has("error"))
	for folder in folders:
		if folder==".openride-assembly":continue
		var path:=directory.path_join(folder)
		var entry:=validate(path);entry["path"]=path;entries.append(entry)
		listing.add_item(folder+" - "+entry.error if entry.has("error") else entry.name)
		listing.set_item_disabled(listing.item_count-1,entry.has("error"))
	var archives := DirAccess.get_files_at(directory); archives.sort()
	for archive in archives:
		if not archive.to_lower().ends_with(".zip"): continue
		entries.append({"name": archive, "path": directory.path_join(archive), "packed": true}); listing.add_item(archive + " · ZIP")
	if entries.is_empty():location.text+="\nNo packages found."
	var preferred := -1
	for i in range(entries.size()):
		if not entries[i].has("error") and (preferred<0 or entries[i].path==selected_path): preferred=i
	if preferred>=0: listing.select(preferred); get_ok_button().disabled=false
	refreshed.emit()

static func validate(path:String) -> Dictionary:
	var file:=FileAccess.open(path.path_join("manifest.json"),FileAccess.READ)
	if file==null or file.get_length()>1024*1024:return {"error":"Missing or oversized manifest"}
	var m:Variant=JSON.parse_string(file.get_as_text())
	if not m is Dictionary or m.get("kind")!="openride.stream-preview" or m.get("schema_version")!=1 or m.get("trainer_ready")!=false:
		return {"error":"Unsupported package type/version"}
	if not m.get("name") is String or not m.get("chunks") is Array or m.chunks.is_empty() or m.chunks.size()>4096 or not m.get("route") is Dictionary or m.route.get("path")!="route.json":
		return {"error":"Invalid package index"}
	if m.get("tile_m")!=256 or m.get("max_live_chunks")!=9 or m.get("neighborhood_radius_cells")!=1 or not m.get("attribution") is Dictionary:
		return {"error":"Unsupported streaming settings"}
	if not m.get("presentation") is Dictionary or not m.presentation.get("terrain_corridor",false):return {"error":"Riding surface missing"}
	var payloads:Array=[m.route];var seen:={}
	if m.has("terrain_provenance"):
		if not m.terrain_provenance is Dictionary or m.terrain_provenance.get("path")!="terrain-provenance.json":return {"error":"Invalid terrain provenance path"}
		payloads.append(m.terrain_provenance)
	if m.has("coast"):
		if not m.coast is Dictionary or m.coast.get("path")!="coast.json":return {"error":"Invalid coast path"}
		payloads.append(m.coast)
	if m.has("detail"):
		if not m.detail is Dictionary or m.detail.get("path")!="presentation.json":return {"error":"Invalid road metadata path"}
		payloads.append(m.detail)
	if m.has("imagery"):
		if not m.imagery is Dictionary or m.imagery.get("path")!="imagery.png":return {"error":"Invalid imagery path"}
		payloads.append(m.imagery)
	for entry in m.chunks:
		if not entry is Dictionary:return {"error":"Invalid chunk"}
		for key in ["x","z"]:
			var value:Variant=entry.get(key)
			if not (value is float or value is int) or not is_finite(value) or value!=floor(value) or absf(value)>10000:return {"error":"Invalid chunk coordinate"}
		if entry.get("path")!="chunk_%d_%d.json" % [entry.x,entry.z] or seen.has(entry.path):return {"error":"Unsafe or duplicate chunk path"}
		seen[entry.path]=true;payloads.append(entry)
	for entry in payloads:
		var size:Variant=entry.get("bytes")
		var limit:=8*1024*1024 if entry.path=="route.json" else (16*1024*1024 if entry.path=="imagery.png" else 2*1024*1024)
		if not (size is float or size is int) or size<=0 or size>limit:return {"error":"Invalid payload size"}
		if not entry.get("sha256") is String or entry.sha256.length()!=64:return {"error":"Missing payload checksum"}
		var payload:=FileAccess.open(path.path_join(entry.path),FileAccess.READ)
		if payload==null or payload.get_length()!=size:return {"error":"Missing/incomplete payload: "+entry.path}
	# Geometry and checksums are validated by the runtime as payloads load.
	return {"name":m.name,"path":path}
