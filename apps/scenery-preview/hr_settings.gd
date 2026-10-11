extends VBoxContainer
## Optional, explicitly selected ANT source; never auto-pair another broadcaster.
var enabled := CheckBox.new()
var number := SpinBox.new()
var transmission := SpinBox.new()
var key_file := LineEdit.new()
var settings := ConfigFile.new()

func _init() -> void:
	settings.load("user://sensors.cfg")
	enabled.text="Use ANT heart rate";enabled.button_pressed=settings.get_value("hr","enabled",false);add_child(enabled)
	for entry in [["Device number",number,65535],["Transmission type",transmission,255]]:
		var row:=HBoxContainer.new();var label:=Label.new();label.text=entry[0];label.custom_minimum_size.x=150;row.add_child(label)
		entry[1].min_value=1;entry[1].max_value=entry[2];row.add_child(entry[1]);add_child(row)
	number.value=settings.get_value("hr","device_number",1)
	transmission.value=settings.get_value("hr","transmission_type",1)
	key_file.placeholder_text="Path to local ANT network-key file";key_file.tooltip_text="Local text file containing the ANT network key; its contents are never saved in ride recordings."
	key_file.text=settings.get_value("hr","key_file","");key_file.custom_minimum_size.x=420;add_child(key_file)
	var hint:=Label.new();hint.text="Enable HR broadcasting on your watch before connecting.";hint.add_theme_font_size_override("font_size",13);add_child(hint)
	enabled.toggled.connect(update_enabled);update_enabled(enabled.button_pressed)

func update_enabled(on:bool) -> void:
	number.editable=on;transmission.editable=on;key_file.editable=on

func selection() -> String:
	settings.set_value("hr","enabled",enabled.button_pressed)
	settings.set_value("hr","device_number",int(number.value))
	settings.set_value("hr","transmission_type",int(transmission.value))
	settings.set_value("hr","key_file",key_file.text.strip_edges())
	if settings.save("user://sensors.cfg")!=OK:push_warning("Could not persist sensor selection")
	if not enabled.button_pressed:return ""
	return JSON.stringify({"device_number":int(number.value),"transmission_type":int(transmission.value),"key_file":key_file.text.strip_edges()})
