extends Control

signal sample_picked(index: int)
var samples: Array = []
var profile := false
var selection_start := 0
var selection_end := 0
var cursor := 0
var projected: Array = []
const TEAL := Color("56d6bd")

func _ready() -> void:
	mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	resized.connect(queue_redraw)

func configure(data: Array, first: int, last: int, selected: int) -> void:
	samples = data
	selection_start = first
	selection_end = last
	cursor = selected
	queue_redraw()

func _draw() -> void:
	var rect := Rect2(Vector2(1, 1), size - Vector2(2, 2))
	draw_style_box(_background(), rect)
	projected.clear()
	if samples.is_empty():
		return
	var coords: Array = []
	var minimum := Vector2(INF, INF)
	var maximum := Vector2(-INF, -INF)
	var reference_lat := 0.0
	var reference_lon := 0.0
	for s in samples:
		if s.get("latitude") != null:
			reference_lat = float(s.latitude)
			reference_lon = float(s.longitude)
			break
	for s in samples:
		var p = null
		if profile and s.get("elevation_m") != null:
			p = Vector2(float(s.route_distance_m), -float(s.elevation_m))
		elif not profile and s.get("latitude") != null and s.get("longitude") != null:
			p = Vector2((float(s.longitude) - reference_lon) * cos(deg_to_rad(reference_lat)) * 111195.0, -(float(s.latitude) - reference_lat) * 111195.0)
		coords.append(p)
		if p != null:
			minimum = minimum.min(p)
			maximum = maximum.max(p)
	if minimum.x == INF:
		return
	var extent := maximum - minimum
	var area := size - Vector2(64, 52)
	var scale_xy := Vector2(area.x / maxf(extent.x, 0.00001), area.y / maxf(extent.y, 0.00001))
	if not profile:
		scale_xy = Vector2.ONE * minf(scale_xy.x, scale_xy.y)
	var offset := (size - extent * scale_xy) * 0.5
	for p in coords:
		projected.append(null if p == null else (p - minimum) * scale_xy + offset)
	for line in range(1, 5):
		var y := 18.0 + (size.y - 36.0) * line / 5.0
		draw_line(Vector2(18, y), Vector2(size.x - 18, y), Color(0.18, 0.25, 0.28, 0.4))
	for i in range(1, projected.size()):
		if projected[i] == null or projected[i - 1] == null:
			continue
		if samples[i].get("break_before", false):
			draw_circle(projected[i], 3, Color("f4bc73"))
			continue
		var active := i > selection_start and i <= selection_end
		draw_line(projected[i-1], projected[i], TEAL if active else Color("465c65"), 2.5 if active else 1.5, true)
	if cursor < projected.size() and projected[cursor] != null:
		draw_circle(projected[cursor], 6, Color.WHITE)
		draw_circle(projected[cursor], 3, Color("163d38"))
	var font := ThemeDB.fallback_font
	if profile:
		draw_string(font, Vector2(16, 18), "%.0f m" % -minimum.y, HORIZONTAL_ALIGNMENT_LEFT, -1, 13, Color("9badb5"))
		draw_string(font, Vector2(16, size.y-10), "%.0f m" % -maximum.y, HORIZONTAL_ALIGNMENT_LEFT, -1, 13, Color("9badb5"))
		draw_string(font, Vector2(size.x-130, size.y-10), "%.2f km" % (maximum.x/1000.0), HORIZONTAL_ALIGNMENT_LEFT, -1, 13, Color("9badb5"))
	else:
		draw_string(font, Vector2(18, 24), "N ↑   LOCAL TRACK VIEW", HORIZONTAL_ALIGNMENT_LEFT, -1, 13, Color("9badb5"))

func _background() -> StyleBoxFlat:
	var style := StyleBoxFlat.new()
	style.bg_color = Color("101e27")
	style.set_corner_radius_all(8)
	return style

func _gui_input(event: InputEvent) -> void:
	if event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_LEFT:
		var nearest := -1
		var distance := INF
		for i in range(projected.size()):
			if projected[i] != null:
				var d: float = projected[i].distance_squared_to(event.position)
				if d < distance:
					distance = d
					nearest = i
		if nearest >= 0:
			sample_picked.emit(nearest)
