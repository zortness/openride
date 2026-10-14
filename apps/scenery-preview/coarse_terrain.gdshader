shader_type spatial;
render_mode cull_disabled;
uniform vec2 nearby_cell;
varying vec3 world;
void vertex(){world=(MODEL_MATRIX*vec4(VERTEX,1.0)).xyz;}
void fragment(){
 vec2 cell=floor(world.xz/256.0);
 if(abs(cell.x-nearby_cell.x)<=1.0 && abs(cell.y-nearby_cell.y)<=1.0){discard;}
 ALBEDO=vec3(0.31,0.42,0.33);ROUGHNESS=1.0;
}
