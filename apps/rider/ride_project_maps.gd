extends VBoxContainer
var owner_screen: Control
var summary: Label
var detail_summary: Label
var prepare_button:Button
var plan_button: Button
var terrain_button: Button
var streets_button: Button
var imagery_button: Button
var coverage: ItemList
var map: Control

func setup(screen: Control) -> void:
	owner_screen = screen; add_theme_constant_override("separation", 10)
	screen.text(self, "Download road maps", 22)
	summary = screen.text(self, "Plan the ride corridor, then download roads and topography. Imagery is optional.", 15)
	prepare_button=screen.add_button(self,"Download maps & infer roads",screen.prepare_roads)
	var details: VBoxContainer = screen.foldout(self,"Map coverage, individual downloads and optional imagery")
	var row := HBoxContainer.new(); details.add_child(row)
	plan_button = screen.add_button(row, "Plan coverage", func(): screen.run_job("plan-maps", screen.project_path))
	terrain_button = screen.add_button(row, "Download topography", func(): acquire("terrain"))
	streets_button = screen.add_button(row, "Download roads", func(): acquire("streets"))
	detail_summary=screen.text(details,"",14)
	imagery_button = screen.add_button(details, "Acquire / resume optional imagery", func(): acquire("imagery"))
	coverage = ItemList.new(); coverage.custom_minimum_size.y = 100; details.add_child(coverage)
	coverage.item_selected.connect(inspect_tile)
	map = preload("res://ride_project_coverage.gd").new(); details.add_child(map)
	screen.text(self, "Downloads use USGS 3DEP and OpenStreetMap. Completed verified tiles are reused; Cancel keeps them for the next attempt.", 14)

func acquire(layer: String) -> void:
	owner_screen.run_job("acquire", owner_screen.project_path, PackedStringArray(["--layer", layer]))

func populate() -> void:
	coverage.clear()
	var plan: Dictionary = owner_screen.snapshot.get("maps", {})
	map.plan = plan; map.samples = owner_screen.snapshot.get("repair_preview", {}).get("samples", []); map.queue_redraw()
	map.imagery_texture = null; map.focus_geometry = []
	if plan.is_empty(): summary.text = "Plan the ride corridor, then download roads and topography. Imagery is optional."; return
	map.roads = []
	var preview_path: String = owner_screen.project_path.path_join("maps").path_join(plan.key).path_join("streets-preview.json")
	var street_note := ""
	if FileAccess.file_exists(preview_path):
		var preview = JSON.parse_string(FileAccess.get_file_as_string(preview_path))
		if preview is Dictionary and preview.get("roads") is Array:
			map.roads = preview.roads
			street_note = "\nStreet overlay: %d / %d ways (display budget; full graph retained)" % [preview.shown_ways, preview.total_ways]
	detail_summary.text = "%d corridor tiles · %.1f m terrain samples · %.1f MiB raw elevation estimate plus metadata\nTopography %d/%d verified · Streets %d/%d verified" % [plan.tiles.size(), plan.resolution_m, plan.estimated_terrain_bytes / 1048576.0, plan.terrain_ready, plan.tiles.size(), plan.streets_ready, plan.tiles.size()]
	detail_summary.text += street_note
	detail_summary.text += "\nImagery %d/%d verified · about 2 m pixels · %.1f MiB raw estimate. Select a region to inspect its image." % [plan.get("imagery_ready", 0), plan.tiles.size(), plan.get("estimated_imagery_bytes", 0) / 1048576.0]
	summary.text="Road maps %d/%d · Topography %d/%d · Downloads resume from verified coverage."%[plan.streets_ready,plan.tiles.size(),plan.terrain_ready,plan.tiles.size()]
	for tile in plan.tiles:
		var layers:Array=plan.get("tile_layers",{}).get(tile.id,[])
		coverage.add_item("%.4f, %.4f → %.4f, %.4f · %s" % [tile.bounds[1],tile.bounds[0],tile.bounds[3],tile.bounds[2],", ".join(layers) if not layers.is_empty() else "No verified layers"])

func inspect_tile(index: int) -> void:
	var plan: Dictionary = owner_screen.snapshot.get("maps", {})
	if plan.is_empty() or index >= plan.tiles.size(): return
	var tile: Dictionary = plan.tiles[index]
	map.focus_geometry = [[tile.bounds[1], tile.bounds[0]], [tile.bounds[3], tile.bounds[2]]]
	map.imagery_texture = null
	var directory: String = owner_screen.project_path.path_join("maps").path_join(plan.key).path_join(tile.id)
	var path := directory.path_join("imagery.png")
	if FileAccess.file_exists(directory.path_join("imagery.receipt.json")) and FileAccess.file_exists(path):
		var receipt = JSON.parse_string(FileAccess.get_file_as_string(directory.path_join("imagery.receipt.json")))
		if receipt is Dictionary and receipt.get("imagery.png", "") == FileAccess.get_sha256(path):
			var image := Image.load_from_file(path)
			if image != null and image.get_width() == 1024 and image.get_height() == 1024:
				map.imagery_texture = ImageTexture.create_from_image(image); map.imagery_bounds = tile.bounds
	map.queue_redraw()

func refresh() -> void:
	var unavailable: bool = owner_screen.busy or not owner_screen.snapshot.has("repair_preview")
	prepare_button.disabled=unavailable
	plan_button.disabled = unavailable
	terrain_button.disabled = unavailable or not owner_screen.snapshot.has("maps")
	streets_button.disabled = terrain_button.disabled
	imagery_button.disabled = terrain_button.disabled
