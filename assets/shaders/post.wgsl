// Post-processing shader
// Group 0: Textures (HDR, Bloom, Samplers)
// Group 1: Camera (CameraUniform)

@group(0) @binding(0) var t_hdr: texture_2d<f32>;
@group(0) @binding(1) var s_hdr: sampler;
@group(0) @binding(2) var t_bloom: texture_2d<f32>;
@group(0) @binding(3) var s_bloom: sampler;

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
@group(1) @binding(0) var<uniform> camera: Camera;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(i32(vertex_index & 1u) << 2u) - 1.0;
    let y = f32(i32(vertex_index & 2u) << 1u) - 1.0;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let hdr_color = textureSampleLevel(t_hdr, s_hdr, in.uv, 0.0).rgb;
    let bloom_color = textureSampleLevel(t_bloom, s_bloom, in.uv, 0.0).rgb;

    // Use camera exposure from uniform (now synchronized with 512-byte layout)
    let exposure = camera.exposure; 
    var color = (hdr_color + bloom_color * 0.15) * exposure;

    // ACES Filmic Tone Mapping
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    color = saturate((color * (a * color + b)) / (color * (c * color + d) + e));

    // Gamma Correction
    color = pow(color, vec3<f32>(1.0 / 2.2));

    // Return final RGBA color. WGPU handles the mapping to the target surface format (e.g., BGRA) automatically.
    return vec4<f32>(color, 1.0);
}
