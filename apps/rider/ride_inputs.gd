extends RefCounted
## Device-independent actions. Adapters can emit these actions without owning control.
const BINDINGS = {
	"or_shift_down": [KEY_Q, JOY_BUTTON_LEFT_SHOULDER],
	"or_shift_up": [KEY_E, JOY_BUTTON_RIGHT_SHOULDER],
	"or_pause": [KEY_P, JOY_BUTTON_Y],
	"or_menu": [KEY_ESCAPE, JOY_BUTTON_START],
	"or_choose_left": [KEY_J, JOY_BUTTON_DPAD_LEFT],
	"or_choose_straight": [KEY_K, JOY_BUTTON_DPAD_UP],
	"or_choose_right": [KEY_L, JOY_BUTTON_DPAD_RIGHT],
	"or_confirm": [KEY_ENTER, JOY_BUTTON_A],
	"or_back": [KEY_BACKSPACE, JOY_BUTTON_B],
}
var held := {}

func _init() -> void:
	for action in BINDINGS:
		if not InputMap.has_action(action): InputMap.add_action(action)
		var key := InputEventKey.new(); key.physical_keycode = BINDINGS[action][0]
		var button := InputEventJoypadButton.new(); button.button_index = BINDINGS[action][1]
		for event in [key,button]:
			if not InputMap.action_has_event(action,event): InputMap.action_add_event(action,event)

func decode(event: InputEvent) -> String:
	for action in BINDINGS:
		if not event.is_action(action): continue
		var identity := "%s:%s" % [event.device,action]
		if event.is_action_released(action): held.erase(identity); return ""
		if not event.is_action_pressed(action) or event.is_echo(): return ""
		if held.has(identity): return ""
		held[identity] = true
		return action
	return ""

func disconnected(device: int) -> void:
	for identity in held.keys():
		if str(identity).begins_with(str(device)+":"): held.erase(identity)
