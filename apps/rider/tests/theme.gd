extends SceneTree

# Run after a cold editor import to verify scene-owned theme inheritance.
func _initialize() -> void:
	verify.call_deferred()

func verify() -> void:
	var scene = load("res://main.tscn").instantiate()
	root.add_child(scene)
	var checkbox := CheckBox.new()
	scene.add_child(checkbox)
	var dialog := Window.new()
	scene.add_child(dialog)
	var dialog_checkbox := CheckBox.new()
	dialog.add_child(dialog_checkbox)
	for control in [checkbox, dialog_checkbox]:
		for icon in ["unchecked", "checked", "unchecked_disabled", "checked_disabled"]:
			var texture = control.get_theme_icon(icon, "CheckBox")
			if texture == null or texture.resource_path != "res://ui/%s.svg" % icon or texture.get_width() <= 0:
				push_error("Custom checkbox texture missing: %s on %s (texture: %s)" % [icon, control.get_path(), texture.resource_path if texture != null else "null"])
				quit(1)
				return
	print("THEME_OK: checkbox textures loaded and inherited in controls and windows")
	quit(0)
