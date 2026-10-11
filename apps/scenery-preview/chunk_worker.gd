extends RefCounted
## Worker-only file reads, checksum verification and JSON parsing. No scene-tree access.
static func read_chunk(root_path: String, entry: Dictionary) -> Dictionary:
	var path := root_path.path_join(entry.path)
	var file := FileAccess.open(path,FileAccess.READ)
	if file == null or file.get_length()>2*1024*1024 or file.get_length()!=entry.get("bytes"):
		return {"error":"Missing/oversized streaming payload: "+entry.path}
	var bytes := file.get_buffer(file.get_length())
	var hash := HashingContext.new()
	hash.start(HashingContext.HASH_SHA256)
	hash.update(bytes)
	if hash.finish().hex_encode()!=entry.get("sha256"):
		return {"error":"Streaming checksum mismatch: "+entry.path}
	var parsed: Variant = JSON.parse_string(bytes.get_string_from_utf8())
	if not parsed is Dictionary or not parsed.get("terrain") is Dictionary or not parsed.get("road") is Dictionary or not parsed.get("lines") is Array:
		return {"error":"Invalid chunk geometry: "+entry.path}
	return {"geometry":parsed}
