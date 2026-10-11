extends RefCounted
## Presentation helpers. No changes to route distance, elevation or trainer control.
static func markers(parent:Node3D,points:Array,metadata:Array) -> void:
	for finish in [false,true]:
		var i:int=points.size()-1 if finish else 0
		var q:Dictionary=points[i];var p:Vector3=Vector3(q.position[0],q.position[1],q.position[2])
		var other:Dictionary=points[i-1 if finish else 1]
		var direction:Vector3=(p-Vector3(other.position[0],other.position[1],other.position[2])) if finish else (Vector3(other.position[0],other.position[1],other.position[2])-p)
		var width:float=float(metadata[i].width_m) if metadata.size()==points.size() else 6.0
		var gate:=Node3D.new();parent.add_child(gate);gate.position=p+(Vector3.ZERO if finish else direction.normalized()*8);gate.rotation.y=atan2(-direction.x,-direction.z)
		var mat:=StandardMaterial3D.new();mat.albedo_color=Color("e6a441") if finish else Color("43bfb2")
		for side in [-1,1]:
			var post:=MeshInstance3D.new();var cylinder:=CylinderMesh.new();cylinder.top_radius=.12;cylinder.bottom_radius=.15;cylinder.height=5;post.mesh=cylinder;post.material_override=mat;post.position=Vector3(side*(width/2+.6),2.5,0);gate.add_child(post)
		var beam:=MeshInstance3D.new();var box:=BoxMesh.new();box.size=Vector3(width+1.5,.8,.3);beam.mesh=box;beam.material_override=mat;beam.position.y=4.7;gate.add_child(beam)
		for side in [-1,1]:
			var label:=Label3D.new();label.text="FINISH" if finish else "START";label.font_size=100;label.pixel_size=.007;label.position=Vector3(0,4.7,side*.17);label.rotation.y=0 if side==1 else PI;label.modulate=Color("10232c");gate.add_child(label)

static func vegetation(parent:Node3D,entries:Array) -> bool:
	if entries.size()>4000:return false
	for entry in entries:
		if not entry is Array or entry.size()!=6:return false
		for v in entry:
			if not (v is float or v is int) or not is_finite(v) or absf(v)>100000:return false
		if entry[3]<0.1 or entry[3]>3 or (entry[4]!=0 and entry[4]!=1):return false
	for kind in [0,1]:
		var selected:Array=entries.filter(func(e:Array):return int(e[4])==kind)
		if selected.is_empty():continue
		for trunk in ([false,true] if kind==1 else [false]):
			var mesh:PrimitiveMesh
			if trunk:
				var stem:=CylinderMesh.new();stem.height=4;stem.top_radius=.13;stem.bottom_radius=.2;stem.radial_segments=5;mesh=stem
			else:
				var crown:=SphereMesh.new();crown.radius=2.2 if kind==1 else .9;crown.height=5 if kind==1 else 1.2;crown.radial_segments=7;crown.rings=3;mesh=crown
			var mat:=StandardMaterial3D.new();mat.albedo_color=Color("65503b") if trunk else Color("425637") if kind==1 else Color("777848");mesh.material=mat
			var multi:=MultiMesh.new();multi.transform_format=MultiMesh.TRANSFORM_3D;multi.mesh=mesh;multi.instance_count=selected.size()
			for i in range(selected.size()):
				var e:Array=selected[i];var basis:=Basis(Vector3.UP,float(e[5])).scaled(Vector3.ONE*float(e[3]));var y:float=(2.0 if trunk else 4.3 if kind==1 else .45)*float(e[3]);multi.set_instance_transform(i,Transform3D(basis,Vector3(e[0],e[1]+y,e[2])))
			var instance:=MultiMeshInstance3D.new();instance.multimesh=multi;instance.visibility_range_end=260 if kind==1 else 160;parent.add_child(instance)
	return true
