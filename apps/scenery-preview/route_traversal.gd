extends RefCounted
## Deterministic distance-based preview; no device I/O or rider physics.
var points: Array = []
var distance_m := 0.0
var length_m := 0.0
var playing := false

func configure(profile: Dictionary) -> String:
	points = []
	playing = false
	distance_m = 0.0
	length_m = 0.0
	var candidate: Variant = profile.get("points")
	if not candidate is Array or candidate.size() < 2 or candidate.size() > 10000:
		return "Route traversal requires 2–10,000 profile points."
	for index in range(candidate.size()):
		var point: Variant = candidate[index]
		if not point is Dictionary or not point.get("position") is Array or point.position.size() != 3:
			return "Invalid traversal position."
		for value in point.position:
			if not (value is float or value is int) or not is_finite(float(value)):
				return "Missing or invalid traversal coordinates."
		for key in ["distance_m", "elevation_m"]:
			var value: Variant = point.get(key)
			if not (value is float or value is int) or not is_finite(float(value)):
				return "Missing or invalid traversal height/distance."
		if index == 0:
			if absf(point.distance_m)>0.001: return "Route must start at zero distance."
			continue
		var previous: Dictionary = candidate[index-1]
		var step: float = point.distance_m-previous.distance_m
		if point.get("break_before", true) or step <= 0:
			return "Traversal needs a continuous profile with increasing distance."
		var horizontal := Vector2(point.position[0]-previous.position[0],point.position[2]-previous.position[2]).length()
		if absf(horizontal-step)>0.01:
			return "Profile distance does not match road geometry."
		var grade: Variant = point.get("grade_percent")
		if not (grade is float or grade is int) or not is_finite(float(grade)) or absf(grade-100.0*(point.position[1]-previous.position[1])/step)>0.01:
			return "Profile grade does not match road geometry."
		if absf((point.elevation_m-previous.elevation_m)-(point.position[1]-previous.position[1]))>0.01:
			return "Profile height does not match road geometry."
	points = candidate
	length_m = points[-1].distance_m
	return ""

func seek(value: float) -> Dictionary:
	if points.is_empty(): return {}
	distance_m = clampf(value,0,length_m)
	var low := 1
	var high := points.size()-1
	while low<high:
		var mid := (low+high)/2
		if points[mid].distance_m < distance_m: low=mid+1
		else: high=mid
	var a: Dictionary = points[low-1]
	var b: Dictionary = points[low]
	var f: float = (distance_m-a.distance_m)/(b.distance_m-a.distance_m)
	var position := Vector3(a.position[0],a.position[1],a.position[2]).lerp(Vector3(b.position[0],b.position[1],b.position[2]),f)
	return {"position":position,"elevation_m":lerpf(a.elevation_m,b.elevation_m,f),"grade_percent":b.grade_percent,"distance_m":distance_m}

func advance(delta: float, speed_kph: float) -> Dictionary:
	if playing:
		seek(distance_m+maxf(delta,0)*maxf(speed_kph,0)/3.6)
		if distance_m>=length_m: playing=false
	return seek(distance_m)
