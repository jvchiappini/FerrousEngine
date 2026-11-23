// Backdrop blur for the GUI: a prefiltered, level-matched separable Gaussian.
//
// A naive blur samples the full-resolution scene at widely spaced taps, so thin
// content (a text baseline, a 1 px rule) is copied once per tap instead of being
// integrated. This module avoids that in two stages:
//
//   1. `fs_downsample` box-filters the scene down a 2x chain (`level 1`, `level
//      2`, ...). Each pass averages the four source texels under the destination
//      texel, so everything above the level's Nyquist limit is removed before it
//      can alias.
//   2. `fs_blur` runs a separable Gaussian at the level whose texel is closest
//      to the requested sigma, with taps exactly one destination texel apart.
//      The weights are computed on the CPU for the exact sigma and uploaded, so
//      the impulse response has the requested width and no secondary peaks.
//
// The GUI quad pipeline samples the final level with a linear sampler, so the
// blurred result is bilinearly upsampled when the backdrop rect is drawn.
//
// `texel` is the source texel size for the downsample pass and the destination
// texel size for the blur pass.

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

struct BlurParams {
    // Source texel size (downsample) or destination texel size (blur).
    texel: vec2<f32>,
    // Gaussian sigma in destination texels (blur pass only).
    sigma: f32,
    // 1.0 for the horizontal pass, 0.0 for the vertical pass.
    horizontal: f32,
    // Number of taps used (1..=13); tap 0 is the centre.
    taps: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
    // Gaussian weights. A uniform `array<f32>` has a 16-byte element stride, so
    // the CPU pads each weight into the first slot of a `vec4`.
    weights: array<vec4<f32>, 16>,
};

@group(0) @binding(0)
var source_tex: texture_2d<f32>;
@group(0) @binding(1)
var source_sampler: sampler;
@group(0) @binding(2)
var<uniform> params: BlurParams;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VsOut {
    // One oversized triangle covering clip space.
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let position = positions[index];
    var out: VsOut;
    out.clip_pos = vec4<f32>(position, 0.0, 1.0);
    // NDC y grows upwards, texture v grows downwards.
    out.uv = vec2<f32>(position.x * 0.5 + 0.5, 0.5 - position.y * 0.5);
    return out;
}

// 2x box downsample: the destination texel is the mean of the 2x2 source texels
// it covers. Sampling at the four source texel centres under a linear sampler
// makes this exact for even source sizes and clamps at the edges otherwise.
@fragment
fn fs_downsample(in: VsOut) -> @location(0) vec4<f32> {
    let half_texel = params.texel * 0.5;
    let top_left = textureSampleLevel(source_tex, source_sampler, in.uv + vec2<f32>(-half_texel.x, -half_texel.y), 0.0);
    let top_right = textureSampleLevel(source_tex, source_sampler, in.uv + vec2<f32>(half_texel.x, -half_texel.y), 0.0);
    let bottom_left = textureSampleLevel(source_tex, source_sampler, in.uv + vec2<f32>(-half_texel.x, half_texel.y), 0.0);
    let bottom_right = textureSampleLevel(source_tex, source_sampler, in.uv + vec2<f32>(half_texel.x, half_texel.y), 0.0);
    let sum = (top_left + top_right + bottom_left + bottom_right) * 0.25;
    return vec4<f32>(sum.rgb, 1.0);
}

// One axis of the separable Gaussian; taps are one destination texel apart.
@fragment
fn fs_blur(in: VsOut) -> @location(0) vec4<f32> {
    var direction: vec2<f32>;
    if params.horizontal > 0.5 {
        direction = vec2<f32>(params.texel.x, 0.0);
    } else {
        direction = vec2<f32>(0.0, params.texel.y);
    }

    let centre_weight = params.weights[0].x;
    var sum = textureSampleLevel(source_tex, source_sampler, in.uv, 0.0).rgb * centre_weight;
    var weight_sum = centre_weight;
    for (var tap = 1u; tap < params.taps; tap = tap + 1u) {
        let offset = direction * f32(tap);
        let weight = params.weights[tap].x;
        sum = sum
            + textureSampleLevel(source_tex, source_sampler, in.uv + offset, 0.0).rgb * weight
            + textureSampleLevel(source_tex, source_sampler, in.uv - offset, 0.0).rgb * weight;
        weight_sum = weight_sum + weight * 2.0;
    }
    return vec4<f32>(sum / weight_sum, 1.0);
}
