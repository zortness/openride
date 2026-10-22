extends SceneTree

# Capture real app screens using a synthetic project. The project is opened read-only.
func _initialize() -> void:
	capture.call_deferred()

func capture() -> void:
	var view := ""
	var output := ""
	var project := ""
	var package := ""
	for argument in OS.get_cmdline_user_args():
		if argument.begins_with("--capture-view="): view = argument.trim_prefix("--capture-view=")
		if argument.begins_with("--capture-output="): output = argument.trim_prefix("--capture-output=")
		if argument.begins_with("--capture-project="): project = argument.trim_prefix("--capture-project=")
		if argument.begins_with("--capture-package="): package = argument.trim_prefix("--capture-package=")
	if not view in ["home", "roads", "preview"] or not output.is_absolute_path():
		fail("Supply --capture-view=home|roads|preview and an absolute --capture-output path")
		return
	var app: Control = load("res://main.tscn").instantiate()
	root.add_child(app)
	await process_frame
	if view == "roads":
		if not FileAccess.file_exists(project.path_join("project.json")):
			fail("Supply a synthetic Ride Project with --capture-project")
			return
		app.shell.navigate(app.ride_project, true)
		app.ride_project.run_job("open", project)
		var deadline := Time.get_ticks_msec() + 20000
		while app.ride_project.busy and Time.get_ticks_msec() < deadline:
			await process_frame
		if app.ride_project.busy or not app.ride_project.snapshot.get("course", {}).has("roads"):
			fail("Road-first project failed to open")
			return
		app.ride_project.stages.current_tab = 1
		app.ride_project.course.show_all.button_pressed = true
		app.ride_project.course.select_issue(5)
		app.ride_project.stages.get_child(1).scroll_vertical = 550
	elif view == "preview":
		if not FileAccess.file_exists(package.path_join("manifest.json")):
			fail("Supply a synthetic built package with --capture-package")
			return
		app.shell.navigate(app.shell.pages.Ride, true)
		app._open_scenery(package)
		var runtime = app.scenery_window.content
		var deadline := Time.get_ticks_msec() + 20000
		while not runtime.failed and not runtime.neighborhood_ready() and Time.get_ticks_msec() < deadline:
			await process_frame
		if runtime.failed or not runtime.neighborhood_ready():
			fail("Preview failed to load nearby scenery")
			return
		runtime.seek(0.0)
	for i in range(5): await process_frame
	await RenderingServer.frame_post_draw
	if root.get_texture().get_image().save_png(output) != OK:
		fail("Could not save screenshot")
		return
	print("DOCS_CAPTURE_OK ", view, " ", output)
	quit(0)

func fail(message: String) -> void:
	push_error("DOCS_CAPTURE_FAILED: " + message)
	quit(1)
