extends SceneTree

func _initialize() -> void:
	var args:=OS.get_cmdline_user_args()
	if args.size()!=1: quit(1);return
	var raw:=FileAccess.get_file_as_string(args[0])
	var profile:Variant=JSON.parse_string(raw)
	var reader=preload("res://route_traversal.gd").new()
	if not profile is Dictionary or not reader.configure(profile).is_empty():
		push_error("Valid continuous corridor rejected");quit(1);return
	for kind in ["position","station","geography","projection","missing_geography"]:
		var changed:Dictionary=profile.duplicate(true)
		match kind:
			"position": changed.points[1].position[0]+=1.0
			"station": changed.points[1].distance_m+=1.0
			"geography": changed.points[1].latitude+=0.001
			"projection": changed.projection.radius_m=1
			"missing_geography": changed.points[1].erase("latitude")
		if reader.configure(changed).is_empty():
			push_error("Corrupt corridor accepted: "+kind);quit(1);return
	print("CORRIDOR_ROUTE_VALIDATION_OK geography, projection and station corruption rejected")
	quit(0)
