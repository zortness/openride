extends SceneTree
func _initialize() -> void:
	run.call_deferred()
func run() -> void:
	var runtime=load("res://scenery/stream_preview.gd").new()
	# Exercise the public pacing selector and lifecycle restoration without loading scenery.
	var initial_fps:=Engine.max_fps
	var id:=root.get_window_id()
	var initial_vsync:=DisplayServer.window_get_vsync_mode(id)
	runtime.saved_max_fps=initial_fps
	runtime.saved_vsync={id:initial_vsync}
	runtime.set_frame_pacing(0)
	var cap_ok:bool=Engine.max_fps==60 and DisplayServer.window_get_vsync_mode(id)==DisplayServer.VSYNC_DISABLED
	runtime.set_frame_pacing(1)
	var sync_ok:bool=Engine.max_fps==0 and DisplayServer.window_get_vsync_mode(id)==DisplayServer.VSYNC_ENABLED
	runtime._exit_tree()
	var restore_ok:bool=Engine.max_fps==initial_fps and DisplayServer.window_get_vsync_mode(id)==initial_vsync
	runtime.free()
	print("FRAME_PACING_TESTS cap=",cap_ok," vsync=",sync_ok," restore=",restore_ok)
	quit(0 if cap_ok and sync_ok and restore_ok else 1)
