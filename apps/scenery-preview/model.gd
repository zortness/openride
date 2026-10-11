extends Node3D

var camera: Camera3D
var center := Vector3.ZERO
var distance := 1000.0
var yaw := 0.4
var pitch := 0.8

func _ready() -> void:
	var path := ""
	var snapshot := ""
	var zoom := 1.0
	for argument in OS.get_cmdline_user_args():
		if argument.begins_with("--model="):
			path = argument.trim_prefix("--model=")
		if argument.begins_with("--snapshot="):
			snapshot = argument.trim_prefix("--snapshot=")
		if argument.begins_with("--zoom="):
			zoom = clampf(argument.trim_prefix("--zoom=").to_float(), 1.0, 20.0)
	var document := GLTFDocument.new()
	var state := GLTFState.new()
	var error := document.append_from_file(path, state)
	if error != OK:
		push_error("GLB import failed: %s" % error)
		get_tree().quit(1)
		return
	var model := document.generate_scene(state)
	if model == null:
		get_tree().quit(1)
		return
	add_child(model)
	var bounds := AABB()
	var count := 0
	for node in model.find_children("*", "MeshInstance3D", true, false):
		var item: AABB = node.global_transform * node.get_aabb()
		bounds = item if count == 0 else bounds.merge(item)
		count += 1
	if count == 0:
		get_tree().quit(1)
		return
	center = bounds.get_center()
	distance = maxf(50.0, bounds.size.length() * 0.85)
	distance /= zoom
	camera = Camera3D.new()
	camera.far = maxf(20000.0, distance * 5.0)
	add_child(camera)
	_update_camera()
	var sun := DirectionalLight3D.new()
	sun.rotation_degrees = Vector3(-55, -30, 0)
	add_child(sun)
	var world := WorldEnvironment.new()
	world.environment = Environment.new()
	world.environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	world.environment.ambient_light_color = Color("c1cede")
	world.environment.ambient_light_energy = 0.65
	add_child(world)
	var layer := CanvasLayer.new()
	add_child(layer)
	var panel := PanelContainer.new()
	panel.set_anchors_and_offsets_preset(Control.PRESET_TOP_WIDE)
	layer.add_child(panel)
	var label := Label.new()
	label.text = "  OPENRIDE / OSM2World comparison\n  Default converter scenery · No USGS elevation or corrected OpenRide road profile\n  %d mesh instances · extent %.0f × %.0f × %.0f m · drag to orbit / wheel to zoom\n  OSM © OpenStreetMap contributors · ODbL · Converter/assets retain their own licenses" % [count, bounds.size.x, bounds.size.y, bounds.size.z]
	panel.add_child(label)
	await get_tree().process_frame
	if not snapshot.is_empty():
		await RenderingServer.frame_post_draw
		error = get_viewport().get_texture().get_image().save_png(snapshot)
		print("MODEL_PREVIEW_OK meshes=", count, " bounds=", bounds)
		get_tree().quit(error)

func _update_camera() -> void:
	camera.position = center + Vector3(sin(yaw) * cos(pitch), sin(pitch), cos(yaw) * cos(pitch)) * distance
	camera.look_at(center)

func _unhandled_input(event: InputEvent) -> void:
	if camera == null:
		return
	if event is InputEventMouseMotion and event.button_mask & MOUSE_BUTTON_MASK_LEFT:
		yaw -= event.relative.x * 0.006
		pitch = clampf(pitch + event.relative.y * 0.006, 0.1, 1.5)
		_update_camera()
	if event is InputEventMouseButton and event.pressed:
		if event.button_index == MOUSE_BUTTON_WHEEL_UP:
			distance = maxf(10.0, distance * 0.85)
		elif event.button_index == MOUSE_BUTTON_WHEEL_DOWN:
			distance = minf(camera.far / 2.0, distance / 0.85)
		_update_camera()
