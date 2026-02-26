// MSDF text shader

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

struct Uniforms {
    resolution: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

// font atlas texture and sampler are bound at group 1
@group(1) @binding(0)
var font_tex: texture_2d<f32>;
@group(1) @binding(1)
var font_sampler: sampler;

@vertex
fn vs_main(
    @location(0) uv: vec2<f32>,
    @location(1) i_pos: vec2<f32>,
    @location(2) i_size: vec2<f32>,
    @location(3) i_uv0: vec2<f32>,
    @location(4) i_uv1: vec2<f32>,
    @location(5) i_color: vec4<f32>,
) -> VsOut {
    var pixel = i_pos + uv * i_size;
    var ndc = (pixel / uniforms.resolution) * 2.0 - vec2<f32>(1.0, 1.0);
    ndc.y = -ndc.y;
    var out: VsOut;
    out.clip_pos = vec4<f32>(ndc, 0.0, 1.0);
    // interpolate uv coordinates across the glyph quad
    out.uv = mix(i_uv0, i_uv1, uv);
    out.color = i_color;
    return out;
}

fn median(r: f32, g: f32, b: f32) -> f32 {
    return max(min(r, g), min(max(r, g), b));
}

// toggle this constant to bypass MSDF evaluation and render the atlas
// contents directly; useful for debugging atlas generation/UVs.
const DEBUG_MSDF: bool = false; // set to true for debug rendering of atlas

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let sample = textureSample(font_tex, font_sampler, in.uv);
    if DEBUG_MSDF {
        // show raw atlas color
        return sample;
    }
    let dist = median(sample.r, sample.g, sample.b);
    // smoothstep around 0.5 for crisp edge
    let alpha = smoothstep(0.5 - 0.01, 0.5 + 0.01, dist);
    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
