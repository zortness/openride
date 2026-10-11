shader_type spatial;
render_mode cull_disabled;
uniform vec2 near_min;
uniform vec2 near_max;
uniform bool near_ready = false;
varying vec2 world_xz;
void vertex() {
    world_xz = (MODEL_MATRIX * vec4(VERTEX, 1.0)).xz;
}
void fragment() {
    if (near_ready && all(greaterThan(world_xz, near_min)) && all(lessThan(world_xz, near_max))) {
        discard;
    }
    ALBEDO = vec3(0.0802, 0.147, 0.0908);
    ROUGHNESS = 1.0;
}
