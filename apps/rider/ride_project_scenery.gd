extends VBoxContainer
var owner_screen: Control
var summary: Label
var search: LineEdit
var listing: ItemList
var view: Control
var kinds: OptionButton
var rings: OptionButton
var vertices: ItemList
var height: SpinBox
var density: SpinBox
var trees: CheckButton
var elevation: LineEdit
var draft: Dictionary = {}
var active_vertex := -1
var selected_id := ""
var features: Array = []
var actions: Array[Button] = []

func setup(screen: Control) -> void:
	owner_screen=screen;add_theme_constant_override("separation",10)
	screen.text(self,"Scenery",22)
	summary=screen.text(self,"Acquire scenery, then select a feature or draw a new polygon. Changes stay in this project.",15)
	var downloads:=HBoxContainer.new();add_child(downloads)
	for layer in ["buildings","vegetation","water"]:
		actions.append(screen.add_button(downloads,"Acquire / resume "+layer,func():screen.run_job("acquire",screen.project_path,PackedStringArray(["--layer",layer]))))
	search=LineEdit.new();search.placeholder_text="Filter by feature identity or kind";add_child(search);search.text_changed.connect(func(_v):show_features())
	listing=ItemList.new();listing.custom_minimum_size.y=100;add_child(listing);listing.item_selected.connect(select_feature)
	var row:=HBoxContainer.new();add_child(row)
	kinds=OptionButton.new();row.add_child(kinds)
	for kind in ["building","vegetation","water","exclusion"]:kinds.add_item(kind)
	actions.append(screen.add_button(row,"New polygon",new_polygon))
	actions.append(screen.add_button(row,"Focus selected course location",func():view.focus_geometry=screen.course.view.focus_geometry.duplicate(true);view.queue_redraw()))
	view=preload("res://ride_project_coverage.gd").new();add_child(view);view.position_selected.connect(place_vertex)
	screen.text(self,"Click to add boundary points. Select a vertex below, then click to move it. Exclusion polygons clear vegetation. Water holes preserve islands. Ambiguous coastline fragments need a reviewed water polygon.",14)
	var controls:=HBoxContainer.new();add_child(controls)
	rings=OptionButton.new();controls.add_child(rings);rings.item_selected.connect(func(_i):active_vertex=-1;draw_draft())
	actions.append(screen.add_button(controls,"Add hole",func():
		if draft.is_empty():return
		draft.holes.append([]);update_rings();rings.select(draft.holes.size());active_vertex=-1;draw_draft()))
	actions.append(screen.add_button(controls,"Append vertex",func():active_vertex=-1;vertices.deselect_all()))
	actions.append(screen.add_button(controls,"Remove hole",func():
		if rings.selected>0:draft.holes.remove_at(rings.selected-1);update_rings();active_vertex=-1;draw_draft()))
	actions.append(screen.add_button(controls,"Remove vertex",func():
		if active_vertex>=0:active_ring().remove_at(active_vertex);active_vertex=-1;draw_draft()))
	vertices=ItemList.new();vertices.custom_minimum_size.y=82;add_child(vertices);vertices.item_selected.connect(func(i):active_vertex=i)
	var fields:=HBoxContainer.new();add_child(fields)
	for caption in ["Building height (m)","Vegetation density"]:
		var label:Label=screen.text(fields,caption,14);label.autowrap_mode=TextServer.AUTOWRAP_OFF
		var spin:=SpinBox.new();fields.add_child(spin)
		if caption.begins_with("Building"):height=spin;height.min_value=1;height.max_value=120;height.value=6
		else:density=spin;density.min_value=0;density.max_value=1;density.step=0.05;density.value=0.35
	trees=CheckButton.new();trees.text="Trees";fields.add_child(trees)
	elevation=LineEdit.new();elevation.placeholder_text="Water elevation (m), blank = DEM estimate";add_child(elevation)
	var edits:=HBoxContainer.new();add_child(edits)
	for spec in [["Save scenery","save"],["Hide feature","hide"],["Undo edit","undo"],["Reset to source","reset"]]:actions.append(screen.add_button(edits,spec[0],edit.bind(spec[1])))
	screen.text(self,"Building heights default to 6 m when absent. Water levels inferred from terrain need visual review. Build again to apply changes; the previous package remains available.",14)

func populate() -> void:
	var scene:Dictionary=owner_screen.snapshot.get("scenery",{})
	features=scene.get("features",[])
	var ready:Dictionary=owner_screen.snapshot.get("maps",{}).get("scenery_ready",{})
	summary.text="%d scenery features · coverage: buildings %d, vegetation %d, water %d tiles\n%d source warnings · %d conflicting edits" % [features.size(),ready.get("buildings",0),ready.get("vegetation",0),ready.get("water",0),scene.get("warnings",[]).size(),scene.get("conflicts",[]).size()]
	for warning in scene.get("warnings",[]).slice(0,5):summary.text+="\n"+str(warning)
	for conflict in scene.get("conflicts",[]).slice(0,5):summary.text+="\nReview and save or reset changed source: "+str(conflict)
	view.plan=owner_screen.snapshot.get("maps",{});view.samples=owner_screen.snapshot.get("repair_preview",{}).get("samples",[])
	show_features()
	for f in features:
		if f.id==selected_id:load_feature(f);break
	view.queue_redraw()

func show_features() -> void:
	listing.clear()
	for f in features:
		if not search.text.is_empty() and not (str(f.id)+" "+str(f.kind)).to_lower().contains(search.text.to_lower()):continue
		listing.add_item("%s · %s%s" % [f.id,f.kind," · hidden" if f.hidden else ""]);listing.set_item_metadata(listing.item_count-1,f.id)
		if listing.item_count>=500:break

func select_feature(index:int) -> void:
	selected_id=str(listing.get_item_metadata(index))
	for f in features:
		if f.id==selected_id:load_feature(f);break

func load_feature(feature:Dictionary) -> void:
	draft=feature.duplicate(true);selected_id=draft.id;active_vertex=-1
	kinds.select(["building","vegetation","water","exclusion"].find(draft.kind))
	height.value=float(draft.height_m) if draft.get("height_m")!=null else 6.0
	density.value=float(draft.density);trees.button_pressed=draft.trees
	elevation.text=str(draft.elevation_m) if draft.get("elevation_m")!=null else ""
	view.focus_geometry=draft.outer.duplicate(true);update_rings();draw_draft()

func new_polygon() -> void:
	selected_id="user-%d-%d" % [Time.get_unix_time_from_system()*1000,Time.get_ticks_usec()]
	draft={"id":selected_id,"kind":kinds.get_item_text(kinds.selected),"outer":[],"holes":[],"density":0.35,"trees":false,"hidden":false,"provenance":"Manual project polygon"}
	active_vertex=-1;update_rings();draw_draft()

func update_rings() -> void:
	rings.clear();rings.add_item("Outer boundary")
	for i in draft.get("holes",[]).size():rings.add_item("Hole %d" % (i+1))

func active_ring() -> Array:
	return draft.outer if rings.selected<=0 else draft.holes[rings.selected-1]

func place_vertex(lat:float,lon:float) -> void:
	if owner_screen.busy:return
	if draft.is_empty():view.focus_geometry=[[lat,lon]];view.queue_redraw();return
	var ring:=active_ring()
	if active_vertex>=0 and active_vertex<ring.size():ring[active_vertex]=[lat,lon]
	else:ring.append([lat,lon])
	draw_draft()

func draw_draft() -> void:
	vertices.clear();view.roads=[]
	if draft.is_empty():view.queue_redraw();return
	for ring in [draft.outer]+draft.holes:
		if not ring.is_empty():view.roads.append(ring+[ring[0]])
	for p in active_ring():vertices.add_item("%.7f, %.7f" % [p[0],p[1]])
	view.queue_redraw()

func edit(mode:String) -> void:
	if draft.is_empty():owner_screen.feedback.text="Select a feature or draw a polygon first.";return
	var extra:=PackedStringArray(["--mode",mode,"--id",selected_id])
	if mode=="save":
		if not elevation.text.is_empty() and not elevation.text.is_valid_float():owner_screen.feedback.text="Enter a numeric water elevation or leave it blank.";return
		draft.kind=kinds.get_item_text(kinds.selected);draft.height_m=height.value if draft.kind=="building" else null;draft.density=density.value;draft.trees=trees.button_pressed;draft.hidden=false
		draft.elevation_m=float(elevation.text) if not elevation.text.is_empty() else null
		var job_path:String=owner_screen.project_path.path_join("jobs/scenery-edit.json")
		DirAccess.make_dir_recursive_absolute(job_path.get_base_dir())
		var job:=FileAccess.open(job_path,FileAccess.WRITE)
		if job==null:owner_screen.feedback.text="Cannot save this project's scenery draft.";return
		job.store_string(JSON.stringify(draft));job.close()
		extra.append_array(["--feature-request",FileAccess.get_sha256(job_path)])
	owner_screen.run_job("edit-scenery",owner_screen.project_path,extra)

func refresh() -> void:
	for button in actions:button.disabled=owner_screen.busy or not owner_screen.snapshot.has("maps")
	listing.mouse_filter=Control.MOUSE_FILTER_IGNORE if owner_screen.busy else Control.MOUSE_FILTER_STOP
