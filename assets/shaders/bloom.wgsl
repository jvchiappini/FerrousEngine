// Bloom downsample/upsample shaders.
// Generates a fullscreen triangle exactly like post.wgsl.

@group(0) @binding(0) var t_input: texture_2d<f32>;
@group(0) @binding(1) var s_input: sampler;

// push constant used by downsample pass only.  threshold of 0 disables it.

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) id: u32) -> VsOut {
    let raw_uv = vec2<f32>(f32((id << 1u) & 2u), f32(id & 2u));
    var out: VsOut;
    out.uv = vec2<f32>(raw_uv.x, 1.0 - raw_uv.y);
    out.pos = vec4<f32>(raw_uv * 2.0 - 1.0, 0.0, 1.0);
    return out;
}

// regular downsample: simply box-filter and return the result.  used for
// every level except the first.
@fragment
fn fs_downsample(in: VsOut) -> @location(0) vec4<f32> {
    let dim = vec2<f32>(textureDimensions(t_input, 0));
    let inv = 1.0 / dim;
    var sum = vec4<f32>(0.0);
    for (var y: i32 = -1; y <= 1; y = y + 1) {
        for (var x: i32 = -1; x <= 1; x = x + 1) {
            let offset = vec2<f32>(f32(x), f32(y)) * inv;
            sum += textureSample(t_input, s_input, in.uv + offset);
        }
    }
    return sum / 9.0;
}

// initial downsample pass that applies a fixed brightness threshold of 1.0
// after filtering.  This isolates extremely bright pixels (light sources
// etc.) and prevents mid-range values from bleeding into the bloom chain.
@fragment
fn fs_downsample_threshold(in: VsOut) -> @location(0) vec4<f32> {
    let dim = vec2<f32>(textureDimensions(t_input, 0));
    let inv = 1.0 / dim;
    var sum = vec4<f32>(0.0);
    for (var y: i32 = -1; y <= 1; y = y + 1) {
        for (var x: i32 = -1; x <= 1; x = x + 1) {
            let offset = vec2<f32>(f32(x), f32(y)) * inv;
            sum += textureSample(t_input, s_input, in.uv + offset);
        }
    }
    let color = sum / 9.0;
    let mx = max(color.r, max(color.g, color.b));
    if (mx < 1.0) {
        return vec4<f32>(0.0);
    }
    return color;
}

// Upsample pass with a 3×3 tent filter.  Weights form a bilinear tent:
//   1 2 1
//   2 4 2  / 16
//   1 2 1
// This produces a smooth gaussian-like blur that removes the hard ring
// artifacts that appear when using a simple point sample during upsample.
// Additive blending into the destination is handled by the pipeline state.
@fragment
fn fs_upsample(in: VsOut) -> @location(0) vec4<f32> {
    let dim = vec2<f32>(textureDimensions(t_input, 0));
    let inv = 1.0 / dim;
    var c = vec4<f32>(0.0);
    // corners × 1
    c += textureSample(t_input, s_input, in.uv + vec2<f32>(-inv.x, -inv.y));
    c += textureSample(t_input, s_input, in.uv + vec2<f32>( inv.x, -inv.y));
    c += textureSample(t_input, s_input, in.uv + vec2<f32>(-inv.x,  inv.y));
    c += textureSample(t_input, s_input, in.uv + vec2<f32>( inv.x,  inv.y));
    // edges × 2
    c += textureSample(t_input, s_input, in.uv + vec2<f32>(    0.0, -inv.y)) * 2.0;
    c += textureSample(t_input, s_input, in.uv + vec2<f32>(    0.0,  inv.y)) * 2.0;
    c += textureSample(t_input, s_input, in.uv + vec2<f32>(-inv.x,     0.0)) * 2.0;
    c += textureSample(t_input, s_input, in.uv + vec2<f32>( inv.x,     0.0)) * 2.0;
    // center × 4
    c += textureSample(t_input, s_input, in.uv) * 4.0;
    return c / 16.0;
}
