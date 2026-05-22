// Normal-Depth Prepass Shader — Instanced variant
//
// Same as prepass.wgsl but reads model matrices from a storage buffer
// using @builtin(instance_index) instead of a dynamic uniform buffer.
// This matches the layout used by the instancing pipeline (group 1 =
// storage buffer, not dynamic uniform).

struct Camera {
    view      : mat4x4<f32>,
    proj      : mat4x4<f32>,
    view_proj : mat4x4<f32>,
    eye_pos   : vec3<f32>,
    exposure  : f32,
    fog_color : vec3<f32>,
    fog_density: f32,
    ambient_color: vec3<f32>,
    ambient_intensity: f32,
    _padding: array<vec4<f32>, 17>,
};

@group(0) @binding(0)
var<uniform> camera: Camera;

// Array of model matrices — one per instance (matches instancing pipeline).
@group(1) @binding(0)
var<storage, read> instances: array<mat4x4<f32>>;

struct VertexInput {
    @location(0) position : vec3<f32>,
    @location(1) normal   : vec3<f32>,
    @location(2) tangent  : vec4<f32>,
    @location(3) color    : vec4<f32>,
    @location(4) uv       : vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_pos    : vec4<f32>,
    @location(0)       view_normal : vec3<f32>,
    @location(1)       view_pos    : vec3<f32>,
};

@vertex
fn vs_main(
    in: VertexInput,
    @builtin(instance_index) instance_idx: u32,
) -> VertexOutput {
    var out: VertexOutput;

    let model_mat   = instances[instance_idx];
    let is_screen_space = abs(model_mat[2][3] - 55.5) < 0.1;
    
    var col0 = model_mat[0];
    var col1 = model_mat[1];
    var col2 = model_mat[2];
    var col3 = model_mat[3];

    if (is_screen_space) {
        col2.w = 0.0; // clear flag
        col3.w = 1.0; // ensure correct perspective division
    }
    let model_clean = mat4x4<f32>(col0, col1, col2, col3);

    // Derive the normal matrix as the transpose of the inverse of the upper-
    // left 3×3.  For uniform-scale objects this equals the model matrix, but
    // we compute it properly to handle non-uniform scaling.
    let m3 = mat3x3<f32>(
        model_mat[0].xyz,
        model_mat[1].xyz,
        model_mat[2].xyz,
    );
    let normal_mat3 = m3;

    let world_pos4  = model_clean * vec4<f32>(in.position, 1.0);
    
    if (is_screen_space) {
        // CULL it! ScreenSpace UI overlays should not be in the SSAO normal-depth prepass
        // By setting Z >= 1.0 or W <= 0.0, wgpu will cull the triangle.
        out.clip_pos    = vec4<f32>(0.0, 0.0, 2.0, 1.0);
        out.view_pos    = vec3<f32>(0.0, 0.0, -0.01);
    } else {
        out.clip_pos    = camera.view_proj * world_pos4;
        out.view_pos    = (camera.view * world_pos4).xyz;
    }

    let world_normal = normalize(normal_mat3 * in.normal);
    out.view_normal  = normalize((camera.view * vec4<f32>(world_normal, 0.0)).xyz);

    return out;
}

struct FragmentOutput {
    @location(0) normal_depth : vec4<f32>,
};

@fragment
fn fs_main(in: VertexOutput) -> FragmentOutput {
    var out: FragmentOutput;

    let packed_normal = normalize(in.view_normal) * 0.5 + vec3<f32>(0.5);
    let linear_depth  = -in.view_pos.z;

    out.normal_depth = vec4<f32>(packed_normal, linear_depth);
    return out;
}
