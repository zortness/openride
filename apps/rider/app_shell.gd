extends Control

const WorldScreen = preload("res://world_screen.gd")

var app: Control
var body: Control
var header: HBoxContainer
var tabs: HBoxContainer
var hints: Label
var pages := {}
var stack: Array[Control] = []
var focused := {}
var switching := false
var history: Control
var ride_menu: PanelContainer
var menu_status: Label
var pause_button: Button
var finish_button: Button
var leave_after_finish := false
var quitting := false
var gamepad := false
var axis := Vector2.ZERO
var next_repeat := 0
var active_world: Control
var tab_buttons: Array[Button] = []
var ride_shade: ColorRect
var current_section := "Home"
var home_back_armed := false
var ride_inputs := preload("res://ride_inputs.gd").new()

func configure(owner_app: Control, inspector: Control) -> void:
	app = owner_app
	get_tree().node_added.connect(func(node: Node):
		if node is FileDialog: prepare_file_dialog.call_deferred(node))
	for node in get_tree().root.find_children("*","FileDialog",true,false): prepare_file_dialog(node)
	for action in {"ui_accept":JOY_BUTTON_A,"ui_cancel":JOY_BUTTON_B,"ui_left":JOY_BUTTON_DPAD_LEFT,"ui_right":JOY_BUTTON_DPAD_RIGHT,"ui_up":JOY_BUTTON_DPAD_UP,"ui_down":JOY_BUTTON_DPAD_DOWN}:
		var binding := InputEventJoypadButton.new(); binding.button_index = {"ui_accept":JOY_BUTTON_A,"ui_cancel":JOY_BUTTON_B,"ui_left":JOY_BUTTON_DPAD_LEFT,"ui_right":JOY_BUTTON_DPAD_RIGHT,"ui_up":JOY_BUTTON_DPAD_UP,"ui_down":JOY_BUTTON_DPAD_DOWN}[action]
		if not InputMap.action_has_event(action,binding): InputMap.action_add_event(action,binding)
	set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	var shell_theme := Theme.new(); shell_theme.default_font_size = 18
	var focus := StyleBoxFlat.new(); focus.bg_color = Color.TRANSPARENT; focus.border_color = Color("65dfd0"); focus.set_border_width_all(3); focus.set_corner_radius_all(6)
	for kind in ["Button","CheckBox","CheckButton","OptionButton","ItemList","TabBar","LineEdit"]: shell_theme.set_stylebox("focus",kind,focus)
	for state in ["normal","hover","pressed"]:
		var style := StyleBoxFlat.new(); style.bg_color = Color("20313b" if state=="normal" else "34535e"); style.set_corner_radius_all(6); style.content_margin_left = 16; style.content_margin_right = 16; style.content_margin_top = 12; style.content_margin_bottom = 12
		shell_theme.set_stylebox(state,"Button",style)
	theme = shell_theme
	var margin := MarginContainer.new(); margin.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT); add_child(margin)
	for side in ["left","right","top","bottom"]: margin.add_theme_constant_override("margin_"+side,20)
	var layout := VBoxContainer.new(); layout.add_theme_constant_override("separation",14); margin.add_child(layout)
	header = HBoxContainer.new(); header.add_theme_constant_override("separation",8); layout.add_child(header)
	var back_button := button(header,"‹ Back",back); back_button.custom_minimum_size.x = 92
	tabs = HBoxContainer.new(); tabs.size_flags_horizontal=Control.SIZE_EXPAND_FILL; tabs.add_theme_constant_override("separation",6); header.add_child(tabs)
	for name in ["Home","Ride","Create","Activities","Settings"]:
		var tab := button(tabs,name,func(): navigate(pages[name],true)); tab.size_flags_horizontal = Control.SIZE_EXPAND_FILL; tab.toggle_mode = true; tab_buttons.append(tab)
		var selected := StyleBoxFlat.new(); selected.bg_color=Color("23505a"); selected.border_color=Color("65dfd0"); selected.set_border_width_all(2); selected.set_corner_radius_all(6)
		selected.content_margin_left=16; selected.content_margin_right=16; selected.content_margin_top=12; selected.content_margin_bottom=12
		tab.add_theme_stylebox_override("pressed",selected); tab.add_theme_stylebox_override("hover_pressed",selected)
	header.add_child(preload("res://window_mode_button.gd").new())
	body = Control.new(); body.size_flags_vertical = Control.SIZE_EXPAND_FILL; layout.add_child(body)
	hints = Label.new(); hints.text = "Arrow keys / Tab: move   Enter: select   Esc: back"; layout.add_child(hints)
	register(inspector,"Inspect source")
	register(app.content_library,"Ride"); pages.Ride = app.content_library
	register(app.route_workflow,"Legacy section builder")
	app.ride_project = preload("res://ride_project.gd").new()
	register(app.ride_project,"Ride Project"); pages.Create = app.ride_project
	app.ride_project.legacy_requested.connect(func(): navigate(app.route_workflow))
	app.ride_project.preview_requested.connect(app._open_scenery)
	var preferences := ConfigFile.new(); preferences.load("user://authoring.cfg")
	var last_project: String = str(preferences.get_value("authoring","last_project",""))
	var last_ride_project: String = str(preferences.get_value("authoring","last_ride_project",""))
	if FileAccess.file_exists(last_ride_project.path_join("project.json")):
		app.ride_project.run_job("open", last_ride_project)
	if FileAccess.file_exists(last_project.path_join("project.json")):
		app.route_workflow.project_path = last_project; app.route_workflow.reload_project()
	register(app.activity_export,"Export activity")
	register(app.device_monitor,"Device diagnostics")
	history = preload("res://activity_history.gd").new(); register(history,"Activities"); pages.Activities = history
	history.export_requested.connect(func(path: String): app.activity_export.select_session(path); navigate(app.activity_export))
	var home := Control.new(); register(home,"Home"); pages.Home = home
	home.add_child(preload("res://home_replay.gd").new())
	var plaque := PanelContainer.new(); plaque.anchor_top=1.0; plaque.anchor_bottom=1.0; plaque.offset_left=38; plaque.offset_right=440; plaque.offset_top=-169; plaque.offset_bottom=-30; plaque.mouse_filter=Control.MOUSE_FILTER_IGNORE; home.add_child(plaque)
	var plaque_style := StyleBoxFlat.new(); plaque_style.bg_color=Color(0.055,0.12,0.15,0.82); plaque_style.set_corner_radius_all(8); plaque_style.set_content_margin_all(18); plaque.add_theme_stylebox_override("panel",plaque_style)
	var identity := VBoxContainer.new(); identity.add_theme_constant_override("separation",3); plaque.add_child(identity)
	label(identity,"OpenRide",43)
	var build_info := home_build_info()
	label(identity,"Version " + str(build_info.get("version","0.1.0")),17)
	label(identity,"Built " + str(build_info.get("built_utc","development")),15)
	app.content_library.refresh()
	var settings := page("Settings"); pages.Settings = settings
	label(settings,"Devices and tools",30)
	label(settings,"Connect your trainer and optional heart-rate source from the ride setup screen.",18)
	var discovery := preload("res://device_discovery.gd").new(); register(discovery,"Device discovery")
	button(settings,"Discover devices / save diagnostics",func(): navigate(discovery))
	button(settings,"Recorded device diagnostics",func(): navigate(app.device_monitor))
	button(settings,"Inspect imported source files",func(): navigate(inspector))
	button(settings,"Export a recording",func(): navigate(app.activity_export))
	label(settings,"Live ride: Q/E or shoulders shift easier/harder; P or north button pauses/resumes.\nEsc or Start opens the ride menu. Nine load steps; gear 5 is neutral.\nJ/K/L or D-pad left/up/right reserve direction choices; junction navigation is not available.",18)
	label(settings,"Controller: D-pad moves focus; south button confirms; east button returns.\nShoulders change inner tabs when available, otherwise main sections.\nEditor: left stick orbits, right stick pans, triggers zoom. Elsewhere left stick moves focus.\nFile dialogs: shoulders cycle fields and Open / Cancel. Text entry uses the keyboard.",18)
	button(app.route_workflow.tabs.get_child(0),"Inspect source samples / export a section",func(): navigate(inspector))
	app.full_ride_review=preload("res://full_ride_review.gd").new(); register(app.full_ride_review,"Full ride review")
	button(app.route_workflow.tabs.get_child(0),"Full ride review / compare TCX and GPX",func(): navigate(app.full_ride_review))
	history.refresh()
	build_ride_menu()
	get_tree().auto_accept_quit = false
	get_tree().root.close_requested.connect(request_quit)
	get_tree().root.focus_exited.connect(func(): ride_inputs.held.clear())
	Input.joy_connection_changed.connect(func(device: int, connected: bool):
		if not connected: ride_inputs.disconnected(device)
		hints.text = "Controller connected" if connected else "Controller disconnected — keyboard and mouse remain available"
		if not connected: axis=Vector2.ZERO; next_repeat=0; gamepad=false)
	for action in ["ui_left","ui_right","ui_up","ui_down"]: InputMap.action_set_deadzone(action,0.35)
	for action in {"or_focus_point":JOY_BUTTON_LEFT_STICK,"or_toggle_tools":JOY_BUTTON_RIGHT_STICK,"or_delete_point":JOY_BUTTON_X,"or_undo_point":JOY_BUTTON_Y}:
		if not InputMap.has_action(action): InputMap.add_action(action)
		var binding := InputEventJoypadButton.new(); binding.button_index = {"or_focus_point":JOY_BUTTON_LEFT_STICK,"or_toggle_tools":JOY_BUTTON_RIGHT_STICK,"or_delete_point":JOY_BUTTON_X,"or_undo_point":JOY_BUTTON_Y}[action]; InputMap.action_add_event(action,binding)
	for entry in [["or_camera_left",JOY_AXIS_RIGHT_X,-1.0],["or_camera_right",JOY_AXIS_RIGHT_X,1.0],["or_camera_up",JOY_AXIS_RIGHT_Y,-1.0],["or_camera_down",JOY_AXIS_RIGHT_Y,1.0],["or_zoom_out",JOY_AXIS_TRIGGER_LEFT,1.0],["or_zoom_in",JOY_AXIS_TRIGGER_RIGHT,1.0]]:
		if not InputMap.has_action(entry[0]): InputMap.add_action(entry[0],0.25)
		var binding := InputEventJoypadMotion.new(); binding.axis=entry[1]; binding.axis_value=entry[2]; InputMap.action_add_event(entry[0],binding)
	navigate(home,true)
	for entry in [["or_orbit_left",JOY_AXIS_LEFT_X,-1.0],["or_orbit_right",JOY_AXIS_LEFT_X,1.0],["or_orbit_up",JOY_AXIS_LEFT_Y,-1.0],["or_orbit_down",JOY_AXIS_LEFT_Y,1.0]]:
		if not InputMap.has_action(entry[0]): InputMap.add_action(entry[0],0.25)
		var binding := InputEventJoypadMotion.new(); binding.axis=entry[1]; binding.axis_value=entry[2]; InputMap.action_add_event(entry[0],binding)

func label(parent: Node, text: String, size_px := 18) -> Label:
	var value := Label.new(); value.text = text; value.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART; value.add_theme_font_size_override("font_size",size_px); parent.add_child(value); return value

func home_build_info() -> Dictionary:
	var path := "res://bin/build-info.json"
	if not FileAccess.file_exists(path): return {}
	var parsed = JSON.parse_string(FileAccess.get_file_as_string(path))
	return parsed if parsed is Dictionary else {}

func button(parent: Node, text: String, callback: Callable) -> Button:
	var value := Button.new(); value.text = text; value.custom_minimum_size.y = 46; value.pressed.connect(callback); parent.add_child(value); return value

func page(title: String) -> VBoxContainer:
	var screen := VBoxContainer.new(); screen.add_theme_constant_override("separation",16); register(screen,title); return screen

func register(screen: Control, title: String) -> void:
	if screen.get_parent()!=null: screen.reparent(body)
	else: body.add_child(screen)
	screen.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	screen.set_meta("screen_title",title); screen.hide()
	if screen.has_signal("open_requested"): screen.open_requested.connect(func(s: Control): navigate(s))
	screen.visibility_changed.connect(func():
		if not switching and not screen.visible and not stack.is_empty() and stack[-1]==screen: back())

func navigate(screen: Control, top := false) -> void:
	ride_inputs.held.clear()
	axis = Vector2.ZERO; next_repeat = 0
	if active_world!=null and is_instance_valid(active_world) and active_world.active_ride() and screen!=active_world:
		show_ride_menu(); return
	if screen.get_parent()!=body: register(screen,screen.title)
	if not stack.is_empty():
		var current: Control = stack[-1]
		focused[current] = current.viewport.gui_get_focus_owner() if current is WorldScreen else get_viewport().gui_get_focus_owner()
	switching = true
	for child in body.get_children(): child.hide()
	if top: stack.clear()
	stack = stack.filter(func(s): return is_instance_valid(s))
	if stack.is_empty() or stack[-1]!=screen: stack.append(screen)
	screen.show(); switching = false
	for tab in tab_buttons:
		if pages[tab.text]==screen: current_section=tab.text
	for tab in tab_buttons:
		tab.set_pressed_no_signal(tab.text==current_section)
	tabs.visible = not (screen is WorldScreen and screen.training)
	hints.visible = screen != pages.Home
	if screen != pages.Home or top: home_back_armed=false
	if screen==history: history.refresh()
	if screen==app.route_workflow: app.route_workflow.library_directory = app.content_library.directory
	focus_screen.call_deferred(screen)

func focus_screen(screen: Control) -> void:
	if screen==pages.Home: tab_buttons[0].grab_focus(); return
	var previous: Variant = focused.get(screen)
	if is_instance_valid(previous) and previous is Control and previous.is_visible_in_tree(): previous.grab_focus(); return
	if screen is WorldScreen: screen.focus_first(); return
	if screen==app.content_library and app.content_library.listing.item_count>0: app.content_library.listing.grab_focus(); return
	for node in screen.find_children("*","Control",true,false):
		if node.is_visible_in_tree() and node.focus_mode==Control.FOCUS_ALL and not (node is BaseButton and node.disabled): node.grab_focus(); return

func back() -> void:
	if stack.is_empty(): return
	var current: Control = stack[-1]
	if current is WorldScreen and current.active_ride(): show_ride_menu(); return
	if ride_menu.visible: ride_menu.hide(); return
	stack = stack.filter(func(s): return is_instance_valid(s))
	if stack.size()<2:
		if current==pages.Home:
			if home_back_armed: request_home_exit()
			else: home_back_armed=true
		else: navigate(pages.Home,true)
		return
	stack.pop_back(); var target: Control = stack.pop_back(); navigate(target)

func request_home_exit() -> void:
	home_back_armed=false
	var confirm := ConfirmationDialog.new()
	confirm.dialog_text="Are you sure you want to exit?"
	confirm.ok_button_text="Yes"
	add_child(confirm)
	confirm.get_cancel_button().text="No"
	confirm.confirmed.connect(func():get_tree().quit())
	confirm.canceled.connect(confirm.queue_free)
	confirm.popup_centered()

func show_world(screen: Control) -> void:
	if screen.training: active_world = screen
	navigate(screen)

func build_ride_menu() -> void:
	ride_shade = ColorRect.new(); ride_shade.color=Color(0,0,0,0.65); ride_shade.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT); add_child(ride_shade); ride_shade.hide()
	ride_menu = PanelContainer.new(); ride_menu.set_anchors_and_offsets_preset(Control.PRESET_CENTER); ride_menu.offset_left=-260; ride_menu.offset_right=260; ride_menu.offset_top=-160; ride_menu.offset_bottom=160; add_child(ride_menu)
	ride_menu.visibility_changed.connect(func():
		ride_shade.visible=ride_menu.visible
		if is_instance_valid(active_world): active_world.viewport.gui_disable_input=ride_menu.visible)
	var box := VBoxContainer.new(); box.add_theme_constant_override("separation",16); ride_menu.add_child(box)
	label(box,"Ride menu",30); menu_status = label(box,"",18)
	button(box,"Return to ride",func(): ride_menu.hide(); active_world.focus_first())
	pause_button = button(box,"Pause / resume",func(): active_world.content.live_ride.toggle())
	finish_button = button(box,"End ride, save and return",func():
		leave_after_finish = true; finish_button.disabled = true; active_world.content.live_ride.finish())
	var menu_buttons := ride_menu.find_children("*","Button",true,false)
	for i in range(menu_buttons.size()):
		var previous: Button=menu_buttons[posmod(i-1,menu_buttons.size())]; var next: Button=menu_buttons[(i+1)%menu_buttons.size()]
		menu_buttons[i].focus_neighbor_top=menu_buttons[i].get_path_to(previous); menu_buttons[i].focus_previous=menu_buttons[i].get_path_to(previous)
		menu_buttons[i].focus_neighbor_bottom=menu_buttons[i].get_path_to(next); menu_buttons[i].focus_next=menu_buttons[i].get_path_to(next)
	ride_menu.hide()

func show_ride_menu() -> void:
	ride_menu.show(); ride_menu.move_to_front()
	for node in ride_menu.find_children("*","Button",true,false): node.grab_focus(); break

func request_quit() -> void:
	if is_instance_valid(active_world) and active_world.active_ride(): quitting = true; show_ride_menu(); return
	var confirm := ConfirmationDialog.new(); confirm.dialog_text = "Exit OpenRide? Save any route edits before exiting."; add_child(confirm)
	if app.full_ride_review.dirty or app.full_ride_review.busy: confirm.dialog_text="A full ride review has unsaved decisions or work in progress. Exit anyway? Return to Full ride review to save or wait."
	elif app.route_workflow.has_unsaved_edits(): confirm.dialog_text="Exit and discard unsaved route edits? Return to the editor and Save route edits to keep them."
	elif app.route_workflow.pid>0: confirm.dialog_text="A route preparation job is running. Exit the app? The separate job will continue; reopen the project to check its result."
	confirm.confirmed.connect(func(): get_tree().quit()); confirm.canceled.connect(confirm.queue_free); confirm.popup_centered()

func has_dialog() -> bool:
	for window in get_tree().root.find_children("*","Window",true,false):
		if window.visible: return true
	return false

func prepare_file_dialog(dialog: FileDialog) -> void:
	if not is_instance_valid(dialog) or dialog.has_meta("controller_navigation"): return
	dialog.set_meta("controller_navigation",true)
	var help := Label.new(); help.text = "Controller: shoulders move between fields and Open / Cancel; D-pad browses files."
	dialog.get_vbox().add_child(help)
	dialog.add_child(preload("res://file_dialog_navigation.gd").new())

func editor_orbit_active() -> bool:
	return not stack.is_empty() and stack[-1] is WorldScreen and stack[-1].content.has_method("_pan_camera") and not ride_menu.visible

func change_section(direction: int) -> void:
	if ride_menu.visible or stack.is_empty(): return
	var inner: TabContainer
	if stack[-1]==app.route_workflow: inner=app.route_workflow.tabs
	elif editor_orbit_active():
		var editor = stack[-1].content
		inner=editor.inspector_tabs
		editor.inspector_button.button_pressed=true
	if inner != null and inner.get_tab_count()>0:
		for step in range(1,inner.get_tab_count()+1):
			var index := posmod(inner.current_tab+direction*step,inner.get_tab_count())
			if not inner.is_tab_disabled(index) and not inner.is_tab_hidden(index):
				inner.current_tab=index; inner.get_tab_bar().grab_focus(); return
		return
	if tabs.visible:
		var values: Array = [pages.Home,pages.Ride,pages.Create,pages.Activities,pages.Settings]
		navigate(values[posmod(values.find(stack[-1])+direction,values.size())],true)

func _input(event: InputEvent) -> void:
	var ride_action: String = ride_inputs.decode(event)
	if event is InputEventJoypadButton or event is InputEventJoypadMotion:
		gamepad = true; hints.text = "D-pad: focus   Left stick: orbit   Right stick: pan   Shoulders: tool tabs" if editor_orbit_active() else "D-pad / left stick: move   South: select   East: back   Shoulders: current tabs"
	elif event is InputEventKey and event.device!=-10:
		gamepad = false; hints.text = "Arrow keys / Tab: move   Enter: select   Esc: back"
	if has_dialog(): axis=Vector2.ZERO; next_repeat=0; return
	if not ride_action.is_empty() and handle_ride_action(ride_action):
		get_viewport().set_input_as_handled(); return
	if event.is_action_pressed("ui_cancel"):
		if ride_menu.visible: ride_menu.hide(); quitting = false
		else: back()
		get_viewport().set_input_as_handled()
	if event is InputEventJoypadButton and event.pressed and event.button_index in [JOY_BUTTON_LEFT_SHOULDER,JOY_BUTTON_RIGHT_SHOULDER] and tabs.visible:
		change_section(-1 if event.button_index==JOY_BUTTON_LEFT_SHOULDER else 1)
		get_viewport().set_input_as_handled()
	if event is InputEventJoypadMotion and event.axis in [JOY_AXIS_LEFT_X,JOY_AXIS_LEFT_Y]:
		if editor_orbit_active(): axis=Vector2.ZERO; next_repeat=0; return
		if event.axis==JOY_AXIS_LEFT_X: axis.x = event.axis_value
		else: axis.y = event.axis_value
		if axis.length()<0.35: next_repeat = 0
		get_viewport().set_input_as_handled()

func handle_ride_action(action: String) -> bool:
	if not is_instance_valid(active_world) or stack.is_empty() or stack[-1] != active_world: return false
	var ride = active_world.content.live_ride
	if ride == null or not ride.active(): return false
	var focus = get_viewport().gui_get_focus_owner()
	if focus is LineEdit or focus is TextEdit: return false
	if action == "or_menu" or action == "or_back":
		if ride_menu.visible: ride_menu.hide(); quitting=false
		else: back()
		return true
	if ride_menu.visible: return false
	match action:
		"or_shift_down", "or_shift_up": ride.shift(-1 if action == "or_shift_down" else 1)
		"or_pause": ride.toggle()
		"or_choose_left", "or_choose_straight", "or_choose_right":
			ride.shift_note = "This route has no selectable junctions."
		_: return false
	return true

func _process(_delta: float) -> void:
	if has_dialog(): ride_inputs.held.clear()
	if gamepad and axis.length()>=0.5 and Time.get_ticks_msec()>=next_repeat and not has_dialog() and not editor_orbit_active():
		var key := InputEventKey.new(); key.pressed = true
		key.device = -10
		key.keycode = (KEY_RIGHT if axis.x>0 else KEY_LEFT) if absf(axis.x)>absf(axis.y) else (KEY_DOWN if axis.y>0 else KEY_UP)
		var first := next_repeat==0; next_repeat = Time.get_ticks_msec()+(350 if first else 130)
		if not stack.is_empty() and stack[-1] is WorldScreen and not ride_menu.visible: stack[-1].viewport.push_input(key)
		else: get_viewport().push_input(key)
		gamepad = true
	if ride_menu.visible and is_instance_valid(active_world) and active_world.content.live_ride!=null:
		var ride = active_world.content.live_ride
		menu_status.text = ride.note
		pause_button.disabled = not ride.active() or not ride.pending.is_empty()
		pause_button.text = "Pause ride" if ride.phase=="running" else "Resume ride"
		if leave_after_finish and not ride.active():
			# The worker can exit between the ride screen's poll and this frame.
			# Drain completion/failure before hiding the screen disables its processing.
			ride.poll()
			leave_after_finish = false; ride_menu.hide(); finish_button.disabled = false
			if quitting: get_tree().quit(); return
			navigate(history,true)
