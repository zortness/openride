extends "res://screen.gd"

var viewport: SubViewport
var content: Node
var training := false

func _ready() -> void:
	var container := SubViewportContainer.new()
	container.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	container.stretch = true
	add_child(container)
	viewport = SubViewport.new()
	viewport.own_world_3d = true
	viewport.handle_input_locally = true
	viewport.render_target_update_mode = SubViewport.UPDATE_WHEN_VISIBLE
	container.add_child(viewport)
	visibility_changed.connect(func():
		if content!=null: content.process_mode = Node.PROCESS_MODE_INHERIT if visible else Node.PROCESS_MODE_DISABLED)

func set_content(node: Node) -> void:
	content = node
	viewport.add_child(node)
	content.process_mode = Node.PROCESS_MODE_INHERIT if visible else Node.PROCESS_MODE_DISABLED

func active_ride() -> bool:
	return training and content != null and content.live_ride != null and content.live_ride.active()

func focus_first() -> void:
	if training and content.get("readiness")!=null and content.readiness.visible: content.connect_ready.grab_focus(); return
	for node in content.find_children("*", "BaseButton", true, false):
		if node.is_visible_in_tree() and not node.disabled:
			node.grab_focus(); return

func _process(delta: float) -> void:
	if not visible or content==null or not content.has_method("_pan_camera"): return
	var shell = get_tree().get_first_node_in_group("app_shell")
	if shell.has_dialog() or shell.ride_menu.visible: return
	var orbit := Vector2(Input.get_axis("or_orbit_left","or_orbit_right"),Input.get_axis("or_orbit_up","or_orbit_down"))
	apply_orbit(orbit,delta)
	var motion := Vector2(Input.get_axis("or_camera_left","or_camera_right"),Input.get_axis("or_camera_up","or_camera_down"))
	if motion.length()>0.25: content._pan_camera(motion*delta*350); content._update_camera()
	var zoom := Input.get_axis("or_zoom_out","or_zoom_in")
	if absf(zoom)>0.2: content.radius=clampf(content.radius*exp(-zoom*delta),2,20000); content._update_camera()

func apply_orbit(orbit: Vector2, delta: float) -> void:
	if orbit.length()<0.25: return
	orbit=orbit.limit_length()
	content.yaw -= orbit.x*delta*1.8
	content.pitch=clampf(content.pitch+orbit.y*delta*1.8,0.1,1.5)
	content._update_camera()

func _input(event: InputEvent) -> void:
	if not visible or content==null or not content.has_method("_focus_point"): return
	if get_tree().get_first_node_in_group("app_shell").has_dialog(): return
	if event.is_action_pressed("or_focus_point"): content._focus_point()
	elif event.is_action_pressed("or_toggle_tools"): content.inspector_button.button_pressed = not content.inspector_button.button_pressed
	elif event.is_action_pressed("or_delete_point"): content._remove_current_point()
	elif event.is_action_pressed("or_undo_point"): content.manual_repair.undo_removal()
	else: return
	get_viewport().set_input_as_handled()
