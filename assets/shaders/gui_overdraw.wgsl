// ─── GUI Overdraw Shader ─────────────────────────────────────────────────────
//
// Runtime overdraw debug view for the GUI/2D quad and text passes.
//
// Every drawn fragment adds a fixed, low-intensity heat colour to the target
// with additive blending.  Overlapping primitives therefore brighten
// monotonically: a red/green channel value of `n * OVERDRAW_HEAT` means that
// roughly `n` fragments were blended into that pixel during the frame (the
// exact count saturates at 1.0 per channel).
//
// The vertex input/output contract mirrors `assets/shaders/gui.wgsl`
// (`vs_main`, instance locations 0..=10) and `assets/shaders/text.wgsl`
// (`vs_text`, instance locations 0..=7) so the quad and text pipelines can be
// swapped for these without touching the instance buffers or pipeline
// layouts.  The fragment stage ignores the interpolated attributes because the
// heat colour is constant; only the clip-space position is needed.
//
// The pipeline that consumes this module must use additive blending
// (`src = One`, `dst = One`), must NOT write depth and must NOT test it
// (`depth_compare = Always`): the view is diagnostic and must never be hidden
// by the depth buffer.

// Heat added per fragment. Keep in sync with `OVERDRAW_HEAT` in
// `crates/ferrous_ui_render/src/pipelines/overdraw.rs`.
const OVERDRAW_HEAT: f32 = 0.02;

struct Uniforms {
    resolution: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

// ─── Quad path (mirrors gui.wgsl) ────────────────────────────────────────────

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) color:     vec4<f32>,
    @location(1) color_b:   vec4<f32>,
    @location(2) radii:     vec4<f32>,
    @location(3) uv:        vec2<f32>,
    @location(4) uv0:       vec2<f32>,
    @location(5) uv1:       vec2<f32>,
    @location(6) size:      vec2<f32>,
    @location(7) @interpolate(flat) tex_index: u32,
    @location(8) @interpolate(flat) flags:     u32,
    @location(9) raw_uv:    vec2<f32>,
};

@vertex
fn vs_main(
    @location(0)  uv:          vec2<f32>,
    @location(1)  i_pos:       vec2<f32>,
    @location(2)  i_size:      vec2<f32>,
    @location(3)  i_uv0:       vec2<f32>,
    @location(4)  i_uv1:       vec2<f32>,
    @location(5)  i_color:     vec4<f32>,
    @location(6)  i_color_b:   vec4<f32>,
    @location(7)  i_radii:     vec4<f32>,
    @location(8)  i_tex_index: u32,
    @location(9)  i_flags:     u32,
    @location(10) i_z_order:   f32,
) -> VsOut {
    var pixel = i_pos + uv * i_size;
    var ndc   = (pixel / uniforms.resolution) * 2.0 - vec2<f32>(1.0);
    ndc.y = -ndc.y;

    var out: VsOut;
    out.clip_pos  = vec4<f32>(ndc, 1.0 - i_z_order, 1.0);
    out.color     = i_color;
    out.color_b   = i_color_b;
    out.radii     = i_radii;
    out.uv        = i_uv0 + uv * (i_uv1 - i_uv0);
    out.uv0       = i_uv0;
    out.uv1       = i_uv1;
    out.size      = i_size;
    out.tex_index = i_tex_index;
    out.flags     = i_flags;
    out.raw_uv    = uv;
    return out;
}

// ─── Text/icon path (mirrors text.wgsl) ──────────────────────────────────────

struct TextOut {
    @builtin(position) clip_pos: vec4<f32>,
};

@vertex
fn vs_text(
    @location(0) uv:       vec2<f32>,
    @location(1) i_pos:    vec2<f32>,
    @location(2) i_size:   vec2<f32>,
    @location(3) i_uv0:    vec2<f32>,
    @location(4) i_uv1:    vec2<f32>,
    @location(5) i_color:  vec4<f32>,
    @location(6) i_z_order: f32,
    @location(7) i_node_id: u32,
) -> TextOut {
    var pixel = i_pos + uv * i_size;
    var ndc   = (pixel / uniforms.resolution) * 2.0 - vec2<f32>(1.0, 1.0);
    ndc.y = -ndc.y;

    var out: TextOut;
    out.clip_pos = vec4<f32>(ndc, i_z_order, 1.0);
    return out;
}

// ─── Fragment stages ─────────────────────────────────────────────────────────

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(OVERDRAW_HEAT, OVERDRAW_HEAT, 0.0, 1.0);
}

@fragment
fn fs_text(in: TextOut) -> @location(0) vec4<f32> {
    return vec4<f32>(OVERDRAW_HEAT, OVERDRAW_HEAT, 0.0, 1.0);
}
