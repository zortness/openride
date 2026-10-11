extends AcceptDialog
signal package_selected(path: String)
var directory := "user://content"
var entries: Array = []
var listing: ItemList
var location: Label
var folder_picker: FileDialog

func _ready() -> void:
	title = "Local route library"
	get_ok_button().text = "Open selected"
	var box := VBoxContainer.new()
	add_child(box)
	location = Label.new()
	location.custom_minimum_size.x = 600
	location.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	box.add_child(location)
	var buttons := HBoxContainer.new()
	box.add_child(buttons)
	for caption in ["Choose folder", "Refresh"]:
		var button := Button.new()
		button.text = caption
		buttons.add_child(button)
		button.pressed.connect(func():
			if caption == "Choose folder": folder_picker.popup_centered_ratio(0.7)
			else: refresh())
	listing = ItemList.new()
	listing.custom_minimum_size = Vector2(620,260)
	box.add_child(listing)
	confirmed.connect(func():
		var selection := listing.get_selected_items()
		if not selection.is_empty():
			var entry: Dictionary = entries[selection[0]]
			var checked := validate(entry.directory)
			if checked.has("error"):
				refresh()
				popup_centered(Vector2i(680,440))
			else: package_selected.emit(checked.path))
	folder_picker = FileDialog.new(); folder_picker.display_mode = FileDialog.DISPLAY_LIST
	folder_picker.access = FileDialog.ACCESS_FILESYSTEM
	folder_picker.file_mode = FileDialog.FILE_MODE_OPEN_DIR
	folder_picker.dir_selected.connect(func(path: String): directory = path; refresh())
	add_child(folder_picker)

func refresh() -> void:
	entries.clear()
	listing.clear()
	location.text = "Drop package folders here, then Refresh:\n" + ProjectSettings.globalize_path(directory)
	if not DirAccess.dir_exists_absolute(directory):
		location.text += "\nFolder does not exist yet. Choose an existing content folder."
		return
	var folders := DirAccess.get_directories_at(directory)
	folders.sort()
	for folder in folders:
		var path := directory.path_join(folder)
		var entry := validate(path)
		entry["directory"] = path
		entries.append(entry)
		listing.add_item(folder + " — " + entry.error if entry.has("error") else "%s / %s · preview" % [entry.name,entry.version])
		listing.set_item_disabled(listing.item_count-1,entry.has("error"))
	if entries.is_empty(): location.text += "\nNo package folders found."

static func read_json(path: String, limit: int) -> Variant:
	var file := FileAccess.open(path,FileAccess.READ)
	if file == null or file.get_length()>limit: return null
	return JSON.parse_string(file.get_as_text())

static func validate(path: String) -> Dictionary:
	var manifest: Variant = read_json(path.path_join("manifest.json"),65536)
	if not manifest is Dictionary:
		return {"error":"Incomplete package: missing/invalid manifest"}
	if manifest.get("kind") != "openride.preview-package" or manifest.get("schema_version") != 1 or manifest.get("trainer_ready") != false:
		return {"error":"Unsupported package version or type"}
	if manifest.get("required_capabilities") != ["scenery-preparation-v1"]:
		return {"error":"Unsupported reader capabilities"}
	if manifest.get("dependencies") != []:
		return {"error":"External dependencies unsupported; use a self-contained package"}
	if not manifest.get("name") is String or not manifest.get("content_version") is String or not manifest.get("files") is Array or manifest.files.size() != 2:
		return {"error":"Invalid package metadata"}
	var seen := {}
	for entry in manifest.files:
		if not entry is Dictionary or entry.get("path") not in ["preparation.json","attribution.json"] or seen.has(entry.path):
			return {"error":"Unsupported or duplicate payload path"}
		seen[entry.path] = true
		var limit := 16*1024*1024 if entry.path == "preparation.json" else 65536
		var payload := path.path_join(entry.path)
		var file := FileAccess.open(payload,FileAccess.READ)
		if file == null or file.get_length()>limit or file.get_length()!=entry.get("bytes"):
			return {"error":"Missing or incomplete payload: " + entry.path}
		if FileAccess.get_sha256(payload)!=entry.get("sha256"):
			return {"error":"Checksum mismatch: " + entry.path}
	var preparation: Variant = read_json(path.path_join("preparation.json"),16*1024*1024)
	var credits: Variant = read_json(path.path_join("attribution.json"),65536)
	if not preparation is Dictionary or preparation.get("kind") != "openride.scenery-preparation" or preparation.get("schema_version") != 1 or preparation.get("trainer_ready") != false:
		return {"error":"Unsupported preparation"}
	if not credits is Dictionary or credits.get("kind") != "openride.content-attribution" or credits.get("schema_version") != 1:
		return {"error":"Missing content attribution"}
	if manifest.get("profile") not in ["points","corrected_profile","reviewed_profile","route_profile"] or not preparation.has(manifest.profile):
		return {"error":"Missing selected route profile"}
	return {"name":manifest.name,"version":manifest.content_version,"path":path.path_join("preparation.json")}
