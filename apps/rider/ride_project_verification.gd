extends RefCounted

func check(value: bool, message: String, app: Node) -> bool:
	if not value: push_error("RIDE_PROJECT_FAILED: " + message); app.get_tree().quit(1)
	return value

func settled(screen: Control, app: Node) -> bool:
	var deadline := Time.get_ticks_msec() + 15000
	while screen.busy and Time.get_ticks_msec() < deadline: await app.get_tree().process_frame
	return check(not screen.busy, "Native job timed out", app)

func run(app: Node) -> void:
	var root := ""
	var screenshot := ""
	var package_zip := ""
	var build_project := ""
	var sources := PackedStringArray()
	for argument in OS.get_cmdline_user_args():
		if argument.begins_with("--project-test-root="): root = argument.trim_prefix("--project-test-root=")
		if argument.begins_with("--project-screenshot="): screenshot = argument.trim_prefix("--project-screenshot=")
		if argument.begins_with("--import-source="): sources.append(argument.trim_prefix("--import-source="))
		if argument.begins_with("--package-zip="): package_zip = argument.trim_prefix("--package-zip=")
		if argument.begins_with("--build-project="): build_project = argument.trim_prefix("--build-project=")
	if not check(not root.is_empty() and not DirAccess.dir_exists_absolute(root), "Supply a new test root", app): return
	var screen = app.ride_project
	if not await settled(screen, app): return
	if not check(app.shell.pages.Create == screen, "Create must open Ride Project", app): return
	app.shell.navigate(screen, true)
	screen.project_name.text = "Mountain ride"
	var original := root.path_join("Original project")
	screen.run_job("create", original)
	if not await settled(screen, app): return
	if not check(screen.project_path == original and screen.snapshot.project.name == "Mountain ride", "Create failed: " + screen.feedback.text, app): return
	var identity: String = screen.snapshot.project.id
	var revision: String = screen.snapshot.revision
	var moved := root.path_join("Moved project")
	if not check(DirAccess.rename_absolute(original, moved) == OK, "Project move failed", app): return
	# A fresh screen must load from disk without the original in-memory state.
	var reopened := preload("res://ride_project.gd").new(); app.add_child(reopened); reopened.hide()
	reopened.run_job("open", moved)
	if not await settled(reopened, app): return
	if not check(reopened.snapshot.get("revision") == revision and reopened.snapshot.project.id == identity, "Moved project identity changed", app): return
	reopened.project_name.text = "Coast ride"; reopened.run_job("rename", moved)
	if not await settled(reopened, app): return
	if not check(reopened.snapshot.project.name == "Coast ride", "Rename failed", app): return
	screen.run_job("rename", moved)
	if not await settled(screen, app): return
	if not check(screen.feedback.text.contains("changed since opening"), "Stale writer was not rejected", app): return
	screen.run_job("open", moved)
	if not await settled(screen, app): return
	if not check(screen.snapshot.project.name == "Coast ride", "Saved rename did not reopen", app): return
	screen.run_job("create", moved)
	if not await settled(screen, app): return
	if not check(screen.feedback.text.contains("empty folder"), "Existing project replaced", app): return
	screen.run_job("open", moved)
	if not await settled(screen, app): return
	reopened.queue_free()
	if not sources.is_empty():
		screen.intake.import_sources(sources)
		if not await settled(screen, app): return
		if not check(screen.snapshot.has("review"), "Import failed: " + screen.feedback.text, app): return
		var review: Dictionary = screen.snapshot.review
		if not check(review.activities.size() == sources.size(), "Source variants missing", app): return
		if not check(review.activities[0].route_distance_m > 5000, "Whole recording was truncated", app): return
		if not check(not screen.intake.log_view.visible and not screen.intake.log_toggle.button_pressed, "Log must start hidden", app): return
		screen.intake.log_toggle.button_pressed = true
		if not check(screen.intake.log_view.visible and not screen.intake.log_view.text.is_empty(), "Log toggle failed", app): return
		screen.intake.log_toggle.button_pressed = false
		screen.intake.choices[1].select(1); screen.intake.choices[2].select(sources.size())
		screen.intake.save_choices()
		if not await settled(screen, app): return
		var selected: Dictionary = screen.snapshot.review.document
		if not check(selected.geometry_source == review.document.sources[0].sha256 and selected.elevation_source == review.document.sources[-1].sha256, "Source choices not saved", app): return
		if not check(selected.get("repairs", {}).is_empty(), "Import unexpectedly repaired elevations", app): return
		var imported_moved := root.path_join("Moved imported project")
		if not check(DirAccess.rename_absolute(moved, imported_moved) == OK, "Imported folder move failed", app): return
		screen.run_job("open", imported_moved)
		if not await settled(screen, app): return
		if not check(screen.snapshot.review.document == selected, "Imported choices changed on reopen", app): return
		print("RIDE_PROJECT_IMPORT_OK complete_sources saved_roles moved_reopen hidden_log no_automatic_repairs")
		if OS.get_cmdline_user_args().has("--verify-project-repairs"):
			var repair_ids: Array = []
			for c in screen.snapshot.repair_preview.candidates:
				if c.allowed: repair_ids.append(c.id)
			if not check(repair_ids.size() == 2, "Expected gap and loop proposals", app): return
			var original_count: int = screen.snapshot.repair_preview.samples.size()
			for id in repair_ids:
				screen.repairs.selected_id = id; screen.repairs.decide("accepted")
				if not await settled(screen, app): return
			if not check(screen.snapshot.repair_preview.samples.size() < original_count and screen.snapshot.repair_preview.unresolved_breaks == 0, "Repairs did not change derived geometry", app): return
			screen.run_job("open", imported_moved)
			if not await settled(screen, app): return
			if not check(screen.snapshot.repair_preview.unresolved_breaks == 0, "Gap repair did not persist", app): return
			for id in repair_ids:
				screen.repairs.selected_id = id; screen.repairs.decide("pending")
				if not await settled(screen, app): return
			if not check(screen.snapshot.repair_preview.samples.size() == original_count and screen.snapshot.repair_preview.unresolved_breaks == 1, "Undo did not restore original topology", app): return
			screen.run_job("plan-maps", imported_moved)
			if not await settled(screen, app): return
			if not check(screen.snapshot.has("maps") and screen.snapshot.maps.tiles.size() > 1, "Full map plan missing", app): return
			print("PROJECT_REPAIRS_MAPS_OK gap loop accept reopen undo full_corridor_plan")
			# Synthetic offline acquisition receipt exercises the real native course CLI.
			var plan: Dictionary = screen.snapshot.maps
			var tile_dir: String = imported_moved.path_join("maps").path_join(plan.key).path_join(plan.tiles[0].id)
			DirAccess.make_dir_recursive_absolute(tile_dir)
			var osm := '<osm version="0.6"><node id="1" version="1" lat="33" lon="-118"/><node id="2" version="1" lat="33.06" lon="-118"/><node id="3" version="1" lat="33.0601" lon="-118"/><way id="10" version="1"><nd ref="1"/><nd ref="2"/><nd ref="3"/><tag k="highway" v="cycleway"/></way></osm>'
			var source_file := FileAccess.open(tile_dir.path_join("streets.osm"), FileAccess.WRITE); source_file.store_string(osm); source_file.close()
			var receipt := FileAccess.open(tile_dir.path_join("streets.receipt.json"), FileAccess.WRITE); receipt.store_string(JSON.stringify({"streets.osm": osm.sha256_text()})); receipt.close()
			screen.run_job("plan-maps", imported_moved)
			if not await settled(screen, app): return
			if not check(not screen.course.button.disabled, "Build course button unavailable with cached streets", app): return
			screen.run_job("build-course", screen.project_path)
			if not await settled(screen, app): return
			if not check(screen.snapshot.has("course") and screen.course.issues.item_count > 0, "Course result or unresolved issues missing: " + screen.feedback.text, app): return
			screen.run_job("open", imported_moved)
			if not await settled(screen, app): return
			if not check(screen.snapshot.has("course"), "Course failed to reopen offline", app): return
			print("PROJECT_COURSE_OK native_build offline_reopen unresolved_coverage")
			screen.course.issues.select(0); screen.course.select_issue(0)
			var edited_sample: int = screen.course.selected_sample
			screen.course.edit("trace")
			if not await settled(screen, app): return
			if not check(screen.snapshot.course.sections.any(func(s): return s.source_sample == edited_sample and s.status == "manual"), "Trace edit not saved: sample=%d sections=%s " % [edited_sample, screen.snapshot.course.sections.map(func(s):return [s.source_sample,s.status])] + screen.feedback.text, app): return
			screen.run_job("open", imported_moved)
			if not await settled(screen, app): return
			if not check(screen.snapshot.course.sections.any(func(s): return s.source_sample == edited_sample and s.status == "manual"), "Edit missing after reopen", app): return
			screen.course.selected_sample = edited_sample; screen.course.edit("undo")
			if not await settled(screen, app): return
			if not check(screen.snapshot.course.sections.any(func(s): return s.source_sample == edited_sample and s.status != "manual"), "Undo failed", app): return
			print("PROJECT_EDITS_OK trace accept reopen undo native_cli")
		var scroll = screen.find_children("*", "ScrollContainer", true, false)[0]
		scroll.scroll_vertical = 2400 if OS.get_cmdline_user_args().has("--verify-project-repairs") else 620
	if not package_zip.is_empty():
		var library := preload("res://content_library.gd").new(); app.add_child(library); library.hide()
		var opened: Array = []
		library.package_selected.connect(func(path: String): opened.append(path))
		library.install_zip(package_zip, true)
		var deadline := Time.get_ticks_msec() + 45000
		while library.package_worker != null and Time.get_ticks_msec() < deadline: await app.get_tree().process_frame
		if not check(opened.size() == 1, "ZIP did not open: " + library.location.text, app): return
		library.directory = package_zip.get_base_dir(); library.refresh()
		if not check(library.entries.any(func(e): return e.get("packed", false)), "ZIP missing from library", app): return
		library.directory = opened[0]; library.refresh()
		if not check(library.entries.size() == 1 and not library.entries[0].has("error"), "Unpacked folder not recognized directly", app): return
		library.run_package("pack", opened[0], root.path_join("UI export.zip"), false)
		while library.package_worker != null and Time.get_ticks_msec() < deadline: await app.get_tree().process_frame
		if not check(FileAccess.file_exists(root.path_join("UI export.zip")), "UI ZIP export failed: " + library.location.text, app): return
		print("PROJECT_ZIP_OK native_install library_zip unpacked_folder native_export")
		library.queue_free()
	if not build_project.is_empty():
		screen.run_job("open", build_project)
		if not await settled(screen, app): return
		if OS.get_cmdline_user_args().has("--verify-road-first"):
			if not check(screen.stages.get_tab_count()==4,"Road workflow requires four stages",app):return
			screen.stages.current_tab=1
			screen.maps.prepare_button.pressed.emit()
			if not await settled(screen,app):return
			if not check(screen.snapshot.get("course",{}).has("roads") and screen.snapshot.course.unresolved==0,"Road inference failed: "+screen.feedback.text,app):return
			screen.course.show_all.button_pressed=true
			screen.course.select_issue(5)
			var road_sample:int=screen.course.selected_sample
			screen.course.choose_road(false)
			if not await settled(screen,app):return
			screen.run_job("open",build_project)
			if not await settled(screen,app):return
			if not check(screen.snapshot.course.sections.any(func(s):return s.source_sample==road_sample and s.status=="manual_match"),"Road choice did not persist",app):return
			screen.course.selected_sample=road_sample;screen.course.choose_road(true)
			if not await settled(screen,app):return
			if not check(screen.snapshot.course.sections.any(func(s):return s.source_sample==road_sample and s.status=="matched"),"Road choice undo failed",app):return
			for argument in OS.get_cmdline_user_args():
				if argument.begins_with("--road-screenshot="):
					var scroll=screen.stages.get_child(1);scroll.scroll_vertical=300
					await app.get_tree().process_frame;await RenderingServer.frame_post_draw
					if not check(app.get_viewport().get_texture().get_image().save_png(argument.trim_prefix("--road-screenshot="))==OK,"Road screenshot failed",app):return
			print("ROAD_FIRST_UI_OK four_stages prepare_maps infer named_choices save reopen undo")
		if OS.get_cmdline_user_args().has("--verify-project-scenery"):
			var editor:Control=screen.scenery
			editor.new_polygon()
			for p in [[33.001,-117.9994],[33.001,-117.9992],[33.0012,-117.9992],[33.0012,-117.9994]]:editor.place_vertex(p[0],p[1])
			var scenery_id:String=editor.selected_id
			editor.edit("save")
			if not await settled(screen,app):return
			if not check(screen.snapshot.scenery.features.any(func(f):return f.id==scenery_id),"Scenery polygon save failed: "+screen.feedback.text,app):return
			editor.edit("hide")
			if not await settled(screen,app):return
			if not check(screen.snapshot.scenery.features.any(func(f):return f.id==scenery_id and f.hidden),"Scenery hide failed",app):return
			editor.edit("undo")
			if not await settled(screen,app):return
			if not check(screen.snapshot.scenery.features.any(func(f):return f.id==scenery_id and not f.hidden),"Scenery undo failed",app):return
			editor.edit("reset")
			if not await settled(screen,app):return
			print("PROJECT_SCENERY_OK polygon_save hide undo reset native_cli")
		if not check(not screen.build.build_button.disabled, "Native build unavailable", app): return
		screen.build.build_button.pressed.emit()
		if not await settled(screen, app): return
		if not check(screen.snapshot.has("build"), "Native package build failed: " + screen.feedback.text, app): return
		screen.build.export_button.pressed.emit()
		if not await settled(screen, app): return
		if not check(screen.snapshot.build.get("export") is String and FileAccess.file_exists(build_project.path_join(screen.snapshot.build.export)), "Project-owned ZIP export missing", app): return
		print("PROJECT_BUILD_OK native_package preserved_version automatic_zip_path")
		screen.build.preview_button.pressed.emit()
		var runtime: Node = app.scenery_window.content
		var load_deadline := Time.get_ticks_msec() + 15000
		while not runtime.failed and not runtime.neighborhood_ready() and Time.get_ticks_msec() < load_deadline: await app.get_tree().process_frame
		if not check(not runtime.failed and runtime.neighborhood_ready(), "Native package preview failed to load", app): return
		print("PROJECT_BUILD_PREVIEW_OK native_geometry runtime_loaded")
	if not screenshot.is_empty():
		await app.get_tree().process_frame; await RenderingServer.frame_post_draw
		if not check(app.get_viewport().get_texture().get_image().save_png(screenshot) == OK, "Screenshot failed", app): return
	print("RIDE_PROJECT_OK create move reopen rename stale_writer nonempty_folder native_cli")
	app.get_tree().quit()
