extends SceneTree

func _initialize() -> void:
	var points: Array = []
	var matches: Array = []
	for index in range(5):
		points.append({"source_sample": index, "position": [index*10, 0, 0], "break_before": index == 0})
		matches.append({"candidates": [{"way_id": str(index), "node_ids": [str(index), str(index+1)], "name": "Main", "highway": "primary", "projected_xz": [index*10, 0]}]})
	var edits := {"2": {"source_sample": 2, "way_id": "2", "node_ids": ["2", "3"]}}
	var planner = preload("res://road_propagation.gd")
	if not check(planner.plan(points, matches, edits, 2).choices.size() == 4, "extends both ways across same-name ways"): return
	points[3].break_before = true
	if not check(planner.plan(points, matches, edits, 2).choices.size() == 2, "stops at forward break"): return
	points[3].break_before = false
	points[2].break_before = true
	if not check(planner.plan(points, matches, edits, 2).choices.size() == 2, "stops at backward break"): return
	points[2].break_before = false
	matches[3].candidates.append(matches[3].candidates[0].duplicate(true))
	if not check(planner.plan(points, matches, edits, 2).choices.size() == 2, "stops at ambiguous road"): return
	matches[3].candidates.pop_back()
	edits["3"] = {"way_id": "other", "node_ids": ["x", "y"]}
	if not check(planner.plan(points, matches, edits, 2).choices.size() == 2, "preserves conflicting choices"): return
	edits.erase("3")
	matches[3].candidates[0].projected_xz = [1000,0]
	if not check(planner.plan(points, matches, edits, 2).choices.size() == 2, "stops at projected gaps"): return
	matches[3].candidates[0].projected_xz = [30,0]
	points[3].position = [1000,0,0]
	if not check(planner.plan(points, matches, edits, 2).choices.size() == 2, "stops at source gaps"): return
	points[3].position = [30,0,0]
	matches[3].candidates[0].name = "Side street"
	if not check(planner.plan(points, matches, edits, 2).choices.size() == 2, "stops when road disappears"): return
	if not check(planner.plan(points, matches, {}, 2).choices.is_empty(), "requires seed choice"): return
	print("ROAD_PROPAGATION_OK checks=9")
	quit(0)

func check(condition: bool, label: String) -> bool:
	if not condition:
		push_error(label)
		quit(1)
	return condition
