//! Generic textured 2D triangle mesh.
//!
//! Vertices are already in world space (the engine applies the draw transform
//! on the CPU, which keeps one draw per texture independent of the number of
//! meshes).  The fragment shader mirrors `sprite_2d.wgsl` exactly: texels are
//! sRGB-decoded by wgpu, straight-alpha texels are premultiplied here, and
//! premultiplied texels only take the vertex coverage.  Output is
//! premultiplied linear RGBA, matching every other Pure2D pipeline.

struct CameraUniform {
    view_proj: mat4x4<f32>,  // 64 bytes
    resolution: vec2<f32>,   //  8 bytes
    padding: vec2<f32>,
    // Physical-pixel viewport origin (0,0 for the single main camera).
    viewport_origin: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(1) @binding(0) var mesh_texture: texture_2d<f32>;
@group(1) @binding(1) var mesh_sampler: sampler;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) alpha: f32,
    // 1 = the sampled texels are premultiplied (constant per mesh draw).
    @location(3) @interpolate(flat) premultiplied: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) alpha: f32,
    @location(2) @interpolate(flat) premultiplied: f32,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.uv = in.uv;
    out.alpha = in.alpha;
    out.premultiplied = in.premultiplied;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex_color = textureSample(mesh_texture, mesh_sampler, in.uv);
    let alpha = tex_color.a * in.alpha;
    var rgb = tex_color.rgb;
    if (in.premultiplied > 0.5) {
        rgb = rgb * in.alpha;
    } else {
        rgb = rgb * alpha;
    }
    // Same threshold as the sprite shader: keeps the two paths pixel-equal
    // and still skips nearly invisible fragments.
    if alpha < 0.004 {
        discard;
    }
    return vec4<f32>(rgb, alpha);
}
