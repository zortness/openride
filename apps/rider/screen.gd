extends Control
## Screens retain their model and background jobs while navigation changes.
signal open_requested(screen: Control)
signal close_requested
var title := ""
var min_size := Vector2i.ZERO

func popup_centered(_size := Vector2i.ZERO) -> void:
	open_requested.emit(self)

