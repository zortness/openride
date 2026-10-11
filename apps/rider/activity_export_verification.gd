extends RefCounted

func run(tree: SceneTree) -> void:
	var source := ""
	var destination := ""
	var screenshot := ""
	var manifest := ""
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--manifest="): manifest = arg.trim_prefix("--manifest=")
		if arg.begins_with("--journal="): source = arg.trim_prefix("--journal=")
		if arg.begins_with("--output="): destination = arg.trim_prefix("--output=")
		if arg.begins_with("--export-snapshot="): screenshot = arg.trim_prefix("--export-snapshot=")
	if source.is_empty() or destination.is_empty():
		push_error("Provide --journal and a new --output directory")
		tree.quit(1)
		return
	var window = load("res://activity_export.gd").new()
	tree.root.add_child(window)
	window.manifest = manifest
	window.source = source
	window.source_label.text = source
	window.begin_export(source, destination)
	var deadline := Time.get_ticks_msec() + 20000
	while window.busy and Time.get_ticks_msec() < deadline:
		await tree.process_frame
	var report_path := destination.path_join("report.json")
	if window.busy or not FileAccess.file_exists(report_path) or not window.result_label.text.begins_with("Exported"):
		push_error("Activity export failed: " + window.result_label.text)
		tree.quit(1)
		return
	var report = JSON.parse_string(FileAccess.get_file_as_string(report_path))
	if report.source_sha256 != FileAccess.get_sha256(source) or report.fit_records <= 0:
		push_error("Export metadata does not match source")
		tree.quit(1)
		return
	if not screenshot.is_empty():
		window.popup_centered()
		await tree.process_frame
		await RenderingServer.frame_post_draw
		window.get_texture().get_image().save_png(screenshot)
	print("ACTIVITY_EXPORT_OK")
	tree.quit()
