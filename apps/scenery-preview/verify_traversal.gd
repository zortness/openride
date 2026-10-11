extends SceneTree

func _initialize() -> void:
	var profile := {"points":[
		{"position":[0,0,0],"distance_m":0,"elevation_m":100,"break_before":true,"grade_percent":null},
		{"position":[10,1,0],"distance_m":10,"elevation_m":101,"break_before":false,"grade_percent":10},
		{"position":[10,0,10],"distance_m":20,"elevation_m":100,"break_before":false,"grade_percent":-10}]}
	var traversal = preload("res://route_traversal.gd").new()
	if not check(traversal.configure(profile).is_empty(),"configure"): return
	var sample: Dictionary = traversal.seek(5)
	if not check(sample.position.is_equal_approx(Vector3(5,0.5,0)) and sample.grade_percent==10 and sample.elevation_m==100.5,"interpolation"): return
	traversal.playing = true
	traversal.advance(1,36)
	if not check(traversal.distance_m==15 and traversal.seek(15).grade_percent==-10,"advance and grade change"): return
	traversal.playing = false
	traversal.advance(5,36)
	if not check(traversal.distance_m==15,"pause"): return
	traversal.playing = true
	traversal.advance(10,36)
	if not check(traversal.distance_m==20 and not traversal.playing,"stop at finish"): return
	if not check(traversal.seek(-10).distance_m==0,"seek clamp"): return
	for key in ["break_before","distance_m","grade_percent"]:
		var broken := profile.duplicate(true)
		broken.points[1][key] = true if key=="break_before" else 0
		if not check(not traversal.configure(broken).is_empty(),"reject invalid "+key): return
	print("ROAD_TRAVERSAL_TESTS_OK checks=9")
	quit(0)

func check(condition: bool, message: String) -> bool:
	if not condition:
		push_error(message)
		quit(1)
	return condition
