extends "res://screen.gd"
## Sparse route inspection and explicit source-bound repair decisions.
signal review_changed(directory: String)
var review_directory := ""
var saving_repair := false
var repair_choice: OptionButton
var repair_details: TextEdit
var repair_buttons: Array[Button] = []
var bridge: RefCounted=ClassDB.instantiate("FullRoutePreview")
var busy := false
var summary := {}
var state := {}
var playing := false
var distance_m := 0.0
var clock := 0.0
var status: Label
var details: Label
var findings: TextEdit
var seek: HSlider
var play: Button
var road: MeshInstance3D
var viewport: SubViewport
var camera: Camera3D
var yaw := 0.0
var zoom := 350.0
var overview: Control
var queued_review: Array = []

func _ready() -> void:
	title="Full-route sparse preview"
	visibility_changed.connect(func():
		if not visible: playing=false; clock=0; play.text="Play at 30 km/h")
	var scroll := ScrollContainer.new(); scroll.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT); scroll.follow_focus=true; add_child(scroll)
	var box := VBoxContainer.new(); box.size_flags_horizontal=Control.SIZE_EXPAND_FILL; box.size_flags_vertical=Control.SIZE_EXPAND_FILL; scroll.add_child(box)
	status=Label.new(); status.autowrap_mode=TextServer.AUTOWRAP_WORD_SMART; box.add_child(status)
	var row := HBoxContainer.new(); box.add_child(row)
	play=Button.new(); play.text="Play at 30 km/h"; play.disabled=true; row.add_child(play); play.pressed.connect(func(): playing=not playing; play.text="Pause" if playing else "Play at 30 km/h")
	for offset in [-10000,10000]:
		var jump := Button.new(); jump.text="%+d km" % (offset/1000); row.add_child(jump); jump.pressed.connect(func(): show_distance(distance_m+offset))
	var rotate := Button.new(); rotate.text="Rotate view"; row.add_child(rotate); rotate.pressed.connect(func(): yaw+=PI/4; position_camera())
	for factor in [0.5,2.0]:
		var button := Button.new(); button.text="Zoom in" if factor<1 else "Zoom out"; row.add_child(button); button.pressed.connect(func(): zoom=clampf(zoom*factor,25,1000); position_camera())
	seek=HSlider.new(); seek.step=0.1; seek.editable=false; box.add_child(seek); seek.value_changed.connect(show_distance)
	details=Label.new(); details.autowrap_mode=TextServer.AUTOWRAP_WORD_SMART; box.add_child(details)
	overview=preload("res://plot.gd").new(); overview.profile=true; overview.custom_minimum_size.y=80; box.add_child(overview)
	overview.sample_picked.connect(func(index: int): show_distance(summary.overview[index].route_distance_m))
	var container := SubViewportContainer.new(); container.stretch=true; container.size_flags_vertical=Control.SIZE_EXPAND_FILL; container.custom_minimum_size.y=180; box.add_child(container)
	viewport=SubViewport.new(); viewport.own_world_3d=true; viewport.render_target_update_mode=SubViewport.UPDATE_ALWAYS; container.add_child(viewport)
	road=MeshInstance3D.new(); viewport.add_child(road)
	var material := StandardMaterial3D.new(); material.shading_mode=BaseMaterial3D.SHADING_MODE_UNSHADED; material.vertex_color_use_as_albedo=true; material.cull_mode=BaseMaterial3D.CULL_DISABLED; road.material_override=material
	camera=Camera3D.new(); camera.far=4000; viewport.add_child(camera); position_camera()
	findings=TextEdit.new(); findings.editable=false; findings.custom_minimum_size.y=60; box.add_child(findings)
	repair_choice=OptionButton.new(); box.add_child(repair_choice); repair_choice.item_selected.connect(show_repair)
	repair_details=TextEdit.new(); repair_details.editable=false; repair_details.custom_minimum_size.y=120; box.add_child(repair_details)
	var repairs_row := HBoxContainer.new(); box.add_child(repairs_row)
	for action in [["Accept repair","accepted"],["Reject repair","rejected"],["Undo decision","proposed"]]:
		var button := Button.new(); button.text=action[0]; repairs_row.add_child(button); repair_buttons.append(button)
		button.pressed.connect(func(): decide_repair(action[1]))
	var footer := Label.new(); footer.text="Sparse inspection only • ±500 m local view • orange marks a retained break • no scenery, trainer connection or activity recording"; footer.autowrap_mode=TextServer.AUTOWRAP_WORD_SMART; box.add_child(footer)

func load_review(directory: String, revision: String) -> void:
	if busy: queued_review=[directory,revision]; return
	review_directory=directory
	repair_choice.clear(); repair_details.text=""
	for button in repair_buttons: button.disabled=true
	playing=false; clock=0; summary={}; state={}; play.disabled=true; play.text="Play at 30 km/h"; seek.editable=false; road.mesh=null; findings.text=""; details.text=""; overview.configure([],0,0,0)
	busy=bridge.load_review(directory,revision); status.text="Preparing full-route profile from saved decisions…"

func _process(delta: float) -> void:
	if busy:
		var raw: String=bridge.poll()
		if raw.is_empty(): return
		busy=false
		if not queued_review.is_empty():
			var next: Array=queued_review; queued_review=[]; load_review(next[0],next[1]); return
		var response: Dictionary=JSON.parse_string(raw)
		if not response.ok:
			status.text=response.message; summary={}; playing=false; play.disabled=true; seek.editable=false; saving_repair=false
			return
		if saving_repair: saving_repair=false; review_changed.emit(review_directory)
		summary=response; seek.max_value=summary.length_m; seek.editable=true; play.disabled=false
		overview.configure(summary.overview,0,summary.overview.size()-1,0)
		status.text="%.2f km • %d source points • %d pauses bridged • %d breaks retained" % [summary.length_m/1000.0,summary.point_count,summary.bridged_pauses,summary.unresolved_breaks]
		findings.text="\n".join(summary.issues)
		if summary.simulation_error!=null: findings.text+="\nContinuous simulation unavailable: "+summary.simulation_error
		repair_choice.clear()
		for repair in summary.repairs: repair_choice.add_item("Sample %d: %s (%s)" % [repair.proposal.sample,repair.proposal.kind.replace("_"," "),repair.status])
		for button in repair_buttons: button.disabled=summary.repairs.is_empty()
		if not summary.repairs.is_empty(): show_repair(0)
		else: show_distance(0)
	if not visible: playing=false; play.text="Play at 30 km/h"; return
	if playing and not summary.is_empty():
		clock+=minf(delta,0.25)
		if clock<0.1: return
		var result: Dictionary=JSON.parse_string(bridge.advance(distance_m,clock*30.0/3.6)); clock=0
		show_distance(result.distance_m)
		if result.stopped: playing=false; play.text="Play at 30 km/h"; details.text+=" • Stopped at route end or unresolved break; seek explicitly to inspect beyond it."

func position_camera() -> void:
	camera.position=Vector3(sin(yaw)*zoom,zoom*0.8,cos(yaw)*zoom)
	camera.look_at(Vector3.ZERO)

func show_repair(index: int) -> void:
	if summary.is_empty() or index<0 or index>=summary.repairs.size(): return
	var repair: Dictionary=summary.repairs[index]
	repair_details.text="%s. Comparison includes other accepted repairs.\nRoute: %.3f → %.3f m; retained breaks: %d → %d\n" % [repair.status,repair.before_length_m,repair.after_length_m,repair.before_breaks,repair.after_breaks]
	for change in repair.elevation_changes:
		repair_details.text+="Elevation edit, sample %d: %.3f → %.3f m\n" % [change.sample,change.before_m,change.after_m]
	for phase in ["before","after"]:
		repair_details.text+=phase.capitalize()+" (stationary samples may consolidate):\n"
		for sample in repair[phase]:
			repair_details.text+="  Sample %d: height %.3f m; station %.3f m; grade %s%s\n" % [sample.sample,sample.elevation_m,sample.distance_m,"unavailable" if sample.grade_percent==null else "%.2f%%" % sample.grade_percent,"; break" if sample.break_before else ""]
	for sample in repair.before:
		if sample.sample==repair.proposal.sample: show_distance(sample.distance_m); break

func decide_repair(decision: String) -> void:
	if busy or summary.is_empty() or repair_choice.selected<0: return
	var repair: Dictionary=summary.repairs[repair_choice.selected]
	playing=false; play.text="Play at 30 km/h"; play.disabled=true; seek.editable=false
	busy=bridge.decide_repair(review_directory,summary.revision,repair.proposal.id,decision)
	saving_repair=busy
	for button in repair_buttons: button.disabled=true
	status.text="Saving repair decision…"

func show_distance(value: float) -> void:
	if summary.is_empty(): return
	var response: Dictionary=JSON.parse_string(bridge.window(value))
	if not response.ok: status.text=response.message; return
	state=response.window; distance_m=state.distance_m; seek.set_value_no_signal(distance_m)
	var nearest := 0
	for i in range(summary.overview.size()):
		if absf(summary.overview[i].route_distance_m-distance_m)<absf(summary.overview[nearest].route_distance_m-distance_m): nearest=i
	overview.cursor=nearest; overview.queue_redraw()
	details.text="%.3f / %.3f km • %.1f m elevation • grade %s • %.6f, %.6f%s" % [distance_m/1000.0,summary.length_m/1000.0,state.elevation_m,"unavailable" if state.grade_percent==null else "%.1f%%" % state.grade_percent,state.latitude,state.longitude," • source break" if state.at_break else ""]
	var mesh := ImmediateMesh.new(); mesh.surface_begin(Mesh.PRIMITIVE_TRIANGLES)
	# Center marker makes local-origin rebasing visible without carrying global floats into Godot.
	triangle(mesh,Vector3(-3,1,0),Vector3(3,1,0),Vector3(0,1,-8),Color.WHITE)
	for i in range(state.points.size()):
		var current := point(state.points[i].position)
		if state.points[i].break_before:
			triangle(mesh,current+Vector3(-5,1,0),current+Vector3(5,1,0),current+Vector3(0,1,-7),Color.ORANGE)
			continue
		if i==0: continue
		var previous := point(state.points[i-1].position)
		var tangent := Vector3(current.x-previous.x,0,current.z-previous.z)
		if tangent.length()<0.001: continue
		var side := tangent.normalized().cross(Vector3.UP)*2.5
		triangle(mesh,previous-side,previous+side,current+side,Color("56d6bd"))
		triangle(mesh,previous-side,current+side,current-side,Color("56d6bd"))
	mesh.surface_end(); road.mesh=mesh

func point(values: Array) -> Vector3:
	return Vector3(values[0],values[1],values[2])

func triangle(mesh: ImmediateMesh,a: Vector3,b: Vector3,c: Vector3,color: Color) -> void:
	mesh.surface_set_color(color); mesh.surface_add_vertex(a); mesh.surface_add_vertex(b); mesh.surface_add_vertex(c)
