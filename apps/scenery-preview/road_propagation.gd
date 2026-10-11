extends RefCounted
## Authoring convenience only: matching names do not prove connected OSM topology.

static func plan(points: Array, matches: Array, edits: Dictionary, index: int) -> Dictionary:
	var result := {"choices": [], "stops": []}
	var selected: Dictionary = edits.get(str(int(points[index].source_sample)), {})
	var seed: Dictionary = {}
	for candidate in matches[index].candidates:
		if selected.get("way_id") == candidate.way_id and selected.get("node_ids") == candidate.node_ids:
			seed = candidate
	if seed.is_empty():
		result.stops.append("Select a road at the current point first")
		return result
	for direction in [-1, 1]:
		var previous := index
		var current: int = index + direction
		var previous_candidate := seed
		while current >= 0 and current < points.size():
			var reason := ""
			if points[maxi(previous, current)].break_before:
				reason = "source break"
			var options: Array = []
			for candidate in matches[current].candidates:
				var same_name: bool = seed.get("name") != null and seed.get("name") != "" and seed.get("name") == candidate.get("name") and seed.highway == candidate.highway
				if candidate.has("projected_xz") and (candidate.way_id == seed.way_id or same_name):
					options.append(candidate)
			if reason.is_empty() and options.size() != 1:
				reason = "road unavailable" if options.is_empty() else "ambiguous road"
			if reason.is_empty():
				var a: Array = previous_candidate.projected_xz
				var b: Array = options[0].projected_xz
				var source_a: Array = points[previous].position
				var source_b: Array = points[current].position
				if Vector2(a[0]-b[0], a[1]-b[1]).length() > 100 or Vector2(source_a[0]-source_b[0], source_a[2]-source_b[2]).length() > 100:
					reason = "gap over 100 m"
			var key := str(int(points[current].source_sample))
			if reason.is_empty() and edits.has(key):
				if edits[key].way_id != options[0].way_id or edits[key].node_ids != options[0].node_ids:
					reason = "existing conflicting choice"
			if not reason.is_empty():
				result.stops.append("sample %d: %s" % [points[current].source_sample, reason])
				break
			if not edits.has(key):
				result.choices.append({"source_sample": points[current].source_sample, "way_id": options[0].way_id, "node_ids": options[0].node_ids})
			previous_candidate = options[0]
			previous = current
			current += direction
	return result
