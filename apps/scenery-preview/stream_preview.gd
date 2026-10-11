extends Node3D
signal export_requested(journal:String)
var hud: PanelContainer
var road_metadata: Array = []
var credits: Label
var ground_material: ShaderMaterial
var asphalt_material: ShaderMaterial
var ride_seconds := 0.0
var training_entry := false
var hr_settings: VBoxContainer
var readiness: PanelContainer
var connect_ready: Button
var completion_replay: Button
var export_ride: Button

var traversal = preload("res://route_traversal.gd").new()
var riding_view := false
var live_mode := false
var live_ride: RefCounted
var finish_live: Button
var power_demo := false
var capture_replay := false
var replay_worker: RefCounted
var replay_path := ""
var replay_state := {}
var capture_picker: FileDialog
var capture_button: Button
var resume_movement: Button
var simulation: RefCounted
var simulated_speed := 0.0
var simulation_note := ""
var simulation_clock := 0
var title_label: Label
var motion_selector: OptionButton
var camera_route = preload("res://route_traversal.gd").new()
var coast_nodes: Array[Node3D] = []
var backdrop_material: ShaderMaterial
var view_yaw := 0.0
var root_path := ""
var route_sha256 := ""
var package_sha256 := ""
var chunk_index := {}
var live := {}
var peak_live := 0
var peak_bytes := 0
var loaded_count := 0
var unloaded_count := 0
var failed := false
var camera: Camera3D
var marker: MeshInstance3D
var status: Label
var slider: HSlider
var completion_panel: PanelContainer
var completion_label: Label
var route_completed := false
var play: Button
var speed: SpinBox
var yaw := 0.5
var pitch := 0.8
var radius := 160.0
var target := Vector3.ZERO
var automatic := false
var max_update_ms := 0.0
var wanted := {}
var prefetched := {}
var cache := {}
var worker: Thread
var worker_key := Vector2i.ZERO
var lookahead = preload("res://route_traversal.gd").new()
var peak_cached := 0
var discarded_count := 0
var max_commit_ms := 0.0
var prefetch_hits := 0
var cache_was_prefetch := {}
var process_cost_ms := 0.0
var saved_max_fps := 0
var saved_vsync := {}


func _ready() -> void:
	riding_view=riding_view or OS.get_cmdline_user_args().has("--riding-view")
	if riding_view:
		saved_max_fps=Engine.max_fps
		for id in [get_window().get_window_id(),get_tree().root.get_window_id()]:
			if not saved_vsync.has(id):saved_vsync[id]=DisplayServer.window_get_vsync_mode(id)
		set_frame_pacing(0)
	camera = Camera3D.new();camera.far=30000;camera.near=0.1;camera.fov=75;add_child(camera)
	var light := DirectionalLight3D.new();light.rotation_degrees=Vector3(-50,-35,0);add_child(light)
	var environment := WorldEnvironment.new();environment.environment=Environment.new()
	environment.environment.ambient_light_source=Environment.AMBIENT_SOURCE_COLOR
	environment.environment.ambient_light_color=Color("a1b7cd");environment.environment.ambient_light_energy=0.5;add_child(environment)
	if riding_view:
		var sky_material := ProceduralSkyMaterial.new()
		sky_material.sky_top_color=Color("668eaf")
		sky_material.sky_horizon_color=Color("c3d4dd")
		sky_material.ground_horizon_color=Color("c3d4dd")
		sky_material.ground_bottom_color=Color("718875")
		var sky := Sky.new();sky.sky_material=sky_material
		environment.environment.sky=sky
		environment.environment.background_mode=Environment.BG_SKY
		environment.environment.fog_enabled=true
		environment.environment.fog_light_color=Color("c3d4dd")
		environment.environment.fog_density=0.0005
	marker=MeshInstance3D.new();var ball:=SphereMesh.new();ball.radius=1.5;ball.height=3;marker.mesh=ball;marker.material_override=material(Color("ffc36a"));marker.material_override.no_depth_test=true;marker.material_override.render_priority=1;marker.material_override.shading_mode=BaseMaterial3D.SHADING_MODE_UNSHADED;add_child(marker)
	marker.visible=not riding_view
	var layer:=CanvasLayer.new();add_child(layer)
	var controls:=HBoxContainer.new();controls.set_anchors_and_offsets_preset(Control.PRESET_TOP_WIDE);controls.offset_left=12;controls.offset_top=12;layer.add_child(controls)
	var theme:=Theme.new();theme.default_font_size=14;controls.theme=theme
	play=Button.new();play.custom_minimum_size=Vector2(72,40);play.text="Play";play.pressed.connect(toggle_play)
	controls.add_child(play)
	speed=SpinBox.new();speed.min_value=1;speed.max_value=60;speed.value=25;speed.suffix="km/h";speed.custom_minimum_size.x=100;controls.add_child(speed)
	if riding_view:
		var pacing:=OptionButton.new()
		pacing.add_item("60 FPS cap");pacing.add_item("VSync")
		pacing.tooltip_text="60 FPS cap uses VSync off for steadier pacing on this PC. Select VSync if you see screen tearing."
		pacing.item_selected.connect(set_frame_pacing);controls.add_child(pacing)
	title_label=Label.new();title_label.text="Riding preview | Simulated speed | No trainer" if riding_view else "Road streaming preview | No trainer";controls.add_child(title_label)
	if riding_view and ClassDB.class_exists("RideSimulation"):
		motion_selector=OptionButton.new();motion_selector.add_item("Constant speed");motion_selector.add_item("Power demo");motion_selector.add_item("Capture replay");motion_selector.add_item("Live trainer")
		motion_selector.item_selected.connect(set_motion_mode);controls.add_child(motion_selector)
	finish_live=Button.new();finish_live.text="Finish ride";finish_live.hide();controls.add_child(finish_live)
	finish_live.pressed.connect(func():
		if live_ride!=null:live_ride.finish())
	capture_button=Button.new();capture_button.text="Open capture";capture_button.hide();controls.add_child(capture_button)
	resume_movement=Button.new();resume_movement.text="Resume movement";resume_movement.hide();controls.add_child(resume_movement)
	resume_movement.pressed.connect(func():
		if replay_worker!=null and not replay_worker.command("resume"):simulation_note="Command queue full; try again")
	capture_picker=FileDialog.new(); capture_picker.display_mode = FileDialog.DISPLAY_LIST;capture_picker.access=FileDialog.ACCESS_FILESYSTEM;capture_picker.file_mode=FileDialog.FILE_MODE_OPEN_FILE
	capture_picker.filters=PackedStringArray(["*.jsonl ; Trainer session capture"]);add_child(capture_picker)
	capture_button.pressed.connect(func():capture_picker.popup_centered_ratio(0.7))
	capture_picker.file_selected.connect(load_replay)
	var bottom:=VBoxContainer.new();bottom.theme=theme;bottom.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_WIDE);bottom.offset_left=12;bottom.offset_right=-12;bottom.offset_top=-92;bottom.offset_bottom=-12;layer.add_child(bottom)
	if riding_view:
		bottom.offset_top=-230
		hud=preload("res://ride_hud.gd").new();bottom.add_child(hud)
		var ui_scale:=OptionButton.new();ui_scale.add_item("Stats: compact");ui_scale.add_item("Stats: large");ui_scale.add_item("Stats: extra large")
		ui_scale.select(0 if hud.scale_setting<1 else 2 if hud.scale_setting>1 else 1)
		ui_scale.item_selected.connect(func(i:int):hud.apply_scale([0.75,1.0,1.25][i]));controls.add_child(ui_scale)
	status=Label.new();status.add_theme_color_override("font_shadow_color",Color.BLACK);status.add_theme_constant_override("shadow_offset_x",1);status.add_theme_constant_override("shadow_offset_y",1);status.autowrap_mode=TextServer.AUTOWRAP_WORD_SMART;bottom.add_child(status)
	slider=HSlider.new();slider.step=0.1;slider.value_changed.connect(seek);bottom.add_child(slider)
	credits=Label.new();credits.text="Terrain: USGS 3DEP | © OpenStreetMap contributors · ODbL 1.0";credits.add_theme_font_size_override("font_size",12);bottom.add_child(credits)
	var benchmark_seconds:=0.0
	completion_panel=PanelContainer.new()
	completion_panel.set_anchors_and_offsets_preset(Control.PRESET_CENTER_TOP)
	completion_panel.offset_left=-210;completion_panel.offset_right=210
	completion_panel.offset_top=64;completion_panel.offset_bottom=116
	var completion_style:=StyleBoxFlat.new();completion_style.bg_color=Color("20343eee")
	completion_style.content_margin_left=16;completion_style.content_margin_right=16
	completion_style.content_margin_top=10;completion_style.content_margin_bottom=10
	completion_panel.add_theme_stylebox_override("panel",completion_style)
	completion_panel.theme=theme;layer.add_child(completion_panel)
	var completion_row:=HBoxContainer.new();completion_row.add_theme_constant_override("separation",16);completion_panel.add_child(completion_row)
	completion_label=Label.new();completion_label.size_flags_horizontal=Control.SIZE_EXPAND_FILL;completion_row.add_child(completion_label)
	completion_replay=Button.new();completion_replay.text="Replay";completion_replay.custom_minimum_size=Vector2(76,36);completion_replay.pressed.connect(toggle_play);completion_row.add_child(completion_replay)
	export_ride=Button.new();export_ride.text="Export ride";export_ride.hide();completion_row.add_child(export_ride)
	export_ride.pressed.connect(func():
		if live_ride!=null and live_ride.completed:export_requested.emit(live_ride.directory.path_join("session.jsonl")))
	completion_panel.hide()
	var snapshot:=""
	var snapshot_fraction:=0.5
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--benchmark-seconds="):benchmark_seconds=clampf(float(arg.trim_prefix("--benchmark-seconds=")),1,300)
		if arg.begins_with("--stream-package="):root_path=arg.trim_prefix("--stream-package=")
		if arg.begins_with("--snapshot-fraction="):snapshot_fraction=clampf(float(arg.trim_prefix("--snapshot-fraction=")),0,1)
		if arg.begins_with("--look-degrees="):view_yaw=deg_to_rad(float(arg.trim_prefix("--look-degrees=")))
		if arg.begins_with("--snapshot="):snapshot=arg.trim_prefix("--snapshot=")
	automatic=OS.get_cmdline_user_args().has("--verify-stream") or not snapshot.is_empty() or benchmark_seconds>0
	if not load_package():return
	if riding_view:
		preload("res://route_detail.gd").markers(self,traversal.points,road_metadata)
	seek(0)
	simulation_clock=Time.get_ticks_msec()
	if training_entry:
		set_motion_mode(3);motion_selector.hide()
		readiness=PanelContainer.new(); readiness.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT); layer.add_child(readiness)
		var ready_style:=StyleBoxFlat.new(); ready_style.bg_color=Color("10212bee"); ready_style.content_margin_left=48; ready_style.content_margin_top=48; ready_style.content_margin_right=48; ready_style.content_margin_bottom=48; readiness.add_theme_stylebox_override("panel",ready_style)
		var ready_box:=VBoxContainer.new(); ready_box.add_theme_constant_override("separation",24); readiness.add_child(ready_box)
		var ready_title:=Label.new(); ready_title.text="Ready to ride"; ready_title.add_theme_font_size_override("font_size",32); ready_box.add_child(ready_title)
		var ready_label:=Label.new(); ready_label.text=live_ride.pairing_summary()+"\nRider + bike: 85 kg total\nTrainer resistance: 0?2% for this first live version\n\nConnect, then choose Start ride when ready."; ready_box.add_child(ready_label)
		hr_settings=load("res://hr_settings.gd").new(); ready_box.add_child(hr_settings)
		connect_ready=Button.new(); connect_ready.text="Connect trainer"; connect_ready.custom_minimum_size.y=52; ready_box.add_child(connect_ready)
		connect_ready.pressed.connect(func():
			var saved: Dictionary = JSON.parse_string(live_ride.bridge.pairing_selection())
			if saved.has("error"): ready_label.text = str(saved.error); return
			if saved.selection.get("heart_rate") != null and hr_settings.enabled.button_pressed:
				ready_label.text = "Bluetooth heart rate is selected. Turn off ANT heart rate to connect."; return
			live_ride.hr_config=hr_settings.selection(); readiness.hide(); toggle_play(); play.grab_focus())
		await get_tree().process_frame
		connect_ready.grab_focus()

	if OS.get_cmdline_user_args().has("--verify-ride-entry") and not Array(OS.get_cmdline_user_args()).any(func(a:String):return a.begins_with("--verify-live=")):
		if not training_entry or not live_mode or motion_selector.visible or live_ride.active() or not readiness.visible:
			fail("Ride entry did not reach disconnected readiness");get_tree().quit(1);return
		print("RIDE_ENTRY_OK explicit_connect=true no_hardware=true")
		if not snapshot.is_empty():
			await RenderingServer.frame_post_draw
			get_viewport().get_texture().get_image().save_png(snapshot)
		get_tree().quit();return
	if OS.get_cmdline_user_args().has("--verify-route-completion"):
		await verify_completion(snapshot)
		return
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--verify-live="):
			await verify_live(arg.trim_prefix("--verify-live="),snapshot)
			return
		if arg.begins_with("--verify-replay="):
			await verify_replay(arg.trim_prefix("--verify-replay="),snapshot)
			return
	if OS.get_cmdline_user_args().has("--verify-power-demo"):
		set_motion_mode(1)
		if failed:return
		simulation.command("resume")
		var state:Dictionary={}
		for tick in range(500):state=JSON.parse_string(simulation.advance(20,200.0,true))
		if state.get("distance_m",0)<=0 or state.get("speed_kph",0)<=0:
			fail("Power demo did not advance");get_tree().quit(1);return
		simulated_speed=state.speed_kph;seek(state.distance_m)
		simulation.command("pause")
		if not await wait_for_neighborhood():return
		var before_distance:float=traversal.distance_m
		toggle_play()
		for frame in range(60):await get_tree().process_frame
		toggle_play()
		if traversal.distance_m<=before_distance:
			fail("Power demo Play did not advance rendered route");get_tree().quit(1);return
		var held_distance:float=traversal.distance_m
		for frame in range(3):await get_tree().process_frame
		if traversal.distance_m!=held_distance:
			fail("Power demo Pause did not hold route");get_tree().quit(1);return
		print("POWER_DEMO_PLAY_PAUSE_OK distance_m=",traversal.distance_m)
		print("POWER_DEMO_OK distance_m=",state.distance_m," speed_kph=",state.speed_kph," grade=",state.grade_percent)
		if not snapshot.is_empty():
			await RenderingServer.frame_post_draw
			get_viewport().get_texture().get_image().save_png(snapshot)
		get_tree().quit();return
	if benchmark_seconds>0:
		await benchmark(benchmark_seconds)
		return
	if automatic:
		# Exercise changing targets while a read is in flight.
		pump_chunks()
		seek(traversal.length_m)
		seek(traversal.length_m/2)
		if not await wait_for_neighborhood(): return
		seek(0)
		if not await wait_for_neighborhood(): return
		for metre in range(0,int(traversal.length_m)+1,25):
			seek(metre)
			if not await wait_for_neighborhood(): return
			if riding_view and (absf(camera.position.y-target.y-1.65)>0.001 or not camera.transform.is_finite()):
				fail("Invalid riding camera pose");return
			for frame in range(4): await get_tree().process_frame
		seek(traversal.length_m)
		if not await wait_for_neighborhood(): return
		if peak_live>9 or peak_cached>3 or loaded_count<=peak_live or unloaded_count==0:
			fail("Streaming did not demonstrate bounded loading/unloading");return
		print("STREAM_TRAVERSAL_OK cells=",chunk_index.size()," peak_live=",peak_live," peak_payload_bytes=",peak_bytes," loads=",loaded_count," unloads=",unloaded_count," route_points=",traversal.points.size()," max_main_update_ms=",max_update_ms," peak_cached=",peak_cached," max_mesh_commit_ms=",max_commit_ms," prefetch_hits=",prefetch_hits," stale_results_discarded=",discarded_count)
		if not snapshot.is_empty():
			seek(traversal.length_m*snapshot_fraction)
			if not await wait_for_neighborhood(): return
			await RenderingServer.frame_post_draw
			if get_viewport().get_texture().get_image().save_png(snapshot)!=OK:fail("Screenshot failed");return
		get_tree().quit()

func fail(message: String) -> void:
	if live_ride!=null:live_ride.finish()
	failed=true;traversal.playing=false;play.disabled=true;status.text=message;push_error(message)
	if automatic: finish_failure.call_deferred()

func finish_failure() -> void:
	for node in live.values(): node.free()
	live.clear()
	for node in coast_nodes: node.free()
	coast_nodes.clear()
	cache.clear()
	# Let pending render-resource releases and awaiting verification finish before exit.
	await get_tree().process_frame
	await get_tree().process_frame
	get_tree().quit(1)

func checked_json(entry: Dictionary) -> Variant:
	var path:=root_path.path_join(entry.path)
	var file:=FileAccess.open(path,FileAccess.READ)
	if file==null or file.get_length()>2*1024*1024 or file.get_length()!=entry.get("bytes"):
		fail("Missing/oversized streaming payload: "+entry.path);return null
	if FileAccess.get_sha256(path)!=entry.get("sha256"):
		fail("Streaming checksum mismatch: "+entry.path);return null
	var parsed:Variant=JSON.parse_string(file.get_as_text())
	if not parsed is Dictionary:fail("Invalid streaming JSON: "+entry.path);return null
	return parsed

func load_package() -> bool:
	var file:=FileAccess.open(root_path.path_join("manifest.json"),FileAccess.READ)
	if file==null or file.get_length()>1024*1024:fail("Missing/oversized stream manifest");return false
	var m:Variant=JSON.parse_string(file.get_as_text())
	if not m is Dictionary or m.get("kind")!="openride.stream-preview" or m.get("schema_version")!=1 or m.get("trainer_ready")!=false or m.get("tile_m")!=256 or m.get("neighborhood_radius_cells")!=1 or m.get("max_live_chunks")!=9:
		fail("Unsupported stream manifest");return false
	if not m.get("chunks") is Array or m.chunks.size()>4096 or not m.get("route") is Dictionary or m.route.get("path")!="route.json" or not m.get("attribution") is Dictionary:
		fail("Invalid stream index");return false
	for entry in m.chunks:
		if not entry is Dictionary or not (entry.get("x") is float or entry.get("x") is int) or not (entry.get("z") is float or entry.get("z") is int):
			fail("Invalid chunk coordinates");return false
		if not is_finite(entry.x) or not is_finite(entry.z) or entry.x!=floor(entry.x) or entry.z!=floor(entry.z) or absf(entry.x)>10000 or absf(entry.z)>10000:
			fail("Invalid chunk grid");return false
		var key:=Vector2i(int(entry.x),int(entry.z))
		if entry.get("path")!="chunk_%d_%d.json" % [key.x,key.y] or chunk_index.has(key):
			fail("Unsafe or duplicate chunk path");return false
		var payload:=FileAccess.open(root_path.path_join(entry.path),FileAccess.READ)
		if payload==null or payload.get_length()!=entry.get("bytes") or payload.get_length()>2*1024*1024:
			fail("Incomplete stream package: "+entry.path);return false
		chunk_index[key]=entry
	route_sha256=m.route.sha256
	package_sha256=FileAccess.get_sha256(root_path.path_join("manifest.json"))
	var route:Variant=checked_json(m.route)
	if route==null:return false
	var error:String=traversal.configure(route)
	if not error.is_empty():fail(error);return false
	lookahead.configure(route)
	camera_route.configure(route)
	if riding_view and not m.get("presentation",{}).get("terrain_corridor",false):
		fail("Riding view requires a package built with --riding-surface");return false
	if m.has("coast"):
		if not m.coast is Dictionary or m.coast.get("path")!="coast.json":
			fail("Invalid coastal payload path");return false
		var coast:Variant=checked_json(m.coast)
		if coast==null or not load_coast(coast):return false
	ground_material=ShaderMaterial.new();ground_material.shader=preload("res://ground_detail.gdshader");ground_material.set_shader_parameter("base_color",Color.WHITE)
	asphalt_material=ShaderMaterial.new();asphalt_material.shader=preload("res://ground_detail.gdshader");asphalt_material.set_shader_parameter("base_color",Color("41454a"));asphalt_material.set_shader_parameter("road",true)
	if m.has("detail"):
		if m.detail.get("path")!="presentation.json":fail("Invalid road metadata path");return false
		var detail:Variant=checked_json(m.detail)
		if not detail is Dictionary or detail.get("schema")!=1 or not detail.get("road_points") is Array or detail.road_points.size()!=traversal.points.size():fail("Invalid road metadata");return false
		road_metadata=detail.road_points
		for i in range(road_metadata.size()):
			var q:Variant=road_metadata[i]
			if not q is Dictionary:fail("Invalid lane record");return false
			for field in ["width_m","lane_offset_m","distance_m"]:
				if not (q.get(field) is float or q.get(field) is int) or not is_finite(q[field]):fail("Invalid lane dimensions");return false
			if q.width_m<1 or q.width_m>30 or absf(q.lane_offset_m)>q.width_m/2 or absf(q.distance_m-traversal.points[i].distance_m)>.001:fail("Lane metadata outside route");return false
	if m.has("imagery"):
		var info:Dictionary=m.imagery
		if info.get("path")!="imagery.png" or not info.get("local_bounds") is Array or info.local_bounds.size()!=4:fail("Invalid imagery metadata");return false
		var ip:=root_path.path_join("imagery.png");var f:=FileAccess.open(ip,FileAccess.READ)
		if f==null or f.get_length()>16*1024*1024 or f.get_length()!=info.get("bytes") or FileAccess.get_sha256(ip)!=info.get("sha256"):fail("Imagery checksum/size mismatch");return false
		var picture:=Image.new()
		if picture.load_png_from_buffer(f.get_buffer(f.get_length()))!=OK or picture.get_width()>2048 or picture.get_height()>2048:fail("Invalid imagery image");return false
		var bounds:Array=info.local_bounds
		for v in bounds:
			if not (v is float or v is int) or not is_finite(v) or absf(v)>100000:fail("Invalid imagery bounds");return false
		if bounds[2]<=0 or bounds[3]<=0:fail("Invalid imagery extent");return false
		print("IMAGERY_LAYER_OK width=",picture.get_width()," height=",picture.get_height())
		credits.text+=" | USGS / NAIP imagery"
		picture.generate_mipmaps();ground_material.set_shader_parameter("imagery",ImageTexture.create_from_image(picture));ground_material.set_shader_parameter("use_imagery",true);ground_material.set_shader_parameter("imagery_bounds",Vector4(bounds[0],bounds[1],bounds[2],bounds[3]))
	slider.max_value=traversal.length_m
	return true

func load_coast(data: Dictionary) -> bool:
	if not data.get("terrain") is Dictionary or not data.get("water_polygon") is Array or data.water_polygon.size()<3 or data.water_polygon.size()>5010:
		fail("Invalid coastal backdrop");return false
	if not data.terrain.get("vertices") is Array or data.terrain.vertices.size()>5000:
		fail("Coastal terrain budget exceeded");return false
	var level:Variant=data.get("sea_level_y")
	if not (level is float or level is int) or not is_finite(level) or absf(level)>10000:
		fail("Invalid sea level");return false
	var polygon:=PackedVector2Array()
	var water_vertices:=[]
	for p in data.water_polygon:
		if not p is Array or p.size()!=2:
			fail("Invalid coastline point");return false
		for value in p:
			if not (value is float or value is int) or not is_finite(value) or absf(value)>100000:
				fail("Invalid coastline coordinate");return false
		polygon.append(Vector2(p[0],p[1]))
		water_vertices.append([p[0],level,p[1]])
	var indices:=Geometry2D.triangulate_polygon(polygon)
	if indices.size()!=(polygon.size()-2)*3:
		fail("Could not triangulate coastal water polygon");return false
	var water:=mesh_instance({"vertices":water_vertices,"indices":Array(indices)},Color("337e9b"))
	if failed:return false
	add_child(water);coast_nodes.append(water)
	water.material_override.roughness=0.3
	var terrain:=mesh_instance(data.terrain,Color("506b55"))
	if failed:return false
	if terrain!=null:
		backdrop_material=ShaderMaterial.new()
		backdrop_material.shader=preload("res://coastal_backdrop.gdshader")
		terrain.material_override=backdrop_material
		add_child(terrain);coast_nodes.append(terrain)
	print("COAST_BACKDROP_OK water_triangles=",indices.size()/3," terrain_vertices=",data.terrain.vertices.size())
	return true

func material(color: Color) -> StandardMaterial3D:
	var result:=StandardMaterial3D.new();result.albedo_color=color;result.cull_mode=BaseMaterial3D.CULL_DISABLED
	return result

func mesh_instance(geometry: Dictionary, color: Color, lines: bool=false) -> MeshInstance3D:
	var values:Variant=geometry.get("vertices")
	if not values is Array or values.size()>30000:fail("Chunk vertex budget exceeded");return null
	if values.is_empty():return null
	var vertices:=PackedVector3Array()
	for value in values:
		if not value is Array or value.size()!=3:fail("Invalid chunk vertex");return null
		for number in value:
			if not (number is float or number is int) or not is_finite(float(number)) or absf(number)>100000:fail("Invalid chunk coordinate");return null
		vertices.append(Vector3(value[0],value[1],value[2]))
	var arrays:=[];arrays.resize(Mesh.ARRAY_MAX);arrays[Mesh.ARRAY_VERTEX]=vertices
	if lines:
		if vertices.size()%2!=0:fail("Invalid line count");return null
	else:
		var values_indices:Variant=geometry.get("indices")
		if not values_indices is Array or values_indices.size()>90000 or values_indices.size()%3!=0:fail("Invalid chunk indices");return null
		var indices:=PackedInt32Array()
		for index in values_indices:
			if not (index is float or index is int) or not is_finite(float(index)) or index!=floor(index) or index<0 or index>=vertices.size():fail("Invalid triangle index");return null
			indices.append(int(index))
		arrays[Mesh.ARRAY_INDEX]=indices
	if not lines and geometry.has("colors") and not geometry.colors.is_empty():
		if geometry.colors.size()!=vertices.size():fail("Invalid terrain color count");return null
		var colors:=PackedColorArray()
		for c in geometry.colors:
			if not c is Array or c.size()!=3:fail("Invalid terrain color");return null
			for v in c:
				if not (v is float or v is int) or not is_finite(v) or v<0 or v>1:fail("Invalid terrain color value");return null
			colors.append(Color(c[0],c[1],c[2]))
		arrays[Mesh.ARRAY_COLOR]=colors
	var mesh:=ArrayMesh.new();mesh.add_surface_from_arrays(Mesh.PRIMITIVE_LINES if lines else Mesh.PRIMITIVE_TRIANGLES,arrays)
	if not lines:
		var surface:=SurfaceTool.new();surface.create_from(mesh,0);surface.generate_normals();mesh=surface.commit()
	var instance:=MeshInstance3D.new();instance.mesh=mesh;instance.material_override=material(color)
	if lines or (not riding_view and color==Color("78aaff")):
		instance.material_override.shading_mode=BaseMaterial3D.SHADING_MODE_UNSHADED
		instance.material_override.no_depth_test=true
	return instance

func neighborhood(position: Vector3) -> Dictionary:
	var cell := Vector2i(floori(position.x/256),floori(position.z/256))
	var result := {}
	for x in range(cell.x-1,cell.x+2):
		for z in range(cell.y-1,cell.y+2):
			var key := Vector2i(x,z)
			if chunk_index.has(key): result[key]=true
	return result

func update_chunks(position: Vector3) -> void:
	var started := Time.get_ticks_usec()
	wanted = neighborhood(position)
	var ahead: Dictionary = lookahead.seek(traversal.distance_m+100.0)
	prefetched = neighborhood(ahead.position)
	for key in live.keys():
		if not wanted.has(key):
			live[key].free()
			live.erase(key)
			unloaded_count+=1
	for key in cache.keys():
		if not wanted.has(key) and not prefetched.has(key):
			cache.erase(key)
			cache_was_prefetch.erase(key)
	max_update_ms=maxf(max_update_ms,(Time.get_ticks_usec()-started)/1000.0)

func neighborhood_ready() -> bool:
	for key in wanted:
		if not live.has(key): return false
	return not wanted.is_empty()

func pump_chunks() -> void:
	if failed: return
	if worker != null and not worker.is_alive():
		var result: Dictionary = worker.wait_to_finish()
		worker = null
		if not wanted.has(worker_key) and not prefetched.has(worker_key):
			discarded_count+=1
		elif result.has("error"):
			fail(result.error)
			return
		else:
			cache[worker_key] = result.geometry
			cache_was_prefetch[worker_key] = not wanted.has(worker_key)
			peak_cached=maxi(peak_cached,cache.size())
	# At most one chunk's meshes are constructed/uploaded in a frame.
	for key in wanted:
		if live.has(key) or not cache.has(key): continue
		var started := Time.get_ticks_usec()
		var geometry: Dictionary = cache[key]
		cache.erase(key)
		if cache_was_prefetch.get(key,false): prefetch_hits+=1
		cache_was_prefetch.erase(key)
		var node := Node3D.new()
		add_child(node)
		live[key]=node
		for kind in ["terrain","road","lines","buildings","roofs","context_roads","unpaved","markings","center_markings"]:
			if not geometry.has(kind): continue
			if not geometry[kind] is Dictionary and kind!="lines":
				fail("Invalid scenery geometry");return
			if riding_view and kind=="lines": continue
			var g: Dictionary = {"vertices":geometry.lines} if kind=="lines" else geometry[kind]
			var instance := mesh_instance(g,Color("506b55") if kind=="terrain" else Color("78aaff") if kind=="road" else Color("dca959"),kind=="lines")
			if failed: return
			if riding_view and kind=="road" and instance!=null:
				instance.material_override.albedo_color=Color("44494c")
				instance.material_override.roughness=0.95
			if instance!=null:
				if kind=="buildings":instance.material_override.albedo_color=Color("cbbda4")
				if kind=="roofs":instance.material_override.albedo_color=Color("806f63")
				if kind in ["road","context_roads"]:instance.material_override=asphalt_material
				if kind=="terrain" and not road_metadata.is_empty():instance.material_override=ground_material
				if kind=="unpaved":instance.material_override.albedo_color=Color("9d8a63")
				if kind=="markings":instance.material_override.albedo_color=Color("eee9ce")
				if kind=="center_markings":instance.material_override.albedo_color=Color("e3b948")
				node.add_child(instance)
		if riding_view and geometry.has("vegetation"):
			if not geometry.vegetation is Array or not preload("res://route_detail.gd").vegetation(node,geometry.vegetation):fail("Invalid vegetation payload");return
		loaded_count+=1
		max_commit_ms=maxf(max_commit_ms,(Time.get_ticks_usec()-started)/1000.0)
		break
	var payload_bytes := 0
	for key in live: payload_bytes+=int(chunk_index[key].bytes)
	peak_live=maxi(peak_live,live.size())
	peak_bytes=maxi(peak_bytes,payload_bytes)
	if worker != null: return
	# Required cells take priority over the bounded forward prefetch cache.
	var next_keys: Array = []
	for key in wanted:
		if not live.has(key) and not cache.has(key): next_keys.append(key)
	if next_keys.is_empty():
		for key in prefetched:
			if not live.has(key) and not cache.has(key): next_keys.append(key)
	if next_keys.is_empty(): return
	# Leave room for the single in-flight result. Evict speculative data first.
	if cache.size()>=3:
		if not wanted.has(next_keys[0]): return
		for key in cache.keys():
			if not wanted.has(key):
				cache.erase(key); cache_was_prefetch.erase(key); break
	if cache.size()>=3: return
	worker_key=next_keys[0]
	worker=Thread.new()
	var error := worker.start(preload("res://chunk_worker.gd").read_chunk.bind(root_path,chunk_index[worker_key].duplicate(true)))
	if error!=OK:
		worker=null
		fail("Could not start chunk reader")

func wait_for_neighborhood() -> bool:
	var deadline := Time.get_ticks_msec()+10000
	while not neighborhood_ready() and not failed:
		if Time.get_ticks_msec()>deadline:
			fail("Chunk loading timed out")
			return false
		await get_tree().process_frame
	return not failed

func set_motion_mode(index:int) -> void:
	if live_ride!=null and live_ride.active():
		motion_selector.select(3);return
	live_ride=null
	live_mode=index==3
	finish_live.visible=live_mode
	if motion_selector!=null:motion_selector.select(index)
	traversal.playing=false;play.text="Play";play.disabled=false
	simulation_note=""
	route_completed=false;completion_panel.hide();export_ride.hide();completion_replay.text="Replay"
	replay_worker=null;replay_state={};resume_movement.hide()
	power_demo=index==1;capture_replay=index==2
	capture_button.visible=capture_replay;speed.visible=not (capture_replay or live_mode)
	slider.editable=not (power_demo or capture_replay or live_mode)
	if power_demo:
		simulation=ClassDB.instantiate("RideSimulation")
		var error:String=simulation.configure(JSON.stringify({"points":traversal.points}))
		if not error.is_empty():fail(error);return
		speed.min_value=0;speed.max_value=600;speed.step=10;speed.suffix="W";speed.value=200
		title_label.text="Power demo | 85 kg total | No trainer"
		simulated_speed=0;seek(0)
	else:
		simulation=null
		speed.min_value=1;speed.max_value=60;speed.step=1;speed.suffix="km/h";speed.value=25
		title_label.text="Riding preview | Simulated speed | No trainer"
	if capture_replay:
		title_label.text="Capture replay | 85 kg total | No trainer"
		simulation_note="Select a session.jsonl capture"
		simulated_speed=0;seek(0)
	if live_mode:
		live_ride=preload("res://live_ride.gd").new()
		title_label.text="Live trainer | 85 kg | Trainer 0-2%"
		simulated_speed=0;seek(0);play.text="Connect trainer"
	simulation_clock=Time.get_ticks_msec()
	refresh_status()

func load_replay(path:String) -> void:
	replay_worker=null;replay_state={};traversal.playing=false;completion_panel.hide();resume_movement.hide()
	var candidate:RefCounted=ClassDB.instantiate("RideReplay")
	var error:String=candidate.configure(JSON.stringify({"points":traversal.points}),path)
	if not error.is_empty():
		simulation_note="Capture error: "+error;refresh_status();return
	replay_worker=candidate;replay_path=path;replay_state=JSON.parse_string(replay_worker.snapshot())
	simulated_speed=0;seek(0);simulation_note="";play.text="Play";refresh_status()

func toggle_play() -> void:
	if failed:return
	if live_mode:
		if not live_ride.active():
			seek(0);live_ride.connect_trainer(root_path.path_join("route.json"),route_sha256,package_sha256)
		else:live_ride.toggle()
		return
	if capture_replay:
		if replay_worker==null:return
		if replay_state.get("status","") in ["replay_complete","route_complete"]:load_replay(replay_path)
		if replay_worker==null:return
		var action:="pause" if replay_state.get("playing",false) else "resume"
		if not replay_worker.command(action):simulation_note="Command queue full; try again"
		return
	if traversal.distance_m>=traversal.length_m:
		seek(0)
		if power_demo:
			simulation.command("reset");simulated_speed=0
	traversal.playing=not traversal.playing
	if power_demo:
		simulation.command("resume" if traversal.playing else "pause")
		simulation_note=""
	simulation_clock=Time.get_ticks_msec()
	play.text="Pause" if traversal.playing else "Play"

func set_frame_pacing(index:int) -> void:
	Engine.max_fps=60 if index==0 else 0
	for id in saved_vsync:
		DisplayServer.window_set_vsync_mode(DisplayServer.VSYNC_DISABLED if index==0 else DisplayServer.VSYNC_ENABLED,id)

func _exit_tree() -> void:
	if live_ride!=null:live_ride.finish();live_ride=null
	replay_worker=null
	if not saved_vsync.is_empty():
		Engine.max_fps=saved_max_fps
		for id in saved_vsync:DisplayServer.window_set_vsync_mode(saved_vsync[id],id)
	if worker!=null:
		worker.wait_to_finish()
		worker=null

func seek(value: float) -> void:
	if failed:return
	var sample:Dictionary=traversal.seek(value)
	if sample.is_empty():return
	target=sample.position;marker.position=target+Vector3.UP*2
	update_chunks(target)
	if failed:return
	if sample.distance_m<traversal.length_m:
		route_completed=false;completion_panel.hide()
	slider.set_value_no_signal(sample.distance_m)
	refresh_status()
	update_camera()

func update_camera() -> void:
	if riding_view:
		var behind: Dictionary=camera_route.seek(traversal.distance_m-3.0)
		var ahead: Dictionary=camera_route.seek(traversal.distance_m+12.0)
		var direction: Vector3=(ahead.position-behind.position).normalized()
		var offset:=0.0
		if not road_metadata.is_empty():
			var low:=1
			while low<road_metadata.size()-1 and road_metadata[low].distance_m<traversal.distance_m:low+=1
			var a:Dictionary=road_metadata[low-1];var b:Dictionary=road_metadata[low]
			offset=lerpf(a.lane_offset_m,b.lane_offset_m,clampf((traversal.distance_m-a.distance_m)/(b.distance_m-a.distance_m),0,1))
		var right:=Vector3(-direction.z,0,direction.x).normalized()
		camera.position=target+Vector3.UP*1.65+right*offset
		camera.look_at(camera.position+direction.rotated(Vector3.UP,-view_yaw)*20.0)
		return
	camera.position=target+Vector3(sin(yaw)*cos(pitch),sin(pitch),cos(yaw)*cos(pitch))*radius;camera.look_at(target)

func refresh_status() -> void:
	var sample: Dictionary = traversal.seek(traversal.distance_m)
	if sample.is_empty(): return
	if riding_view:
		var hud_power:Variant=null;var cadence:Variant=null;var hr:Variant=null;var seconds:=ride_seconds
		if live_mode and live_ride!=null:
			hud_power=live_ride.state.get("power_w")
			cadence=live_ride.cadence if Time.get_ticks_msec()<live_ride.cadence_deadline else null
			hr=live_ride.hr_bpm if Time.get_ticks_msec()<live_ride.hr_deadline else null
			seconds=maxi(0,live_ride.elapsed_ms-live_ride.ride_start_ms)/1000.0 if live_ride.ride_start_ms>=0 else 0
		elif capture_replay:hud_power=replay_state.get("power_w");seconds=replay_state.get("replay_ms",0)/1000.0
		elif power_demo:hud_power=speed.value
		hud.display(hud_power,cadence,hr,simulated_speed if live_mode or power_demo or capture_replay else (speed.value if traversal.playing else 0.0),seconds,traversal.length_m-sample.distance_m,sample.grade_percent)
		if live_mode and live_ride!=null:
			status.text=live_ride.note
			if live_ride.hr_status!="off" and hr==null:status.text+=" ? HR "+("unavailable" if live_ride.hr_status=="unavailable" else "searching")
		elif capture_replay:status.text="Capture replay ? "+str(replay_state.get("status","")).replace("_"," ")+" "+simulation_note
		elif power_demo:status.text="Power demo ? "+simulation_note
		else:status.text="Route preview" if neighborhood_ready() else "Loading nearby scenery..."
		return
	status.text="%.0f / %.0f m | %+.2f%% grade | %d / 9 chunks | %s" % [sample.distance_m,traversal.length_m,sample.grade_percent,live.size(),"Drag to orbit / wheel to zoom" if neighborhood_ready() else "Loading nearby terrain..."]

func _process(delta: float) -> void:
	var process_started:=Time.get_ticks_usec()
	if traversal.playing:ride_seconds+=delta
	pump_chunks()
	if backdrop_material!=null:
		var cell:=Vector2(floor(target.x/256),floor(target.z/256))*256
		backdrop_material.set_shader_parameter("near_min",cell-Vector2(224,224))
		backdrop_material.set_shader_parameter("near_max",cell+Vector2(480,480))
		backdrop_material.set_shader_parameter("near_ready",neighborhood_ready())
	if failed: return
	var now:=Time.get_ticks_msec()
	var elapsed:=now-simulation_clock
	simulation_clock=now
	if live_mode and live_ride!=null:
		live_ride.poll()
		motion_selector.disabled=live_ride.active()
		finish_live.disabled=not live_ride.active() or live_ride.pending=="finish"
		play.disabled=live_ride.active() and (live_ride.faulted or not live_ride.pending.is_empty() or not (live_ride.phase=="running" or (live_ride.phase=="paused" and live_ride.can_resume)))
		play.text="Connect trainer" if not live_ride.active() else ("Pause" if live_ride.phase=="running" else ("Resume" if live_ride.started else "Start ride"))
		if not live_ride.state.is_empty():
			simulated_speed=live_ride.state.speed_kph;seek(live_ride.state.distance_m)
		if live_ride.route_complete:finish_route()
		if live_ride.completed and not live_ride.active() and not live_ride.faulted:
			completion_label.text=("Route complete | %.2f km" if live_ride.route_complete else "Ride saved | %.2f km") % (traversal.distance_m/1000.0)
			completion_replay.text="New ride";export_ride.show();completion_panel.show()
		refresh_status()
	elif capture_replay:
		if replay_worker!=null:
			replay_state=JSON.parse_string(replay_worker.snapshot())
			simulated_speed=replay_state.speed_kph;seek(replay_state.distance_m)
			play.text="Pause" if replay_state.playing else "Resume"
			resume_movement.visible=replay_state.status in ["power_stale","power_missing"] and replay_state.power_w!=null
			if replay_state.status=="route_complete":finish_route()
			elif replay_state.status=="replay_complete":
				play.text="Replay";completion_label.text="Capture complete | %.0f m travelled" % traversal.distance_m;completion_panel.show()
			elif replay_state.status in ["power_stale","power_missing"]:
				simulation_note="Movement paused; resume when power returns"
			elif replay_state.status=="running":simulation_note=""
		refresh_status()
	elif power_demo and traversal.playing and neighborhood_ready():
		var state:Dictionary=JSON.parse_string(simulation.advance(elapsed,speed.value,true))
		if state.has("error"):fail(state.error);return
		simulated_speed=state.speed_kph;seek(state.distance_m)
		if state.status!="running":
			traversal.playing=false;play.text="Replay" if state.status=="finished" else "Resume"
			if state.status=="finished":finish_route()
			if state.status=="timing_gap":simulation_note=" | Paused after timing gap"
	elif traversal.playing and neighborhood_ready():
		traversal.advance(delta,speed.value);seek(traversal.distance_m)
		if not traversal.playing:finish_route()
	elif not traversal.points.is_empty():
		refresh_status()

	process_cost_ms=(Time.get_ticks_usec()-process_started)/1000.0

func _unhandled_input(event: InputEvent) -> void:
	if riding_view:
		if event is InputEventMouseMotion and event.button_mask & MOUSE_BUTTON_MASK_RIGHT:
			view_yaw=clampf(view_yaw+event.relative.x*0.004,-1.5,1.5);update_camera()
		if event is InputEventKey and event.pressed and event.keycode==KEY_F:
			view_yaw=0;update_camera()
		return
	if event is InputEventMouseMotion and event.button_mask & MOUSE_BUTTON_MASK_LEFT:
		yaw-=event.relative.x*0.006;pitch=clampf(pitch+event.relative.y*0.006,0.15,1.5);update_camera()
	if event is InputEventMouseButton and event.pressed:
		if event.button_index==MOUSE_BUTTON_WHEEL_UP:radius=maxf(25,radius*0.85)
		elif event.button_index==MOUSE_BUTTON_WHEEL_DOWN:radius=minf(250,radius/0.85)
		update_camera()

func benchmark(seconds:float) -> void:
	if OS.get_cmdline_user_args().has("--benchmark-vsync-off"):
		DisplayServer.window_set_vsync_mode(DisplayServer.VSYNC_DISABLED,get_window().get_window_id())
		DisplayServer.window_set_vsync_mode(DisplayServer.VSYNC_DISABLED,get_tree().root.get_window_id())
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--benchmark-speed="):speed.value=clampf(float(arg.trim_prefix("--benchmark-speed=")),1,60)
		if arg.begins_with("--benchmark-frame-cap="):Engine.max_fps=clampi(int(arg.trim_prefix("--benchmark-frame-cap=")),0,240)
	if not await wait_for_neighborhood():return
	traversal.playing=true
	var warmup:=Time.get_ticks_msec()+2000
	while Time.get_ticks_msec()<warmup:await get_tree().process_frame
	var frames:Array[float]=[]
	var costs:Array[float]=[]
	var engine_costs:Array[float]=[]
	var drawn_at_start:=Engine.get_frames_drawn()
	var began:=Time.get_ticks_usec()
	var previous:=began
	var stalled:=0
	var gpu_bytes:=0.0
	while Time.get_ticks_usec()-began<seconds*1000000 and not failed:
		await get_tree().process_frame
		var now:=Time.get_ticks_usec()
		frames.append((now-previous)/1000.0);previous=now
		costs.append(process_cost_ms)
		engine_costs.append(Performance.get_monitor(Performance.TIME_PROCESS)*1000.0)
		if not neighborhood_ready():stalled+=1
		gpu_bytes=maxf(gpu_bytes,Performance.get_monitor(Performance.RENDER_VIDEO_MEM_USED))
	if failed:return
	frames.sort();costs.sort();engine_costs.sort()
	var report:={"seconds":(previous-began)/1000000.0,"frames":frames.size(),"p50_ms":frames[int((frames.size()-1)*0.50)],"p95_ms":frames[int((frames.size()-1)*0.95)],"p99_ms":frames[int((frames.size()-1)*0.99)],"max_ms":frames[-1],"loading_frames":stalled,"peak_live_chunks":peak_live,"peak_cached":peak_cached,"engine_video_memory_bytes":gpu_bytes,"viewport":str(get_viewport().get_visible_rect().size),"gpu":RenderingServer.get_video_adapter_name(),"distance_m":traversal.distance_m}
	report["script_p95_ms"]=costs[int((costs.size()-1)*0.95)]
	report["engine_process_p95_ms"]=engine_costs[int((engine_costs.size()-1)*0.95)]
	report["vsync"]=DisplayServer.window_get_vsync_mode(get_window().get_window_id())
	report["embedded_window"]=get_window().is_embedded()
	report["engine_max_fps"]=Engine.max_fps
	report["drawn_frames"]=Engine.get_frames_drawn()-drawn_at_start
	report["draw_calls"]=Performance.get_monitor(Performance.RENDER_TOTAL_DRAW_CALLS_IN_FRAME)
	report["rendered_primitives"]=Performance.get_monitor(Performance.RENDER_TOTAL_PRIMITIVES_IN_FRAME)
	report["window_mode"]=DisplayServer.window_get_mode(get_tree().root.get_window_id())
	print("SCENERY_BENCHMARK ",JSON.stringify(report))
	get_tree().quit()

func finish_route() -> void:
	traversal.playing=false
	route_completed=true
	play.text="Replay"
	completion_label.text="Route complete  |  %.2f km" % (traversal.length_m/1000.0)
	completion_panel.show()
	refresh_status()

func verify_completion(snapshot:String) -> void:
	seek(traversal.length_m-0.01)
	if not await wait_for_neighborhood():return
	toggle_play()
	for frame in range(3):await get_tree().process_frame
	if not route_completed or not completion_panel.visible or traversal.playing:
		fail("Constant-speed completion was not announced");get_tree().quit(1);return
	toggle_play()
	if route_completed or completion_panel.visible or traversal.distance_m!=0:
		fail("Replay did not clear completion");get_tree().quit(1);return
	toggle_play()
	set_motion_mode(1)
	simulation.command("resume")
	var state:Dictionary={}
	for tick in range(100000):
		state=JSON.parse_string(simulation.advance(20,600.0,true))
		if state.get("status")=="finished":break
	if state.get("status")!="finished":
		fail("Power completion fixture did not finish");get_tree().quit(1);return
	seek(state.distance_m)
	if not await wait_for_neighborhood():return
	traversal.playing=true
	for frame in range(3):await get_tree().process_frame
	if not route_completed or not completion_panel.visible or traversal.playing:
		fail("Power completion was not announced");get_tree().quit(1);return
	print("ROUTE_COMPLETION_OK constant_speed=true replay=true power_demo=true")
	if not snapshot.is_empty():
		await RenderingServer.frame_post_draw
		get_viewport().get_texture().get_image().save_png(snapshot)
	get_tree().quit()


func verify_replay(path:String,snapshot:String) -> void:
	set_motion_mode(2);load_replay(path)
	if replay_worker==null:
		fail(simulation_note);get_tree().quit(1);return
	toggle_play()
	# Deliberately block the renderer: replay's worker must continue independently.
	OS.delay_msec(400)
	var state:Dictionary=JSON.parse_string(replay_worker.snapshot())
	if state.replay_ms<250:
		fail("Replay did not progress while rendering was blocked");get_tree().quit(1);return
	replay_worker.command("pause");OS.delay_msec(40)
	var held:Dictionary=JSON.parse_string(replay_worker.snapshot())
	OS.delay_msec(80)
	var paused:Dictionary=JSON.parse_string(replay_worker.snapshot())
	if held.distance_m!=paused.distance_m or held.replay_ms!=paused.replay_ms:
		fail("Replay pause did not hold");get_tree().quit(1);return
	if not await wait_for_neighborhood():return
	for frame in range(3):await get_tree().process_frame
	print("REPLAY_WORKER_OK renderer_block_ms=400 replay_ms=",state.replay_ms," distance_m=",state.distance_m," duration_ms=",state.duration_ms," source_sha256=",state.source_sha256)
	toggle_play()
	var deadline:int=Time.get_ticks_msec()+int(state.duration_ms)+3000
	while replay_state.get("status","")!="replay_complete" and Time.get_ticks_msec()<deadline:
		await get_tree().process_frame
	if replay_state.get("status","")!="replay_complete" or route_completed or not completion_panel.visible or not completion_label.text.begins_with("Capture complete"):
		fail("Capture end feedback failed");get_tree().quit(1);return
	print("REPLAY_CAPTURE_END_OK distance_m=",traversal.distance_m," elapsed_ms=",replay_state.replay_ms)
	if not snapshot.is_empty():
		await RenderingServer.frame_post_draw
		get_viewport().get_texture().get_image().save_png(snapshot)
	toggle_play();OS.delay_msec(60)
	var restarted:Dictionary=JSON.parse_string(replay_worker.snapshot())
	if restarted.replay_ms>200 or completion_panel.visible:
		fail("Capture restart failed");get_tree().quit(1);return
	set_motion_mode(0)
	if replay_worker!=null:
		fail("Replay worker not released");get_tree().quit(1);return
	get_tree().quit()


func verify_live(fixture:String,snapshot:String) -> void:
	if not training_entry:set_motion_mode(3)
	var fixture_device := {"id":"synthetic-two", "name":"Synthetic trainer"}
	var pairing_error: String = live_ride.bridge.save_pairing(JSON.stringify({"schema":1,"resistance":fixture_device,"power":fixture_device,"cadence":fixture_device}))
	if not pairing_error.is_empty(): fail(pairing_error); get_tree().quit(1); return
	live_ride.worker_override=fixture
	if readiness!=null:
		hr_settings.enabled.button_pressed=true;hr_settings.number.value=42;hr_settings.transmission.value=241
		connect_ready.pressed.emit()
	else:live_ride.hr_config='{"device_number":42,"transmission_type":241,"key_file":"fixture"}';toggle_play()
	var deadline:=Time.get_ticks_msec()+10000
	while live_ride.phase!="paused" and Time.get_ticks_msec()<deadline:await get_tree().process_frame
	if live_ride.phase!="paused" or not live_ride.can_resume:
		fail("Live fixture did not connect paused");get_tree().quit(1);return
	var competitor:RefCounted=ClassDB.instantiate("TrainerSession")
	if competitor.start_session(fixture,live_ride.directory+"-duplicate"):
		fail("Second controller owner allowed");get_tree().quit(1);return
	toggle_play()
	while live_ride.phase!="running" and Time.get_ticks_msec()<deadline:await get_tree().process_frame
	if live_ride.phase!="running" or traversal.distance_m!=10 or hud.values[0].text!="200" or hud.values[1].text!="82" or hud.values[2].text!="88":
		fail("Live state did not reach riding view");get_tree().quit(1);return
	toggle_play()
	while live_ride.phase!="paused" and Time.get_ticks_msec()<deadline:await get_tree().process_frame
	if live_ride.phase!="paused" or live_ride.epoch!=2 or not live_ride.heart_rate_text().contains("unavailable"):
		fail("Live pause acknowledgement missing");get_tree().quit(1);return
	toggle_play()
	while live_ride.phase!="running" and Time.get_ticks_msec()<deadline:await get_tree().process_frame
	await get_tree().create_timer(3.1).timeout
	if not live_ride.heart_rate_text().contains("HR --"):fail("Stale HR retained");get_tree().quit(1);return
	if live_ride.phase!="running":fail("Live resume failed");get_tree().quit(1);return
	if not await wait_for_neighborhood():return
	if not snapshot.is_empty():
		await RenderingServer.frame_post_draw
		get_viewport().get_texture().get_image().save_png(snapshot)
	live_ride.finish()
	while live_ride.active() and Time.get_ticks_msec()<deadline:await get_tree().process_frame
	await get_tree().process_frame
	if not live_ride.completed or live_ride.faulted:
		fail("Live finish failed");get_tree().quit(1);return
	if not export_ride.visible or not completion_label.text.begins_with("Ride saved"):
		fail("Finish summary/export unavailable");get_tree().quit(1);return
	var emitted:Array=[]
	export_requested.connect(func(path:String):emitted.append(path))
	export_ride.pressed.emit()
	if emitted!=[live_ride.directory.path_join("session.jsonl")]:
		fail("Export action lost session path");get_tree().quit(1);return
	var history:=FileAccess.get_file_as_string(live_ride.directory.path_join("fixture-control.txt"))
	if history!="resume 1\npause\nresume 2\nfinish\n":
		fail("Live control history mismatch");get_tree().quit(1);return
	print("LIVE_RIDING_FIXTURE_OK start_paused=true exclusive_owner=true resume_pause_finish=true directory=",live_ride.directory)
	set_motion_mode(0);get_tree().quit()
