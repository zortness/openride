extends SceneTree
var failures:=0
func check(condition:bool,message:String) -> void:
	if not condition:failures+=1;push_error(message)
func _initialize() -> void:
	var library=load("res://content_library.gd")
	var path:="user://library-check-"+str(Time.get_ticks_usec())
	DirAccess.make_dir_recursive_absolute(path)
	for name in ["route.json","chunk_0_0.json"]:
		var f:=FileAccess.open(path.path_join(name),FileAccess.WRITE);f.store_string("{}");f.close()
	var entry:={"path":"chunk_0_0.json","x":0,"z":0,"bytes":2,"sha256":"a".repeat(64)}
	var m:={"kind":"openride.stream-preview","schema_version":1,"trainer_ready":false,"name":"Fixture","tile_m":256,"max_live_chunks":9,"neighborhood_radius_cells":1,"attribution":{},"presentation":{"terrain_corridor":true},"route":{"path":"route.json","bytes":2,"sha256":"a".repeat(64)},"chunks":[entry]}
	write(path,m)
	check(not library.validate(path).has("error"),"Valid bounded index rejected")
	var cases:=[]
	var wrong:Dictionary=m.duplicate(true);wrong.schema_version=99;cases.append(wrong)
	wrong=m.duplicate(true);wrong.route.path="../route.json";cases.append(wrong)
	wrong=m.duplicate(true);wrong.chunks[0].x=0.5;cases.append(wrong)
	wrong=m.duplicate(true);wrong.chunks.append(entry);cases.append(wrong)
	wrong=m.duplicate(true);wrong.route.bytes=3;cases.append(wrong)
	wrong=m.duplicate(true);wrong.presentation.terrain_corridor=false;cases.append(wrong)
	wrong=m.duplicate(true);wrong.coast={"path":"../coast.json"};cases.append(wrong)
	for candidate in cases:
		write(path,candidate);check(library.validate(path).has("error"),"Invalid package accepted")
	check(library.validate(path+"-missing").has("error"),"Missing package accepted")
	print("CONTENT_INDEX_TESTS checks=9 failures=",failures)
	quit(1 if failures else 0)
func write(path:String,value:Dictionary) -> void:
	var f:=FileAccess.open(path.path_join("manifest.json"),FileAccess.WRITE);f.store_string(JSON.stringify(value))
