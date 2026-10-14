extends VBoxContainer
var owner_screen: Control
var summary: Label
var button: Button
var issues: ItemList
var view: Control
var selected_sample := -1
var latitude: LineEdit
var longitude: LineEdit
var ways: OptionButton
var edit_buttons: Array[Button] = []
var show_all: CheckButton
var pending_samples: Array[int] = []
var next_button: Button
var stretch_button: Button
var undo_button: Button
var choice_note: Label
var connection_button: Button
var connection_note: Label
var selected_status := ""

func setup(screen: Control) -> void:
	owner_screen=screen;add_theme_constant_override("separation",10)
	screen.text(self,"Which roads did you ride?",24)
	summary=screen.text(self,"Infer a connected road sequence, then review only the uncertain stretches.",16)
	button=screen.add_button(self,"Infer roads",func():screen.run_job("infer-roads",screen.project_path))
	show_all=CheckButton.new();show_all.text="Inspect all road occurrences";add_child(show_all);show_all.toggled.connect(func(_v):populate())
	next_button=screen.add_button(self,"Next to review",next_issue)
	var workspace:=HBoxContainer.new();workspace.add_theme_constant_override("separation",16);add_child(workspace)
	issues=ItemList.new();issues.custom_minimum_size=Vector2(310,280);workspace.add_child(issues);issues.item_selected.connect(select_issue)
	view=preload("res://ride_project_coverage.gd").new();view.size_flags_horizontal=Control.SIZE_EXPAND_FILL;workspace.add_child(view)
	view.custom_minimum_size.y=340
	var context_row:=HBoxContainer.new();add_child(context_row)
	screen.add_button(context_row,"Wider context",func():view.focus_padding=minf(view.focus_padding*2.0,0.032);view.queue_redraw())
	screen.add_button(context_row,"Closer view",func():view.focus_padding=maxf(view.focus_padding/2.0,0.00025);view.queue_redraw())
	var row:=HBoxContainer.new();add_child(row)
	ways=OptionButton.new();ways.size_flags_horizontal=Control.SIZE_EXPAND_FILL;ways.fit_to_longest_item=false;row.add_child(ways)
	ways.item_selected.connect(func(_i):preview_road())
	edit_buttons.append(screen.add_button(row,"Use this road",choose_road.bind(false)))
	undo_button=screen.add_button(row,"Undo road choice",choose_road.bind(true));edit_buttons.append(undo_button)
	stretch_button=screen.add_button(self,"Use road along this stretch",func():choose_road(false,true));edit_buttons.append(stretch_button)
	choice_note=screen.text(self,"Apply across nearby crossings on this road, up to 5 km each way. Stops at gaps, saved choices, branches, or where the recording leaves the road. Undo restores the group together.",14)
	var connections:VBoxContainer=screen.foldout(self,"Advanced: recorded connections")
	connection_button=screen.add_button(connections,"Connect using recording",connect_recording)
	connection_note=screen.text(connections,"",14)
	screen.text(self,"Recording: green · Proposed road: amber. A road choice applies to this visit, so repeated roads and out-and-backs stay separate.",14)
	var recovery:VBoxContainer=screen.foldout(self,"Changed recording or maps")
	screen.text(recovery,"Restart keeps a copy of your previous road choices, then infers roads again from the current recording and maps.",14)
	screen.add_button(recovery,"Restart road inference (archive choices)",func():screen.run_job("reset-roads",screen.project_path))
	var advanced:VBoxContainer=screen.foldout(self,"Advanced: legacy trace corrections")
	var coordinates:=HBoxContainer.new();advanced.add_child(coordinates)
	latitude=LineEdit.new();latitude.placeholder_text="Latitude";coordinates.add_child(latitude)
	longitude=LineEdit.new();longitude.placeholder_text="Longitude";coordinates.add_child(longitude)
	view.position_selected.connect(func(lat:float,lon:float):latitude.text="%.7f"%lat;longitude.text="%.7f"%lon)
	screen.add_button(advanced,"Rebuild legacy trace course",func():screen.run_job("build-course",screen.project_path))
	for spec in [["Keep recorded trace","trace"],["Use edited point","point"],["Choose road/path","way"],["Join gap to point","join"],["Undo edit","undo"]]:
		edit_buttons.append(screen.add_button(advanced,spec[0],edit.bind(spec[1])))
	screen.add_button(advanced,"Archive stale legacy edits",func():screen.run_job("archive-edits",screen.project_path))
	screen.text(advanced,"Legacy corrections are preserved separately. Road-first inference uses the original reference recording and its own saved road choices.",14)

func choose_road(undo:bool, nearby:bool=false) -> void:
	if selected_sample<0:return
	var args:=PackedStringArray(["--sample",str(selected_sample)])
	if undo:args.append_array(["--mode","undo"])
	else:
		if ways.selected<0:return
		args.append_array(["--way",str(ways.get_selected_metadata())])
		if nearby:args.append_array(["--scope","nearby"])
	owner_screen.run_job("choose-road",owner_screen.project_path,args)

func preview_road() -> void:
	var roads:Dictionary=owner_screen.snapshot.get("course",{}).get("roads",{})
	view.highlight_roads=[]
	view.selected_road_label=ways.get_item_text(ways.selected) if ways.selected>=0 else ""
	if ways.selected>=0:
		var geometry:Array=roads.get("nearby_roads",{}).get(str(ways.get_selected_metadata()),[])
		if not geometry.is_empty():view.highlight_roads=[geometry]
	if selected_status in ["disconnected","manual_connection"]:
		var connection:Array=roads.get("connection_previews",{}).get(str(selected_sample),[])
		if not connection.is_empty():view.highlight_roads=[connection];view.focus_geometry=connection
	view.queue_redraw()

func connect_recording() -> void:
	owner_screen.run_job("choose-road",owner_screen.project_path,PackedStringArray(["--sample",str(selected_sample),"--mode","undo-connection" if selected_status=="manual_connection" else "connect"]))

func populate() -> void:
	issues.clear();ways.clear();pending_samples.clear()
	view.plan=owner_screen.snapshot.get("maps",{})
	view.samples=owner_screen.snapshot.get("repair_preview",{}).get("samples",[])
	view.roads=[];view.highlight_roads=[];view.focus_geometry=[];view.road_labels=[]
	var course:Dictionary=owner_screen.snapshot.get("course",{})
	if course.is_empty():summary.text="Infer a connected road sequence, then review only the uncertain stretches."
	else:
		var roads:Dictionary=course.get("roads",{})
		if roads.is_empty():summary.text="An earlier trace-based course is preserved. Choose Infer roads to create the new road-first course."
		else:
			summary.text="%.2f km on roads · %d stretches to review\nHeights will be generated from topography when you build the preview."%[course.length_m/1000.,course.unresolved]
			if course.unresolved==0:summary.text+="\nRoad sequence ready. Continue to Preview."
			if roads.get("overlay_limited",false):summary.text+="\nSome road outlines are omitted from the map; inference uses the complete road graph."
		for section in course.sections:view.roads.append(section.geometry)
		if not roads.is_empty() and not show_all.button_pressed:
			var review_starts:Dictionary={}
			for issue in roads.issues:
				review_starts[int(issue.first_sample)]=str(issue.reason).replace("_"," ").capitalize()
			var distance_m:=0.0
			var accepted:=0
			for section in course.sections:
				var sample:=int(section.source_sample)
				if review_starts.has(sample):
					issues.add_item("○ %.2f km · %s"%[distance_m/1000.,review_starts[sample]])
					issues.set_item_metadata(issues.item_count-1,sample);pending_samples.append(sample)
				elif section.status in ["manual_match","manual_connection"]:
					issues.add_item("✓ %.2f km · %s"%[distance_m/1000.,"Recorded connection" if section.status=="manual_connection" else "Accepted"])
					issues.set_item_metadata(issues.item_count-1,sample)
					issues.set_item_custom_fg_color(issues.item_count-1,Color("71d6c2"));accepted+=1
				distance_m+=float(section.length_m)
			summary.text+="\n%d saved road choices · Checkmarks mark your accepted visits; automatic matches need no action."%accepted
		else:
			for section in course.sections:
				if show_all.button_pressed or section.status not in ["matched","manual_match","manual"] or section.break_before:
					var name:String=roads.get("way_names",{}).get(str(int(section.way)) if section.way!=null else "","Unmapped")
					var state:String="✓ Recorded connection" if section.status=="manual_connection" else ("✓ Accepted" if section.status=="manual_match" else ("Automatic" if section.status=="matched" else str(section.status).replace("_"," ").capitalize()))
					issues.add_item("%s · %s · visit %d"%[name,state,section.source_sample]);issues.set_item_metadata(issues.item_count-1,int(section.source_sample))
					if section.status not in ["matched","manual_match","manual","manual_connection"]:pending_samples.append(int(section.source_sample))
	for index in issues.item_count:
		if int(issues.get_item_metadata(index))==selected_sample:issues.select(index)
	if issues.get_selected_items().is_empty() and issues.item_count>0:issues.select(0)
	if not issues.get_selected_items().is_empty():select_issue(issues.get_selected_items()[0])
	else:selected_sample=-1;selected_status=""
	view.queue_redraw();refresh()

func select_issue(index:int) -> void:
	selected_sample=int(issues.get_item_metadata(index));ways.clear();view.road_labels=[];selected_status=""
	var roads:Dictionary=owner_screen.snapshot.get("course",{}).get("roads",{})
	for section in owner_screen.snapshot.get("course",{}).get("sections",[]):
		if int(section.source_sample)!=selected_sample:continue
		selected_status=section.status
		view.focus_geometry=section.geometry
		if not section.geometry.is_empty():latitude.text="%.7f"%section.geometry[-1][0];longitude.text="%.7f"%section.geometry[-1][1]
		for way in section.get("alternative_ways",[]):
			var key:=str(int(way))
			var caption: String="%d · %s"%[ways.item_count+1,roads.get("way_names",{}).get(key,"OSM road "+key)]
			ways.add_item(caption);ways.set_item_metadata(ways.item_count-1,key)
			view.road_labels.append({"text":caption,"geometry":roads.get("nearby_roads",{}).get(key,[])})
			if section.way!=null and int(section.way)==int(way):ways.select(ways.item_count-1)
	preview_road();refresh()
	var group_size:int=int(roads.get("choice_groups",{}).get(str(selected_sample),0))
	undo_button.text="Undo stretch choice (%d visits)"%group_size if group_size>0 else "Undo road choice"

func next_issue() -> void:
	if pending_samples.is_empty():return
	var target:int=pending_samples[0]
	for sample in pending_samples:
		if sample>selected_sample:target=sample;break
	for index in issues.item_count:
		if int(issues.get_item_metadata(index))==target:
			issues.select(index);issues.ensure_current_is_visible();select_issue(index);return

func edit(mode: String) -> void:
	if selected_sample < 0: return
	var args := PackedStringArray(["--sample", str(selected_sample), "--mode", mode])
	if mode in ["point", "join"]: args.append_array(["--latitude", latitude.text, "--longitude", longitude.text])
	if mode == "way":
		if ways.selected < 0: return
		args.append_array(["--way", ways.get_item_metadata(ways.selected)])
	owner_screen.run_job("edit-course", owner_screen.project_path, args)

func refresh() -> void:
	next_button.disabled=owner_screen.busy or pending_samples.is_empty()
	button.disabled = owner_screen.busy or owner_screen.snapshot.get("maps", {}).get("streets_ready", 0) == 0
	for item in edit_buttons: item.disabled = owner_screen.busy or selected_sample < 0
	stretch_button.disabled=owner_screen.busy or selected_sample<0 or ways.item_count==0
	var connections:Dictionary=owner_screen.snapshot.get("course",{}).get("roads",{}).get("connection_previews",{})
	connection_button.visible=selected_status in ["disconnected","manual_connection"]
	connection_note.visible=connection_button.visible
	connection_button.text="Undo recorded connection" if selected_status=="manual_connection" else "Connect using recording"
	connection_button.disabled=owner_screen.busy or (selected_status!="manual_connection" and not connections.has(str(selected_sample)))
	connection_note.text="Amber previews the short recorded connection between road endpoints. Accept only if this is the path you rode. Terrain supplies its elevation." if connections.has(str(selected_sample)) else "No short continuous recording is available here. Recorded connections are limited to 250 m and cannot bridge missing GPS data."
