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
    @location(3) color : vec4<f32>,
    @location(4) uv : vec2<f32>,
    // built-in instance index used to lookup the correct model matrix
    @builtin(instance_index) instance_idx : u32,
};

@vertex
fn vs_main(in: VertexInput) -> @builtin(position) vec4<f32> {
    let model = models[in.instance_idx];
    let is_screen_space = abs(model[2][3] - 55.5) < 0.1;
    
    // Explicitly reconstruct the matrix to bypass typical DXC/Naga mutation bugs
    var col0 = model[0];
    var col1 = model[1];
    var col2 = model[2];
    var col3 = model[3];
    
    if (is_screen_space) {
        col2.w = 0.0;
        col3.w = 1.0;
    }
    
    let model_clean = mat4x4<f32>(col0, col1, col2, col3);
    let world_pos = model_clean * vec4<f32>(in.position, 1.0);
    
    if (is_screen_space) {
        // Cull UI from shadow maps
        return vec4<f32>(0.0, 0.0, 2.0, 1.0);
    }
    
    return dir_light.light_view_proj * world_pos;
}
