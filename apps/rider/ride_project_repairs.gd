extends VBoxContainer
const Plot = preload("res://plot.gd")
var owner_screen: Control
var report: Dictionary = {}
var listing: ItemList
var summary: Label
var detail: Label
var before: Control
var after: Control
var accept: Button
var reject: Button
var undo: Button
var selected_id := ""

func setup(screen: Control) -> void:
	owner_screen = screen; add_theme_constant_override("separation", 10)
	summary = screen.text(self, "Repair", 22)
	screen.text(self, "Close short recording gaps or remove tiny accidental loops. Review each proposal; original samples and heights stay intact.", 15)
	listing = ItemList.new(); listing.custom_minimum_size.y = 150; add_child(listing); listing.item_selected.connect(select_proposal)
	detail = screen.text(self, "Choose a proposal to compare before and after.", 15)
	var row := HBoxContainer.new(); add_child(row)
	for caption in ["Original", "Proposed"]:
		var column := VBoxContainer.new(); column.size_flags_horizontal = Control.SIZE_EXPAND_FILL; row.add_child(column)
		screen.text(column, caption, 14)
		var plot := Plot.new(); plot.custom_minimum_size = Vector2(240, 170); column.add_child(plot)
		if caption == "Original": before = plot
		else: after = plot
	row = HBoxContainer.new(); add_child(row)
	accept = screen.add_button(row, "Accept repair", func(): decide("accepted"))
	reject = screen.add_button(row, "Keep original / reject", func(): decide("rejected"))
	undo = screen.add_button(row, "Undo decision", func(): decide("pending"))
	hide()

func populate() -> void:
	report = owner_screen.snapshot.get("repair_preview", {})
	visible = not report.is_empty()
	if report.is_empty(): return
	summary.text = "Repair · %.3f → %.3f km · %d unresolved breaks" % [report.before_length_m / 1000.0, report.length_m / 1000.0, report.unresolved_breaks]
	listing.clear()
	var selected := 0
	for i in range(report.candidates.size()):
		var c: Dictionary = report.candidates[i]
		listing.add_item("%s · samples %d–%d · %s%s" % ["Gap" if c.decision.kind == "gap" else "Tiny loop", c.decision.first, c.decision.last, c.decision.status, " · needs course editing" if not c.allowed else ""])
		if c.id == selected_id: selected = i
	if listing.item_count > 0: listing.select(selected); select_proposal(selected)
	else:
		selected_id = ""; detail.text = "No gap or tiny-loop proposals for this source."
		for plot in [before, after]: plot.configure([], 0, 0, 0)

func select_proposal(index: int) -> void:
	var c: Dictionary = report.candidates[index]; selected_id = c.id
	detail.text = "%s\nProposed distance change: %+.2f m · saved decision: %s" % [c.reason, c.distance_change_m, c.decision.status]
	before.configure(c.before, 0, c.before.size() - 1, 0)
	after.configure(c.after, 0, c.after.size() - 1, 0)
	refresh()

func refresh() -> void:
	var disabled: bool = owner_screen.busy or selected_id.is_empty()
	accept.disabled = disabled; reject.disabled = disabled; undo.disabled = disabled
	if not disabled:
		for c in report.candidates:
			if c.id == selected_id: accept.disabled = not c.allowed or c.decision.status == "accepted"

func decide(status: String) -> void:
	if selected_id.is_empty(): return
	owner_screen.run_job("repair", owner_screen.project_path, PackedStringArray(["--repair", selected_id, "--status", status]))
