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
	var projection: Variant = profile.get("projection")
	if projection!=null:
		if not projection is Dictionary or projection.get("kind")!="spherical_orthographic" or projection.get("radius_m")!=6371000 or not projection.get("origin") is Array or projection.origin.size()!=3:
			return "Unsupported route projection."
		for value in projection.origin:
			if not (value is float or value is int) or not is_finite(value): return "Invalid projection origin."
		if absf(projection.origin[0])>=80 or absf(projection.origin[1])>180: return "Projection origin outside supported bounds."
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
		if projection!=null:
			for key in ["latitude","longitude"]:
				var value: Variant=point.get(key)
				if not (value is float or value is int) or not is_finite(value): return "Missing route geography."
			if absf(point.latitude)>=80 or absf(point.longitude)>180: return "Invalid route geography."
			var latitude:float=deg_to_rad(point.latitude)
			var origin_lat:float=deg_to_rad(projection.origin[0])
			var delta_lon:float=deg_to_rad(point.longitude-projection.origin[1])
			if sin(origin_lat)*sin(latitude)+cos(origin_lat)*cos(latitude)*cos(delta_lon)<=0:
				return "Route lies behind projection hemisphere."
			var x:float=6371000*cos(latitude)*sin(delta_lon)
			var z:float=-6371000*(cos(origin_lat)*sin(latitude)-sin(origin_lat)*cos(latitude)*cos(delta_lon))
			if sqrt(x*x+z*z)>99000 or absf(point.position[0]-x)>0.001 or absf(point.position[2]-z)>0.001 or absf(point.position[1]-(point.elevation_m-projection.origin[2]))>0.001:
				return "Route position does not match declared projection."
		if index == 0:
			if absf(point.distance_m)>0.001: return "Route must start at zero distance."
			continue
		var previous: Dictionary = candidate[index-1]
		var step: float = point.distance_m-previous.distance_m
		if point.get("break_before", true) or step <= 0:
			return "Traversal needs a continuous profile with increasing distance."
		var horizontal := Vector2(point.position[0]-previous.position[0],point.position[2]-previous.position[2]).length()
		if projection!=null:
			# Stationing stays geodesic; a projected chord is only display geometry.
			var lat_a:float=deg_to_rad(previous.latitude)
			var lat_b:float=deg_to_rad(point.latitude)
			var dl:float=deg_to_rad(point.longitude-previous.longitude)
			var hav:float=pow(sin((lat_b-lat_a)/2),2)+cos(lat_a)*cos(lat_b)*pow(sin(dl/2),2)
			horizontal=2*6371000*asin(sqrt(clampf(hav,0,1)))
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
