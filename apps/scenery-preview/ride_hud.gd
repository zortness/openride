extends PanelContainer
var values: Array[Label] = []
var secondary := Label.new()
var scale_setting := 1.0
var settings := ConfigFile.new()

func _init() -> void:
	settings.load("user://riding-ui.cfg")
	scale_setting=clampf(float(settings.get_value("hud","scale",1.0)),0.75,1.5)
	var style:=StyleBoxFlat.new();style.bg_color=Color("10232cde")
	style.content_margin_left=18;style.content_margin_right=18;style.content_margin_top=8;style.content_margin_bottom=8
	style.corner_radius_top_left=8;style.corner_radius_top_right=8
	add_theme_stylebox_override("panel",style)
	var column:=VBoxContainer.new();add_child(column)
	var row:=HBoxContainer.new();row.add_theme_constant_override("separation",26);column.add_child(row)
	for title in ["POWER · W","CADENCE · rpm","HEART RATE · bpm","SPEED · km/h"]:
		var cell:=VBoxContainer.new();cell.size_flags_horizontal=Control.SIZE_EXPAND_FILL;row.add_child(cell)
		var value:=Label.new();value.text="--";values.append(value);cell.add_child(value)
		var caption:=Label.new();caption.text=title;caption.add_theme_font_size_override("font_size",14);caption.modulate=Color("bdd0d7");cell.add_child(caption)
	column.add_child(secondary);secondary.add_theme_font_size_override("font_size",20)
	apply_scale(scale_setting)

func apply_scale(value:float) -> void:
	scale_setting=value
	for label in values:label.add_theme_font_size_override("font_size",int(52*value))
	secondary.add_theme_font_size_override("font_size",int(20*value))
	settings.set_value("hud","scale",value);settings.save("user://riding-ui.cfg")

func display(power:Variant,cadence:Variant,hr:Variant,speed:float,seconds:float,remaining:float,grade:float) -> void:
	var readings:Array=[power,cadence,hr,speed]
	for i in range(4):values[i].text="--" if readings[i]==null else (("%.1f" if i==3 else "%.0f") % float(readings[i]))
	secondary.text="%02d:%02d elapsed     %.2f km remaining     %+.1f%% grade" % [int(seconds)/60,int(seconds)%60,maxf(remaining,0)/1000.0,grade]
