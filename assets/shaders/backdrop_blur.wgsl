// Separable backdrop blur for the GUI.
//
// Two fullscreen passes blur the scene target at quarter resolution:
//   1. horizontal: sample the full-resolution scene into `blur_h`;
//   2. vertical: sample `blur_h` into `blur_v`.
// The GUI quad pipeline samples `blur_v` with a linear sampler, so the
// quarter-resolution result is upsampled for free when the backdrop rect is
// drawn. The cost is bounded by the quarter-resolution target and a fixed
// nine-tap kernel, independent of the requested blur radius.

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

struct BlurParams {
    // Texel size of the destination (quarter-resolution) target.
    texel: vec2<f32>,
    // Gaussian radius in destination texels.
    radius: f32,
    // 1.0 for the horizontal pass, 0.0 for the vertical pass.
    horizontal: f32,
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

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let weights = array<f32, 5>(0.227027, 0.1945946, 0.1216216, 0.054054, 0.016216);
    var direction: vec2<f32>;
    if params.horizontal > 0.5 {
        direction = vec2<f32>(params.texel.x, 0.0);
    } else {
        direction = vec2<f32>(0.0, params.texel.y);
    }
    let delta = direction * max(params.radius, 0.0);

    var sum = textureSampleLevel(source_tex, source_sampler, in.uv, 0.0) * weights[0];
    var weight_sum = weights[0];
    for (var tap = 1u; tap < 5u; tap = tap + 1u) {
        let offset = delta * f32(tap);
        let weight = weights[tap];
        sum = sum + textureSampleLevel(source_tex, source_sampler, in.uv + offset, 0.0) * weight;
        sum = sum + textureSampleLevel(source_tex, source_sampler, in.uv - offset, 0.0) * weight;
        weight_sum = weight_sum + weight * 2.0;
    }
    return sum / weight_sum;
}
