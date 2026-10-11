extends SceneTree

func _initialize() -> void:
	var args := OS.get_cmdline_user_args()
	if args.is_empty(): quit(1); return
	var reader = preload("res://package_library.gd")
	if not check(not reader.validate(args[0]).has("error"),"Valid package rejected"): return
	var directory := "user://package-validation"
	DirAccess.make_dir_recursive_absolute(directory)
	for name in ["preparation.json","attribution.json"]:
		if DirAccess.copy_absolute(args[0].path_join(name),directory.path_join(name)) != OK:
			quit(1); return
	var original: Dictionary = reader.read_json(args[0].path_join("manifest.json"),65536)
	var mutations := [
		{"schema_version":99},
		{"required_capabilities":["unknown"]},
		{"dependencies":["missing-world"]},
		{"files":[{"path":"../escape"},{}]},
		{"trainer_ready":true}
	]
	for mutation in mutations:
		var modified := original.duplicate(true)
		modified.merge(mutation,true)
		write_json(directory.path_join("manifest.json"),modified)
		if not check(reader.validate(directory).has("error"),"Invalid manifest accepted"): return
	write_json(directory.path_join("manifest.json"),original)
	var file := FileAccess.open(directory.path_join("preparation.json"),FileAccess.READ_WRITE)
	file.store_8(32)
	file.close()
	if not check(reader.validate(directory).get("error","").begins_with("Checksum mismatch"),"Tampered payload accepted"): return
	DirAccess.remove_absolute(directory.path_join("manifest.json"))
	if not check(reader.validate(directory).has("error"),"Incomplete copy accepted"): return
	print("PACKAGE_VALIDATION_OK checks=8")
	quit(0)

func write_json(path: String, value: Dictionary) -> void:
	var file := FileAccess.open(path,FileAccess.WRITE)
	file.store_string(JSON.stringify(value))
	file.close()

func check(condition: bool, message: String) -> bool:
	if not condition:
		push_error(message)
		quit(1)
	return condition
