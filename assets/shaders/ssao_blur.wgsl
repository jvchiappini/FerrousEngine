// SSAO Bilateral Blur
//
// A two-pass (horizontal + vertical) box blur that respects depth
// discontinuities.  Samples whose depth differs too much from the
// centre pixel are excluded so that occlusion doesn't bleed across
// hard geometric edges.

struct BlurParams {
    // Texel size of the SSAO texture (1/width, 1/height)
    texel_size   : vec2<f32>,
    // 0 = horizontal pass, 1 = vertical pass
    direction    : u32,
    // Depth threshold: samples with |Δdepth| > this are excluded
    depth_thresh : f32,
};

@group(0) @binding(0)
var<uniform> params: BlurParams;

// Raw (or intermediate) SSAO occlusion
@group(1) @binding(0)
var ssao_tex     : texture_2d<f32>;
@group(1) @binding(1)
var ssao_sampler : sampler;

// Normal+depth texture from prepass (to gate bilateral weights)
@group(1) @binding(2)
var nd_tex     : texture_2d<f32>;
@group(1) @binding(3)
var nd_sampler : sampler;

// ── Fullscreen triangle ───────────────────────────────────────────────────────

struct VsOut {
    @builtin(position) pos : vec4<f32>,
    @location(0)       uv  : vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vid: u32) -> VsOut {
    var out: VsOut;
    let x = f32((vid << 1u) & 2u);
    let y = f32(vid & 2u);
    out.uv  = vec2<f32>(x * 0.5, y * 0.5);
    out.pos = vec4<f32>(x - 1.0, 1.0 - y, 0.0, 1.0);
    return out;
}

// ── Fragment shader ───────────────────────────────────────────────────────────

struct FsOut {
    @location(0) occlusion : f32,
};

@fragment
fn fs_main(in: VsOut) -> FsOut {
    let uv = in.uv;

    // Centre pixel depth
    let centre_depth = textureSample(nd_tex, nd_sampler, uv).a;

    // Step direction in UV space
    var step = vec2<f32>(0.0, 0.0);
    if (params.direction == 0u) {
        step = vec2<f32>(params.texel_size.x, 0.0);
    } else {
        step = vec2<f32>(0.0, params.texel_size.y);
    }

    // 5-tap bilateral blur (radius 2)
    var result = 0.0;
    var weight_sum = 0.0;

    for (var i: i32 = -2; i <= 2; i = i + 1) {
        let offset_uv   = uv + step * f32(i);
        let sample_ao   = textureSample(ssao_tex, ssao_sampler, offset_uv).r;
        let sample_depth = textureSample(nd_tex, nd_sampler, offset_uv).a;

        // Bilateral weight: full weight if depth is similar, zero if too different
        let depth_diff = abs(sample_depth - centre_depth);
        let w = select(0.0, 1.0, depth_diff < params.depth_thresh);

        result     += sample_ao * w;
        weight_sum += w;
    }

    // Avoid divide by zero (all neighbours rejected → keep centre value)
    let final_ao = select(
        textureSample(ssao_tex, ssao_sampler, uv).r,
        result / weight_sum,
        weight_sum > 0.0001
    );

    var out: FsOut;
    out.occlusion = final_ao;
    return out;
}
