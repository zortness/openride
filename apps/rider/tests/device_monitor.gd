extends SceneTree

func _initialize() -> void:
	run.call_deferred()

func run() -> void:
	var monitor = load("res://device_monitor.gd").new()
	root.add_child(monitor)
	monitor.ingest({"decoded":{"Ok":{"power_w":0,"cadence_rpm":null,"speed_kmh":0,"heart_rate_bpm":null}}})
	if monitor.samples != 1 or not monitor.metrics.text.contains("Power 0 W") or not monitor.metrics.text.contains("Cadence — rpm"):
		push_error("Monitor lost zero/missing distinction")
		quit(1)
		return
	monitor.ingest({"event":"control_acquired"})
	if not monitor.commands.text.contains("acknowledged"):
		quit(1)
		return
	monitor.ingest({"event":"command_acknowledged","opcode":8})
	if not monitor.commands.text.contains("opcode 8"):
		quit(1)
		return
	monitor.ingest({"event":"session_check_failed","message":"timeout"})
	if not monitor.commands.text.contains("unconfirmed"):
		quit(1)
		return
	print("DEVICE_MONITOR_OK")
	quit()
