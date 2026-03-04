// Normal-Depth Prepass Shader
//
// Renders world-space normals (packed into [0,1]) in RGB and
// linear view-space depth in A.  The SSAO pass reads this texture
// to recover per-pixel normals and positions in view space.

struct Camera {
    view_proj : mat4x4<f32>,
    eye_pos   : vec4<f32>,
};

struct Model {
    model      : mat4x4<f32>,
    normal_mat : mat4x4<f32>,
};

// We also need the raw view and projection matrices for the prepass so
// we can transform positions/normals into view space.  These are packed
// into a second uniform in the same camera bind group slot, but since
// the existing Camera uniform only exposes view_proj we store view and
// proj separately via a dedicated uniform.
struct PrepassCamera {
    view      : mat4x4<f32>,
    proj      : mat4x4<f32>,
    view_proj : mat4x4<f32>,
    eye_pos   : vec4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: PrepassCamera;

@group(1) @binding(0)
var<uniform> model: Model;

struct VertexInput {
    @location(0) position : vec3<f32>,
    @location(1) normal   : vec3<f32>,
    @location(2) tangent  : vec4<f32>,
    @location(3) color    : vec3<f32>,
    @location(4) uv       : vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_pos    : vec4<f32>,
    @location(0)       view_normal : vec3<f32>,
    @location(1)       view_pos    : vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    let world_pos4  = model.model * vec4<f32>(in.position, 1.0);
    out.clip_pos    = camera.view_proj * world_pos4;

    // Normal in view space: use the normal matrix (inverse-transpose of model)
    // then rotate by the view matrix.
    let world_normal = normalize((model.normal_mat * vec4<f32>(in.normal, 0.0)).xyz);
    out.view_normal  = normalize((camera.view * vec4<f32>(world_normal, 0.0)).xyz);

    // View-space position (for reconstructing positions in SSAO)
    let view_pos4   = camera.view * world_pos4;
    out.view_pos    = view_pos4.xyz;

    return out;
}

struct FragmentOutput {
    @location(0) normal_depth : vec4<f32>,
};

@fragment
fn fs_main(in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;

    // Pack view-space normal from [-1,1] to [0,1]
    let packed_normal = normalize(in.view_normal) * 0.5 + vec3<f32>(0.5);

    // Linear depth: negate Z because in view space Z is negative for objects
    // in front of the camera (right-handed).  Store as positive value.
    let linear_depth = -in.view_pos.z;

    out.normal_depth = vec4<f32>(packed_normal, linear_depth);
    return out;
}
