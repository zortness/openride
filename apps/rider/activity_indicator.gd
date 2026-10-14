extends Control
## An indeterminate indicator: motion means a job is pending, not bytes received.
var phase := 0.0
var active := false

func _init() -> void:
	custom_minimum_size = Vector2(24, 24)
	size_flags_vertical = Control.SIZE_SHRINK_CENTER
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	set_process(false)

func set_active(value: bool) -> void:
	active = value
	set_process(active)
	queue_redraw()

func _process(delta: float) -> void:
	phase = fmod(phase + delta * TAU * 0.8, TAU)
	queue_redraw()

func _draw() -> void:
	if not active: return
	var center := size * 0.5
	draw_arc(center, 8.0, 0.0, TAU, 40, Color("29454d"), 2.5, true)
	draw_arc(center, 8.0, phase, phase + TAU * 0.7, 32, Color("71d6c2"), 2.5, true)
