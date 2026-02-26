// ─── MSDF Text Shader ─────────────────────────────────────────────────────
//
// Renders glyphs from a multi-channel signed distance field atlas.
//
// The atlas encodes signed distances so that values *below* 0.5 are inside
// the glyph and values *above* 0.5 are outside.  The `median(R,G,B)` trick
// recovers a single SDF that preserves sharp corners.

// ── Vertex output ────────────────────────────────────────────────────────────

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

// ── Uniforms & bindings ──────────────────────────────────────────────────────

struct Uniforms {
    resolution: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

// Font atlas texture and point sampler at group 1.
@group(1) @binding(0)
var font_tex: texture_2d<f32>;
@group(1) @binding(1)
var font_sampler: sampler;

// ── Vertex stage ─────────────────────────────────────────────────────────────

@vertex
fn vs_main(
    @location(0) uv: vec2<f32>,
    @location(1) i_pos: vec2<f32>,
    @location(2) i_size: vec2<f32>,
    @location(3) i_uv0: vec2<f32>,
    @location(4) i_uv1: vec2<f32>,
    @location(5) i_color: vec4<f32>,
) -> VsOut {
    // Transform pixel coordinates to NDC.
    var pixel = i_pos + uv * i_size;
    var ndc = (pixel / uniforms.resolution) * 2.0 - vec2<f32>(1.0, 1.0);
    ndc.y = -ndc.y;

    var out: VsOut;
    out.clip_pos = vec4<f32>(ndc, 0.0, 1.0);
    // Interpolate UVs across the glyph quad.
    out.uv = mix(i_uv0, i_uv1, uv);
    out.color = i_color;
    return out;
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Median of three values — used to collapse the three SDF channels into one.
fn median(r: f32, g: f32, b: f32) -> f32 {
    return max(min(r, g), min(max(r, g), b));
}

// Set to `true` to render the raw atlas texture (useful for debugging UVs).
const DEBUG_MSDF: bool = false;

// ── Fragment stage ───────────────────────────────────────────────────────────

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let sample = textureSample(font_tex, font_sampler, in.uv);
    
    let sig_dist = median(sample.r, sample.g, sample.b);
    let w = max(fwidth(sig_dist), 0.001); // Asegurar que w nunca sea negativo/cero

    // CORRECCIÓN WGSL: bordes ordenados de menor a mayor e invertimos:
    let opacity = 1.0 - smoothstep(0.5 - w, 0.5 + w, sig_dist);

    return vec4<f32>(in.color.rgb, in.color.a * opacity);
}