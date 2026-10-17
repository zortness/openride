extends Button
var last_mode := DisplayServer.WINDOW_MODE_WINDOWED

func _ready() -> void:
	text = ""
	tooltip_text = "Toggle window / fullscreen"
	custom_minimum_size = Vector2(46, 46)
	pressed.connect(toggle_window_mode)
	last_mode = DisplayServer.window_get_mode()

func _process(_delta: float) -> void:
	var mode := DisplayServer.window_get_mode()
	if mode != last_mode:
		last_mode = mode
		queue_redraw()

func toggle_window_mode() -> void:
	var full := DisplayServer.window_get_mode() == DisplayServer.WINDOW_MODE_FULLSCREEN
	DisplayServer.window_set_mode(DisplayServer.WINDOW_MODE_WINDOWED if full else DisplayServer.WINDOW_MODE_FULLSCREEN)
	queue_redraw()

func _draw() -> void:
	var ink := Color("dcecef")
	var center := size / 2.0
	var box := Rect2(center - Vector2(10, 8), Vector2(20, 16))
	if DisplayServer.window_get_mode() == DisplayServer.WINDOW_MODE_FULLSCREEN:
		draw_rect(box, ink, false, 2.0)
		draw_line(box.position + Vector2(4, 5), box.position + Vector2(9, 5), ink, 2.0)
	else:
		for x in [0, 1]:
			for y in [0, 1]:
				var corner := box.position + Vector2(box.size.x * x, box.size.y * y)
				var inward := Vector2(1 if x == 0 else -1, 1 if y == 0 else -1)
				draw_line(corner, corner + Vector2(inward.x * 6.0, 0), ink, 2.0)
				draw_line(corner, corner + Vector2(0, inward.y * 6.0), ink, 2.0)
