// Variant of the shadow shader that reads model matrices from a storage
// buffer, suitable for use with `draw_indexed` calls that specify
// `instance_count > 1`.  The buffer is indexed by the builtin
// `instance_index` so a single draw call renders all instances.

struct DirectionalLight {
    direction : vec3<f32>,
    _pad0 : f32,
    color : vec3<f32>,
    intensity : f32,
    light_view_proj : mat4x4<f32>,
};

// group 1 holds the directional light uniform (same layout as the world pass)
@group(1) @binding(0)
var<uniform> dir_light : DirectionalLight;

// group 0 is now a storage buffer containing an array of model matrices
// (each mat4x4<f32>).  We index this using the instance index provided by
// the vertex stage.
@group(0) @binding(0)
var<storage, read> models : array<mat4x4<f32>>;

struct VertexInput {
    @location(0) position : vec3<f32>,
    // other vertex attributes are declared so the layout matches the PBR
    // pipeline, but they are unused here.
    @location(1) normal : vec3<f32>,
    @location(2) tangent : vec4<f32>,
    @location(3) color : vec3<f32>,
    @location(4) uv : vec2<f32>,
    // built-in instance index used to lookup the correct model matrix
    @builtin(instance_index) instance_idx : u32,
};

@vertex
fn vs_main(in: VertexInput) -> @builtin(position) vec4<f32> {
    let idx = in.instance_idx;
    let world_pos = models[idx] * vec4<f32>(in.position, 1.0);
    return dir_light.light_view_proj * world_pos;
}
