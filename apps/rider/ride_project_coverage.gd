extends Control
signal position_selected(latitude: float, longitude: float)
var plan: Dictionary = {}
var samples: Array = []
var roads: Array = []
var highlight_roads: Array = []
var focus_geometry: Array = []
var road_labels: Array = []
var selected_road_label := ""
var focus_padding := 0.001
var map_lo := Vector2.ZERO
var map_offset := Vector2.ZERO
var map_scale := 1.0
var longitude_scale := 1.0
var imagery_texture: Texture2D
var imagery_bounds: Array = []

func _ready() -> void:
	custom_minimum_size = Vector2(240, 230); clip_contents = true; resized.connect(queue_redraw)

func _gui_input(event: InputEvent) -> void:
	if event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_LEFT and not plan.is_empty():
		var p: Vector2 = (event.position - map_offset) / map_scale + map_lo
		position_selected.emit(-p.y, p.x / longitude_scale)

func _draw() -> void:
	draw_rect(Rect2(Vector2.ZERO, size), Color("10232a"))
	if plan.is_empty(): return
	longitude_scale = cos(deg_to_rad(float(plan.origin[0])))
	var lo := Vector2(INF, INF); var hi := Vector2(-INF, -INF)
	for tile in plan.tiles:
		lo = lo.min(Vector2(tile.bounds[0] * longitude_scale, -tile.bounds[3])); hi = hi.max(Vector2(tile.bounds[2] * longitude_scale, -tile.bounds[1]))
	if not focus_geometry.is_empty():
		lo = Vector2(INF, INF); hi = Vector2(-INF, -INF)
		for p in focus_geometry:
			var point := Vector2(p[1] * longitude_scale, -p[0]); lo = lo.min(point); hi = hi.max(point)
		lo -= Vector2.ONE * focus_padding; hi += Vector2.ONE * focus_padding
	var span := hi - lo
	var scale := minf((size.x - 24) / maxf(span.x, 0.00001), (size.y - 24) / maxf(span.y, 0.00001))
	map_lo = lo; map_scale = scale; map_offset = (size - span * scale) * 0.5
	var project := func(lon: float, lat: float): return (Vector2(lon * longitude_scale, -lat) - lo) * scale + map_offset
	if imagery_texture != null and imagery_bounds.size() == 4:
		var a: Vector2 = project.call(imagery_bounds[0], imagery_bounds[3]); var b: Vector2 = project.call(imagery_bounds[2], imagery_bounds[1])
		draw_texture_rect(imagery_texture, Rect2(a, b - a), false)
	for tile in plan.tiles:
		var a: Vector2 = project.call(tile.bounds[0], tile.bounds[3]); var b: Vector2 = project.call(tile.bounds[2], tile.bounds[1])
		draw_rect(Rect2(a, b - a), Color("42606b"), false)
	for road in roads:
		for i in range(1, road.size()):
			draw_line(project.call(road[i-1][1], road[i-1][0]), project.call(road[i][1], road[i][0]), Color("8597ac"), 1)
	for label in road_labels:
		var geometry:Array=label.geometry
		for i in range(1,geometry.size()):
			draw_line(project.call(geometry[i-1][1],geometry[i-1][0]),project.call(geometry[i][1],geometry[i][0]),Color("8597ac"),2)
	for road in highlight_roads:
		for i in range(1,road.size()):draw_line(project.call(road[i-1][1],road[i-1][0]),project.call(road[i][1],road[i][0]),Color("ffc66d"),3)
	var previous = null
	for sample in samples:
		if sample.latitude == null or sample.longitude == null: previous = null; continue
		var point: Vector2 = project.call(sample.longitude, sample.latitude)
		if previous != null and not sample.break_before: draw_line(previous, point, Color("56d6bd"), 2)
		previous = point
	if not focus_geometry.is_empty():
		var endpoint = focus_geometry[-1]
		draw_circle(project.call(endpoint[1], endpoint[0]), 5, Color("ffc66d"))
	draw_road_labels(project)

func draw_road_labels(project:Callable) -> void:
	var font:=ThemeDB.fallback_font
	var occupied:Array[Rect2]=[]
	var placements:Array[Dictionary]=[]
	var viewport:=Rect2(Vector2(8,8),size-Vector2(16,16))
	for label in road_labels:
		var geometry:Array=label.geometry
		var anchor:=Vector2(INF,INF)
		var best:=INF
		for i in range(1,geometry.size()):
			var a:Vector2=project.call(geometry[i-1][1],geometry[i-1][0])
			var b:Vector2=project.call(geometry[i][1],geometry[i][0])
			var point:=Geometry2D.get_closest_point_to_segment(size*0.5,a,b)
			var distance:=point.distance_squared_to(size*0.5)
			if viewport.has_point(point) and distance<best:anchor=point;best=distance
		if not is_finite(best):continue
		var caption:String=label.text
		var width:=minf(font.get_string_size(caption,HORIZONTAL_ALIGNMENT_LEFT,-1,14).x+12,size.x-16)
		for offset in range(16):
			var position:=anchor+Vector2(10, -28+offset*25)
			position.x=clampf(position.x,8,maxf(8,size.x-width-8))
			position.y=clampf(position.y,8,maxf(8,size.y-30))
			var rect:=Rect2(position,Vector2(width,23))
			if occupied.any(func(other):return other.intersects(rect)):continue
			occupied.append(rect)
			draw_line(anchor,rect.get_center(),Color("8597ac"),1,true)
			draw_circle(anchor,3,Color("c9d7df"))
			placements.append({"rect":rect,"caption":caption})
			break
	# Draw text after all leader lines so adjacent roads cannot obscure labels.
	for placement in placements:
		var rect:Rect2=placement.rect
		draw_rect(rect,Color("08151e"))
		var color:=Color("ffc66d") if placement.caption==selected_road_label else Color("eef6fa")
		draw_string(font,rect.position+Vector2(6,17),placement.caption,HORIZONTAL_ALIGNMENT_LEFT,rect.size.x-12,14,color)
