extends SceneTree

func _initialize() -> void:
	run.call_deferred()

func run() -> void:
	var app = load("res://main.tscn").instantiate()
	root.add_child(app)
	await process_frame
	assert(app.library.groups.size() == 1, "Expected paired RPV imports")
	assert(app.library.groups[0].sources.size() == 2)
	assert(app.activity.format == "TCX")
	assert(app.activity.samples.size() == 1014)
	assert(app.metrics.text.contains("12.540"))
	app.start_sample.value = 100
	app.end_sample.value = 800
	app.cursor.value = 400
	assert(app.map_view.selection_start == 100)
	assert(app.elevation.selection_end == 800)
	assert(app.details.text.begins_with("#400"))
	app.sources.select(0)
	app._select_source(0)
	assert(app.activity.format == "GPX")
	assert(app.activity.samples.size() == 1013)
	print("PREVIEW_OK: paired-source selection, distance labels, subsection controls, sample details")
	quit()
