extends SceneTree

func _initialize() -> void:
	var args := OS.get_cmdline_user_args()
	var data: Dictionary = JSON.parse_string(FileAccess.get_file_as_string(args[0]))
	var repair = preload("res://manual_repair.gd")
	var result: Dictionary = repair.make_preview(data,5,31)
	if not check(result.profile.points.size() == 54,"inclusive range"): return
	if not check(result.span.start_source_sample == 981 and result.span.end_source_sample == 1009,"endpoint mapping"): return
	var canonical: Dictionary = JSON.parse_string(FileAccess.get_file_as_string(args[1])).reviewed_profile
	for index in range(canonical.points.size()):
		var a: Dictionary = canonical.points[index]
		var b: Dictionary = result.profile.points[index]
		if not check(a.source_sample == b.source_sample and a.position == b.position,"position parity"): return
		if a.grade_percent != null and not check(absf(a.grade_percent-b.grade_percent)<0.001,"grade parity"): return
	if not check(repair.make_preview(data,1,31).has("error"),"first endpoint protected"): return
	if not check(repair.make_preview(data,5,81).has("error"),"last endpoint protected"): return
	if not check(repair.make_preview(data,31,5).has("error"),"reversed range rejected"): return
	var wider: Dictionary = repair.make_preview(data,5,32)
	if not check(wider.profile.points.size() == 53 and wider.profile.findings.is_empty(),"wider range continuity"): return
	print("MANUAL_REPAIR_PARITY_OK points=54; range guards and 5-32 alternative passed")
	quit(0)

func check(condition: bool, message: String) -> bool:
	if not condition:
		push_error(message)
		quit(1)
	return condition
