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
	var entry:Dictionary=validate(entries[selected[0]].path)
	if entry.has("error"):
		refresh();popup_centered();return
	if preview:package_selected.emit(entry.path)
	else:ride_selected.emit(entry.path)

func refresh() -> void:
	var selected_path := ""
	var selected := listing.get_selected_items()
	if not selected.is_empty() and selected[0]<entries.size(): selected_path = entries[selected[0]].path
	entries.clear();listing.clear();get_ok_button().disabled=true
	location.text="Drop route package folders into:\n"+directory
	if not DirAccess.dir_exists_absolute(directory):
		location.text+="\nFolder not found. Create it or choose an existing folder.";refreshed.emit();return
	var folders:=DirAccess.get_directories_at(directory);folders.sort()
	for folder in folders:
		var path:=directory.path_join(folder)
		var entry:=validate(path);entry["path"]=path;entries.append(entry)
		listing.add_item(folder+" - "+entry.error if entry.has("error") else entry.name)
		listing.set_item_disabled(listing.item_count-1,entry.has("error"))
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
		if not (size is float or size is int) or size<=0 or size>(16*1024*1024 if entry.path=="imagery.png" else 2*1024*1024):return {"error":"Invalid payload size"}
		if not entry.get("sha256") is String or entry.sha256.length()!=64:return {"error":"Missing payload checksum"}
		var payload:=FileAccess.open(path.path_join(entry.path),FileAccess.READ)
		if payload==null or payload.get_length()!=size:return {"error":"Missing/incomplete payload: "+entry.path}
	# Geometry and checksums are validated by the runtime as payloads load.
	return {"name":m.name,"path":path}
