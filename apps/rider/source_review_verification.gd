extends RefCounted

func check(ok: bool, message: String, app: Node) -> bool:
	if not ok: push_error("SOURCE_REVIEW_FAILED: "+message); app.get_tree().quit(1)
	return ok

func settled(screen: Control, app: Node) -> bool:
	var deadline := Time.get_ticks_msec()+15000
	while screen.busy and Time.get_ticks_msec()<deadline: await app.get_tree().process_frame
	return check(not screen.busy and screen.last_operation_ok and not screen.review.is_empty(),"Review operation completed: "+screen.status.text,app)

func run(app: Node) -> void:
	var root := ""; var inputs := ""; var snapshot := ""
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--review-root="): root=arg.trim_prefix("--review-root=")
		if arg.begins_with("--review-inputs="): inputs=arg.trim_prefix("--review-inputs=")
		if arg.begins_with("--review-snapshot="): snapshot=arg.trim_prefix("--review-snapshot=")
	if not check(not root.is_empty() and not DirAccess.dir_exists_absolute(root),"New verification directory required",app): return
	DirAccess.make_dir_recursive_absolute(root)
	var source := "<gpx><trk><name>Synthetic full ride</name><trkseg>"
	for row in [["33","00:00:00",10],["33.1","00:00:30",500],["33.11","00:05:00",510],["33.12","00:05:30",490]]:
		source += '<trkpt lat="%s" lon="-118"><time>2022-01-01T%sZ</time><ele>%d</ele></trkpt>' % row
	source += "</trkseg></trk></gpx>"
	var paths := PackedStringArray()
	for i in range(2):
		var path := root.path_join("synthetic-%d.gpx" % i)
		var file := FileAccess.open(path,FileAccess.WRITE); file.store_string(source.replace("Synthetic full ride","Synthetic full ride %d" % i)); file.close(); paths.append(path)
	var original_hash := FileAccess.get_sha256(paths[0])
	var screen = app.full_ride_review; app.shell.navigate(screen)
	screen.create_review(paths,root.path_join("synthetic-review"))
	if not await settled(screen,app): return
	if not check(screen.review.activities.size()==2 and screen.review.activities[0].route_distance_m>5000,"Full variants imported beyond 5 km",app): return
	if not check(screen.review.document.geometry_source==null and screen.map.samples.size()==4 and screen.profile.samples.size()==4,"No automatic source choice or truncated overview",app): return
	screen.geometry.select(1); screen.geometry.item_selected.emit(1)
	screen.elevation.select(2); screen.elevation.item_selected.emit(2)
	screen.gaps.select(0); screen.gaps.item_selected.emit(0)
	screen.classification.select(2); screen.classification.item_selected.emit(2)
	screen.note.text="Reviewed recording pause; keep source break"; screen.note.text_changed.emit(screen.note.text)
	if not check(screen.dirty,"Decision edits are unsaved",app): return
	var directory: String=screen.directory
	screen.open_review(root.path_join("other"))
	if not check(not screen.busy and screen.directory==directory and screen.dirty,"Unsaved changes block replacement",app): return
	screen.save_button.pressed.emit()
	if not await settled(screen,app): return
	if not check(not screen.dirty,"Save completed: "+screen.status.text,app): return
	var saved: Dictionary=screen.review.document.duplicate(true)
	screen.open_review(directory)
	if not await settled(screen,app): return
	if not check(screen.review.document==saved and screen.review.activities[0].samples[2].break_before,"Reopen preserves decisions without joining gap",app): return
	app.shell.navigate(app.shell.pages.Home,true); app.shell.navigate(screen)
	if not check(screen.review.document==saved,"Navigation retains review",app): return
	if not check(FileAccess.get_sha256(paths[0])==original_hash,"Original unchanged",app): return
	var reopened: RefCounted=ClassDB.instantiate("FullRideReviewer")
	reopened.start_review(JSON.stringify({"action":"open","directory":directory}))
	var raw := ""; var deadline := Time.get_ticks_msec()+15000
	while raw.is_empty() and Time.get_ticks_msec()<deadline:
		raw=reopened.poll_review(); await app.get_tree().process_frame
	if not check(not raw.is_empty() and JSON.parse_string(raw).review.document==saved,"New bridge reopens saved decisions",app): return
	if not inputs.is_empty():
		var cases: Array=JSON.parse_string(FileAccess.get_file_as_string(inputs))
		for i in range(cases.size()):
			var hashes: Array=[]
			for path in cases[i]: hashes.append(FileAccess.get_sha256(path))
			screen.create_review(PackedStringArray(cases[i]),root.path_join("private-review-%d" % i))
			if not await settled(screen,app): return
			if not check(screen.review.activities.size()==2 and screen.review.activities[0].route_distance_m>100000,"Private full-ride pair retained",app): return
			for j in range(cases[i].size()):
				if not check(FileAccess.get_sha256(cases[i][j])==hashes[j],"Private original unchanged",app): return
			screen.open_review(screen.directory)
			if not await settled(screen,app): return
			if not check(screen.review.document.geometry_source==null and screen.review.document.elevation_source==null and screen.review.document.decisions.is_empty(),"Private decisions remain explicit and unselected",app): return
		print("SOURCE_REVIEW_PRIVATE_OK pairs=",cases.size()," decisions_left_unselected=true")
	print("SOURCE_REVIEW_OK: full overview, explicit sources, saved gap review, reopen and original preservation")
	if not snapshot.is_empty(): await app._snapshot(snapshot); return
	app.get_tree().quit()
