// ─── SVG Shader ───────────────────────────────────────────────────────────
//
// Renders arbitrary meshes (tessellated SVGs).

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};

struct Uniforms {
    resolution: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

// Push constants or another bind group for instance data (z, color)
// For now, we'll use a simplified version:
// We'll pass Z and Color in a second vertex buffer or uniform bind group.
// But since this is a separate pipeline, let's use a simple direct draw.

@vertex
fn vs_main(
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) z_order: f32,
) -> VsOut {
    let ndc = (position / uniforms.resolution) * 2.0 - vec2<f32>(1.0);
    var out: VsOut;
    out.clip_pos = vec4<f32>(ndc.x, -ndc.y, z_order, 1.0);
    out.color = color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return in.color;
}
