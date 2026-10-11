shader_type spatial;
render_mode cull_disabled;
uniform vec3 base_color : source_color = vec3(0.46,0.48,0.33);
uniform bool road = false;
uniform bool use_imagery = false;
uniform sampler2D imagery : source_color, filter_linear_mipmap;
uniform vec4 imagery_bounds;
varying vec3 world;
varying vec3 tint;
float hash(vec2 p) {return fract(sin(dot(p,vec2(127.1,311.7)))*43758.5453);}
float noise(vec2 p) {vec2 i=floor(p);vec2 f=fract(p);f=f*f*(3.0-2.0*f);return mix(mix(hash(i),hash(i+vec2(1,0)),f.x),mix(hash(i+vec2(0,1)),hash(i+vec2(1,1)),f.x),f.y);}
void vertex(){world=(MODEL_MATRIX*vec4(VERTEX,1.0)).xyz;tint=COLOR.rgb;}
void fragment(){
 vec3 color=base_color*tint;
 if(use_imagery){vec2 uv=(world.xz-imagery_bounds.xy)/imagery_bounds.zw;color=mix(color,texture(imagery,uv).rgb,0.62);}
 float grain=noise(world.xz*(road?22.0:3.0));
 float broad=noise(world.xz*0.18);
 ALBEDO=color*(0.94+grain*0.07+broad*0.05);
 ROUGHNESS=0.94;
}
