extends RefCounted

var tree: SceneTree

func run(app: Control) -> void:
	tree = app.get_tree()
	tree.create_timer(35.0).timeout.connect(func():
		push_error("Native test did not complete within 35 seconds")
		tree.quit(1))
	if not ClassDB.class_exists("RideInspector"):
		push_error("Native RideInspector extension is required")
		tree.quit(1)
		return
	var activity_path := ""
	var output_path := ""
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--fixture="): activity_path = arg.trim_prefix("--fixture=")
		if arg.begins_with("--output="): output_path = arg.trim_prefix("--output=")
	if not check(not activity_path.is_empty() and not output_path.is_empty()): return
	if not check(not FileAccess.file_exists(output_path)): return
	app.import_paths(PackedStringArray([activity_path]))
	var deadline := Time.get_ticks_msec() + 30000
	while app.importing:
		if Time.get_ticks_msec() > deadline:
			push_error("Native import timed out")
			tree.quit(1)
			return
		await tree.process_frame
	if not check(app.library.errors.is_empty()): return
	if not check(app.activity.samples.size() == 1014): return
	app.start_sample.value = 100
	app.end_sample.value = 800
	app.export_selection(output_path)
	if not FileAccess.file_exists(output_path):
		push_error("Export failed: " + app.status.text)
		tree.quit(1)
		return
	app.open_route(output_path)
	if not check(int(app.route_document.source_range[0]) == 100 and int(app.route_document.source_range[1]) == 800): return
	if not check(app.route_document.points.size() == 701): return
	if not check(app.export_button.disabled): return
	print("NATIVE_OK: threaded import, subsection export, validated reopen")
	tree.quit()

func check(condition: bool) -> bool:
	if not condition:
		push_error('Native test condition failed')
		tree.quit(1)
	return condition
