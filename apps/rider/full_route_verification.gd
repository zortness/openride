extends RefCounted

func check(ok: bool,message: String,app: Node) -> bool:
	if not ok: push_error("FULL_PREVIEW_FAILED: "+message); app.get_tree().quit(1)
	return ok

func wait_for(screen: Control,app: Node) -> bool:
	var deadline := Time.get_ticks_msec()+15000
	while screen.busy and Time.get_ticks_msec()<deadline: await app.get_tree().process_frame
	return check(not screen.busy,"Worker timeout",app)

func run(app: Node) -> void:
	var root := ""; var source_path := ""; var snapshot := ""
	var test_repairs := OS.get_cmdline_user_args().has("--verify-route-repairs")
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with("--preview-root="): root=arg.trim_prefix("--preview-root=")
		if arg.begins_with("--preview-source="): source_path=arg.trim_prefix("--preview-source=")
		if arg.begins_with("--preview-snapshot="): snapshot=arg.trim_prefix("--preview-snapshot=")
	if not check(not root.is_empty() and not DirAccess.dir_exists_absolute(root),"New test directory required",app): return
	DirAccess.make_dir_recursive_absolute(root)
	var synthetic := source_path.is_empty()
	if synthetic:
		source_path=root.path_join("synthetic.gpx")
		var xml := "<gpx><trk><trkseg>"
		for i in range(400):
			var lat := 33+i*0.0005 if i<200 else 33+(i-1)*0.0005+0.00002
			if test_repairs and i==200: lat=33+199*0.0005+0.000007
			if test_repairs and i==198: lat=33+199*0.0005-0.0001
			if test_repairs and i==201: lat=33+199*0.0005+0.000107
			var height := 10+sin(i*0.03)
			if test_repairs and i==199: height=10.0
			if test_repairs and i==200: height=11.0
			var timestamp := Time.get_datetime_string_from_unix_time(1640995200+i*5+(300 if i>=200 else 0))+"Z"
			xml+='<trkpt lat="%.7f" lon="-118"><ele>%.3f</ele><time>%s</time></trkpt>' % [lat,height,timestamp]
		xml+="</trkseg></trk></gpx>"
		var file := FileAccess.open(source_path,FileAccess.WRITE); file.store_string(xml); file.close()
	var original_hash := FileAccess.get_sha256(source_path)
	var screen=app.full_ride_review; app.shell.navigate(screen)
	screen.create_review(PackedStringArray([source_path]),root.path_join("review"))
	if not await wait_for(screen,app): return
	if not check(screen.last_operation_ok,screen.status.text,app): return
	# Explicit decisions apply only to this new disposable test review.
	screen.geometry.select(1); screen.geometry.item_selected.emit(1)
	screen.elevation.select(1); screen.elevation.item_selected.emit(1)
	for gap in screen.review.gaps: screen.review.document.decisions[gap.id]={"classification":"stationary_pause","note":"Automated preview fixture; original review unchanged"}
	screen.changed(); screen.open_preview()
	if not check(screen.full_preview==null,"Unsaved decisions must block preview",app): return
	screen.save_review()
	if not await wait_for(screen,app): return
	if not check(screen.last_operation_ok,screen.status.text,app): return
	screen.open_preview(); var preview=screen.full_preview
	if not await wait_for(preview,app): return
	if not check(not preview.summary.is_empty(),preview.status.text,app): return
	if not check(preview.summary.length_m>20000,"No 5 km preview cap",app): return
	if synthetic and not test_repairs and not check(preview.summary.bridged_pauses==1 and preview.summary.unresolved_breaks==0,"Short pause connected",app): return
	if test_repairs:
		var original_breaks: int=preview.summary.unresolved_breaks
		var proposal_count: int=preview.summary.repairs.size()
		if not check(proposal_count>0 and original_breaks>0,"Repair proposed without modifying route",app): return
		for action in [["accepted",0],["rejected",original_breaks],["proposed",original_breaks],["accepted",0]]:
			for i in range(proposal_count):
				preview.repair_choice.select(i); preview.decide_repair(action[0])
				if not await wait_for(preview,app): return
				if not await wait_for(screen,app): return
				if not check(not preview.summary.is_empty(),preview.status.text,app): return
				if not check(screen.review.revision==preview.summary.revision,"Parent review refreshed",app): return
				if not check(preview.summary.repairs[i].status==action[0],"Decision persisted",app): return
			if not check(preview.summary.unresolved_breaks==action[1],"Repair decision applied",app): return
		# Exercise Godot's JSON integer conversion when saving an existing repair.
		screen.changed(); screen.save_review()
		if not await wait_for(screen,app): return
		if not check(screen.last_operation_ok,"Save decisions with repair payload: "+screen.status.text,app): return
		print("ROUTE_REPAIRS_OK accept/reject/undo/save/reopen")
	for fraction in [0.0,0.5,0.99,1.0,0.1]:
		preview.show_distance(preview.summary.length_m*fraction)
		if not check(absf(preview.distance_m-preview.summary.length_m*fraction)<0.01 and preview.state.points.size()<=1024,"Distant seek and bounded local window",app): return
		for p in preview.state.points:
			if not check(absf(p.position[0])<2000 and absf(p.position[2])<2000,"Rebased coordinates stay local",app): return
	preview.play.pressed.emit()
	await app.get_tree().create_timer(0.2).timeout
	if not check(preview.distance_m>preview.summary.length_m*0.1,"Playback advances",app): return
	app.shell.navigate(screen)
	await app.get_tree().process_frame
	if not check(not preview.playing,"Leaving preview stops playback",app): return
	screen.open_preview()
	if not await wait_for(preview,app): return
	if not check(not preview.summary.is_empty() and FileAccess.get_sha256(source_path)==original_hash,"Reopen and original preservation",app): return
	preview.show_distance(preview.summary.length_m*0.5)
	print("FULL_PREVIEW_OK km=",preview.summary.length_m/1000.0," points=",preview.summary.point_count," bridged=",preview.summary.bridged_pauses," retained=",preview.summary.unresolved_breaks," issues=",preview.summary.issues.size())
	if not snapshot.is_empty(): await app._snapshot(snapshot); return
	app.get_tree().quit()
