// Very simple vertex-only shader that projects vertices into the light's
// clip space.  The directional light uniform contains a `light_view_proj`
// matrix which is used instead of the camera matrix from the main pass.

struct Model {
    model : mat4x4<f32>,
};

// group 0 is the model matrix (dynamic uniform buffer reused from the
// main pipeline layout)
@group(0) @binding(0)
var<uniform> model : Model;

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

struct VertexInput {
    @location(0) position : vec3<f32>,
    // other vertex attributes are declared so the layout matches the PBR
    // pipeline, but they are unused here.
    @location(1) normal : vec3<f32>,
    @location(2) tangent : vec4<f32>,
    @location(3) color : vec3<f32>,
    @location(4) uv : vec2<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> @builtin(position) vec4<f32> {
    let world_pos = model.model * vec4<f32>(in.position, 1.0);
    return dir_light.light_view_proj * world_pos;
}
