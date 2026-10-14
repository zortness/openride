extends VBoxContainer
var owner_screen: Control
var summary: Label
var build_button: Button
var preview_button: Button
var export_button: Button
var steep: CheckButton
var readiness: Label

func setup(screen: Control) -> void:
	owner_screen = screen; add_theme_constant_override("separation", 10)
	screen.text(self, "Build preview", 22)
	summary = screen.text(self, "Review the course and acquire terrain, then build its 3D preview.", 15)
	readiness = screen.text(self, "", 16)
	steep = CheckButton.new(); steep.text = "Retain inspected grades above 15% in this preview"; add_child(steep)
	var row := HBoxContainer.new(); add_child(row)
	build_button = screen.add_button(row, "Build 3D preview", begin_build)
	preview_button = screen.add_button(row, "Open built preview", func(): screen.preview_requested.emit(screen.project_path.path_join(screen.snapshot.build.path)))
	export_button = screen.add_button(row, "Export ride ZIP", func(): screen.run_job("export-package", screen.project_path))
	screen.text(self, "The last usable build stays available while a new version is prepared. Cached imagery streams with nearby terrain; saved buildings, vegetation and water are included. Review inferred heights and source warnings before sharing.", 14)

func begin_build() -> void:
	if owner_screen.busy or build_button.disabled:return
	var extra:=PackedStringArray(["--accept-steep", "true" if steep.button_pressed else "false"])
	var plan:Dictionary=owner_screen.snapshot.get("maps",{})
	if int(plan.get("terrain_ready",0))<plan.get("tiles",[]).size():
		owner_screen.job_queue.clear()
		owner_screen.job_queue.append({"action":"build-package","extra":extra})
		owner_screen.run_job("acquire",owner_screen.project_path,PackedStringArray(["--layer","terrain"]))
	else:owner_screen.run_job("build-package",owner_screen.project_path,extra)

func populate() -> void:
	var built: Dictionary = owner_screen.snapshot.get("build", {})
	if built.is_empty(): summary.text = "Review the course and acquire terrain, then build its 3D preview."; return
	var current: bool = built.get("current",false)
	summary.text = "%s · %.2f km · %d streamed chunks · %d reused assets" % ["Current saved build" if current else "Previous saved build — rebuild after reviewing changes", built.length_m / 1000.0, built.chunks, built.reused_chunks]
	summary.text+="\nImagery on %d chunks · %d chunks use some terrain shading · %d scenery warnings" % [built.get("imagery_chunks",0),built.get("imagery_gaps",0),built.get("warnings",[]).size()]
	for warning in built.get("warnings",[]).slice(0,5):summary.text+="\n"+str(warning)
	if built.get("export") is String: summary.text += "\nZIP: " + owner_screen.project_path.path_join(built.export)

func refresh() -> void:
	var snapshot: Dictionary = owner_screen.snapshot
	var plan: Dictionary = snapshot.get("maps", {})
	var remaining:int=maxi(0,plan.get("tiles",[]).size()-int(plan.get("terrain_ready",0)))
	var blockers:PackedStringArray=[]
	if not snapshot.has("course"):blockers.append("Go to Roads and infer the course first.")
	elif int(snapshot.course.get("unresolved",1))>0:blockers.append("Review %d remaining stretches on the Roads tab."%int(snapshot.course.unresolved))
	if plan.is_empty():blockers.append("Plan map coverage on the Roads tab first.")
	if not snapshot.get("scenery",{}).get("conflicts",[]).is_empty():blockers.append("Resolve changed scenery edits on Scenery & share first.")
	build_button.disabled=owner_screen.busy or not blockers.is_empty()
	build_button.text="Download topography & build preview" if remaining>0 else "Build 3D preview"
	if not blockers.is_empty():readiness.text="\n".join(blockers)
	elif remaining>0:readiness.text="Road review complete. Topography: %d/%d areas ready. Build will download the remaining %d areas, then generate the preview automatically. Downloads can be cancelled and resumed."%[int(plan.get("terrain_ready",0)),plan.get("tiles",[]).size(),remaining]
	else:readiness.text="Road review and topography complete. Ready to build the 3D preview."
	preview_button.disabled = owner_screen.busy or not snapshot.has("build")
	export_button.disabled = preview_button.disabled
	steep.disabled = owner_screen.busy
