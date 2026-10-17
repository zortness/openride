extends Control
## A fictional, looping coastal ride for the Home screen. It never uses saved routes.

const PERIOD_M := 640.0
const SPEED_MPS := 12.0
const ROAD_HALF_WIDTH := 3.7
const ROAD_Y := 0.035
const MARKING_Y := 0.095

var viewport: SubViewport
var camera: Camera3D
var progress_m := 0.0

func _ready() -> void:
	set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	var frame := SubViewportContainer.new()
	frame.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	frame.stretch = true
	frame.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(frame)
	viewport = SubViewport.new()
	viewport.size = Vector2i(1280, 720)
	viewport.own_world_3d = true
	viewport.msaa_3d = Viewport.MSAA_4X
	viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	frame.add_child(viewport)
	var scene := Node3D.new()
	viewport.add_child(scene)
	var environment := WorldEnvironment.new()
	var sky := Environment.new()
	sky.background_mode = Environment.BG_COLOR
	sky.background_color = Color("a9c8d3")
	sky.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	sky.ambient_light_color = Color("d7e2dc")
	sky.ambient_light_energy = 0.8
	sky.tonemap_mode = Environment.TONE_MAPPER_FILMIC
	environment.environment = sky
	scene.add_child(environment)
	var sunlight := DirectionalLight3D.new()
	sunlight.rotation_degrees = Vector3(-48, -32, 0)
	sunlight.light_color = Color("ffebc5")
	sunlight.light_energy = 1.25
	scene.add_child(sunlight)
	build_scene(scene)
	camera = Camera3D.new()
	camera.current = true
	camera.fov = 76.0
	camera.far = 430.0
	scene.add_child(camera)
	place_camera()
	visibility_changed.connect(func():
		viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS if is_visible_in_tree() else SubViewport.UPDATE_DISABLED)

func _process(delta: float) -> void:
	if not is_visible_in_tree(): return
	progress_m = fposmod(progress_m + minf(delta, 0.1) * SPEED_MPS, PERIOD_M)
	place_camera()

func road_x(distance_m: float) -> float:
	var phase := TAU * distance_m / PERIOD_M
	return sin(phase) * 5.0 + sin(phase * 2.0) * 1.8

func place_camera() -> void:
	var x := road_x(progress_m)
	camera.position = Vector3(x + 0.4, 2.15, -progress_m)
	camera.look_at(Vector3(road_x(progress_m + 21.0), 1.75, -(progress_m + 21.0)), Vector3.UP)

func material(color: Color, roughness := 1.0) -> StandardMaterial3D:
	var result := StandardMaterial3D.new()
	result.albedo_color = color
	result.roughness = roughness
	return result

func box(parent: Node3D, center: Vector3, size: Vector3, surface: Material) -> void:
	var instance := MeshInstance3D.new()
	var mesh := BoxMesh.new()
	mesh.size = size
	instance.mesh = mesh
	instance.material_override = surface
	instance.position = center
	parent.add_child(instance)

func strip(parent: Node3D, width: float, offset: float, surface: Material, dashed := false, height_m := ROAD_Y) -> void:
	var vertices := PackedVector3Array()
	for step in range(-8, 161):
		var start := float(step) * 8.0
		if dashed and step % 3 != 0: continue
		var end := start + (4.0 if dashed else 8.0)
		var a := Vector3(road_x(start) + offset - width / 2.0, height_m, -start)
		var b := Vector3(road_x(start) + offset + width / 2.0, height_m, -start)
		var c := Vector3(road_x(end) + offset - width / 2.0, height_m, -end)
		var d := Vector3(road_x(end) + offset + width / 2.0, height_m, -end)
		vertices.append_array(PackedVector3Array([a,b,c,b,d,c]))
	var mesh := ArrayMesh.new()
	var arrays := []
	arrays.resize(Mesh.ARRAY_MAX)
	arrays[Mesh.ARRAY_VERTEX] = vertices
	var normals := PackedVector3Array()
	normals.resize(vertices.size())
	normals.fill(Vector3.UP)
	arrays[Mesh.ARRAY_NORMAL] = normals
	mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
	var instance := MeshInstance3D.new()
	instance.mesh = mesh
	if surface is BaseMaterial3D: surface.cull_mode = BaseMaterial3D.CULL_DISABLED
	instance.material_override = surface
	parent.add_child(instance)

func build_scene(scene: Node3D) -> void:
	var ocean := material(Color("477e91"), 0.35)
	var sand := material(Color("d3bd95"))
	var land := material(Color("829b81"))
	var asphalt := material(Color("42515b"))
	var sidewalk := material(Color("b8b5a6"))
	var line := material(Color("e6d9a6"))
	var white := material(Color("e5e1d3"))
	var trunk := material(Color("6a6250"))
	var leaves := [material(Color("577b63")),material(Color("719067")),material(Color("426c64"))]
	var walls := [material(Color("d5c8b7")),material(Color("b9c6ba")),material(Color("dfd6c6")),material(Color("a9b8b7"))]
	var roofs := [material(Color("727c7d")),material(Color("9b8b7d")),material(Color("839397"))]
	box(scene, Vector3(-260, -0.19, -600), Vector3(490, 0.2, 1450), ocean)
	box(scene, Vector3(-10, -0.16, -600), Vector3(12, 0.22, 1450), sand)
	box(scene, Vector3(255, -0.19, -600), Vector3(500, 0.2, 1450), land)
	strip(scene, ROAD_HALF_WIDTH * 2.0 + 0.9, 0.0, asphalt)
	strip(scene, 0.15, 0.0, line, true, MARKING_Y)
	strip(scene, 0.14, -ROAD_HALF_WIDTH + 0.25, white, false, MARKING_Y)
	strip(scene, 0.14, ROAD_HALF_WIDTH - 0.25, white, false, MARKING_Y)
	strip(scene, 2.4, -5.35, sidewalk)
	strip(scene, 2.3, 5.25, sidewalk)
	var tree_mesh := SphereMesh.new()
	tree_mesh.radius = 1.0
	tree_mesh.height = 2.0
	var trunk_mesh := CylinderMesh.new()
	trunk_mesh.top_radius = 0.13
	trunk_mesh.bottom_radius = 0.19
	trunk_mesh.height = 3.5
	for i in range(-2, 66):
		var distance_m := float(i) * 20.0
		var pattern := posmod(i, 32)
		var bend := road_x(distance_m)
		for side in [-1, 1]:
			var tree_x: float = bend + float(side) * (8.2 if side < 0 else 9.4)
			var tree_z := -distance_m - float(pattern % 3) * 1.1
			var base := Vector3(tree_x, 0.0, tree_z)
			var stem := MeshInstance3D.new()
			stem.mesh = trunk_mesh
			stem.material_override = trunk
			stem.position = base + Vector3(0, 1.75, 0)
			scene.add_child(stem)
			var crown := MeshInstance3D.new()
			crown.mesh = tree_mesh
			crown.material_override = leaves[posmod(pattern + side, leaves.size())]
			crown.scale = Vector3(2.1, 1.6, 1.8) if side > 0 else Vector3(1.45, 1.4, 1.45)
			crown.position = base + Vector3(0, 4.15, 0)
			scene.add_child(crown)
		if pattern % 4 == 1:
			continue
		var height := 5.5 + float(pattern % 5) * 1.6
		var width := 9.0 + float(pattern % 3) * 4.0
		var building_x := bend + 22.0 + float(pattern % 2) * 4.0
		box(scene, Vector3(building_x, height / 2.0, -distance_m - 4.0), Vector3(width, height, 13.0), walls[pattern % walls.size()])
		box(scene, Vector3(building_x, height + 0.12, -distance_m - 4.0), Vector3(width + 0.4, 0.24, 13.4), roofs[pattern % roofs.size()])
