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
	if entry.has("imagery"):
		var info: Dictionary=entry.imagery
		var expected: String="imagery_%d_%d.png" % [entry.x,entry.z]
		if info.get("path")!=expected:return {"error":"Unsafe chunk imagery path"}
		var picture_file:=FileAccess.open(root_path.path_join(expected),FileAccess.READ)
		if picture_file==null or picture_file.get_length()>128*1024 or picture_file.get_length()!=info.get("bytes"):return {"error":"Missing or oversized chunk imagery"}
		var raw:=picture_file.get_buffer(picture_file.get_length())
		hash=HashingContext.new();hash.start(HashingContext.HASH_SHA256);hash.update(raw)
		if hash.finish().hex_encode()!=info.get("sha256"):return {"error":"Chunk imagery checksum mismatch"}
		# Bound dimensions before invoking the decoder, including on untrusted ZIPs.
		if raw.size()<24 or raw.slice(0,8).hex_encode()!="89504e470d0a1a0a" or raw.slice(16,24).hex_encode()!="0000008200000082":return {"error":"Invalid chunk imagery dimensions"}
		var picture:=Image.new()
		if picture.load_png_from_buffer(raw)!=OK:return {"error":"Corrupt chunk imagery"}
		picture.generate_mipmaps()
		parsed["_imagery_image"]=picture
	return {"geometry":parsed}
