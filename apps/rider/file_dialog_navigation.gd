extends Node

# Input belongs to the dialog's viewport, including dialogs inside the 3D editor.
func _input(event: InputEvent) -> void:
	var dialog := get_parent() as FileDialog
	if not dialog.visible: return
	if event is InputEventJoypadButton and event.pressed and event.button_index in [JOY_BUTTON_LEFT_SHOULDER,JOY_BUTTON_RIGHT_SHOULDER]:
		var owner := dialog.gui_get_focus_owner()
		var target: Control
		if owner != null:
			target = owner.find_prev_valid_focus() if event.button_index==JOY_BUTTON_LEFT_SHOULDER else owner.find_next_valid_focus()
		if target != null: target.grab_focus()
		else: dialog.get_cancel_button().grab_focus()
		dialog.set_input_as_handled()
