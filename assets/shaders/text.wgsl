// ─── MSDF Text Shader ─────────────────────────────────────────────────────
//
// Renders glyphs from a multi-channel signed distance field atlas.  The atlas
// encodes, per channel, the signed distance to the nearest outline edge that
// enables that channel; every edge enables at least two channels (see
// `crates/ferrous_font/src/msdf_gen.rs`), so `median(R,G,B)` recovers a single
// smooth distance that keeps sharp corners and still anti-aliases straight
// edges.
//
// The anti-aliasing width follows Chlumsky's `screenPxRange` method
// (https://github.com/Chlumsky/msdfgen#using-a-multi-channel-distance-field):
// the atlas range is converted to screen pixels from the glyph's atlas cell
// size and the on-screen quad size.  This is more robust than `fwidth(sd)`,
// which spikes where the nearest-edge channel changes.

// ── Vertex output ────────────────────────────────────────────────────────────

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv0: vec2<f32>,
    @location(3) uv1: vec2<f32>,
    @location(4) quad_size: vec2<f32>,
};

// ── Uniforms & bindings ──────────────────────────────────────────────────────

struct Uniforms {
    resolution: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

// Font atlas texture and sampler at group 1.
@group(1) @binding(0)
var font_tex: texture_2d<f32>;
@group(1) @binding(1)
var font_sampler: sampler;

// Must match `PIXEL_RANGE` in `crates/ferrous_font/src/msdf_gen.rs`.
const PXRANGE: f32 = 6.0;

// ── Vertex stage ─────────────────────────────────────────────────────────────

@vertex
fn vs_main(
    @location(0) uv: vec2<f32>,
    @location(1) i_pos: vec2<f32>,
    @location(2) i_size: vec2<f32>,
    @location(3) i_uv0: vec2<f32>,
    @location(4) i_uv1: vec2<f32>,
    @location(5) i_color: vec4<f32>,
    @location(6) i_z_order: f32,
    @location(7) i_node_id: u32,
) -> VsOut {
    // Transform pixel coordinates to NDC.
    var pixel = i_pos + uv * i_size;
    var ndc = (pixel / uniforms.resolution) * 2.0 - vec2<f32>(1.0, 1.0);
    ndc.y = -ndc.y;

    var out: VsOut;
    out.clip_pos = vec4<f32>(ndc, i_z_order, 1.0);
    // Interpolate UVs across the glyph quad.
    out.uv = mix(i_uv0, i_uv1, uv);
    out.color = i_color;
    // Constant per quad (needed by the fragment stage for the screen px range).
    out.uv0 = i_uv0;
    out.uv1 = i_uv1;
    out.quad_size = i_size;
    return out;
}

// ── Helpers ──────────────────────────────────────────────────────────────────
//
// `msdf_median`, `msdf_cell_texels`, `msdf_screen_px_range` and
// `msdf_opacity` live in `msdf_common.wgsl`, which the pipeline prepends to
// this module.

// Set to `true` to render the raw atlas texture (useful for debugging UVs).
const DEBUG_MSDF: bool = false;

// ── Fragment stage ───────────────────────────────────────────────────────────

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let sample = textureSample(font_tex, font_sampler, in.uv);

    if DEBUG_MSDF {
        return vec4<f32>(sample.rgb, in.color.a);
    }

    let sig_dist = msdf_median(sample.r, sample.g, sample.b);

    // Size of this glyph's atlas cell, in texels (uv span × atlas dimensions).
    // Works for both the font atlas and the icon atlas without a uniform.
    let dims = textureDimensions(font_tex, 0);
    let atlas_dim = vec2<f32>(f32(dims.x), f32(dims.y));
    let cell = msdf_cell_texels(in.uv0, in.uv1, atlas_dim);

    // Distance-field range in screen pixels; never below 1 (AA would fail).
    let screen_px_range = msdf_screen_px_range(in.quad_size, cell, PXRANGE);
    let opacity = msdf_opacity(sig_dist, screen_px_range);

    return vec4<f32>(in.color.rgb, in.color.a * opacity);
}
