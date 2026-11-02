// Separable Gaussian blur of a full-target layer texture.
//
// One pass per direction: the host binds `direction` = (1, 0) for the horizontal
// pass and (0, 1) for the vertical pass.  The kernel comes from
// `ferrous_2d::render::BlurKernel2d`: `taps` one-sided samples (centre included)
// at one-pixel spacing, normalized so the weights sum to 1.  The texture is
// linear and premultiplied, and so is the output.
//
// Uniform layout (std140):
//   direction: vec2<f32>        offset 0
//   taps:      f32              offset 8
//   radius:    f32 (unused)     offset 12
//   weights:   array<vec4, 12>  offset 16   (48 scalar weights, packed 4 per vec4)

struct BlurParams {
    direction: vec2<f32>,
    taps: f32,
    radius: f32,
    weights: array<vec4<f32>, 12>,
};

@group(0) @binding(0) var source_tex: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;
@group(0) @binding(2) var<uniform> params: BlurParams;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var positions: array<vec2<f32>, 3> = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let p = positions[vertex_index];
    var out: VertexOutput;
    out.clip_position = vec4<f32>(p, 0.0, 1.0);
    out.uv = vec2<f32>((p.x + 1.0) * 0.5, (1.0 - p.y) * 0.5);
    return out;
}

fn weight_at(index: u32) -> f32 {
    let slot = params.weights[index / 4u];
    switch (index % 4u) {
        case 0u: { return slot.x; }
        case 1u: { return slot.y; }
        case 2u: { return slot.z; }
        default: { return slot.w; }
    }
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let size = vec2<f32>(textureDimensions(source_tex));
    let step = params.direction / max(size, vec2<f32>(1.0));
    let taps = u32(max(params.taps, 1.0));

    var colour = textureSample(source_tex, source_sampler, in.uv) * weight_at(0u);
    for (var i: u32 = 1u; i < taps; i = i + 1u) {
        let offset = step * f32(i);
        let weight = weight_at(i);
        colour = colour
            + textureSample(source_tex, source_sampler, in.uv + offset) * weight
            + textureSample(source_tex, source_sampler, in.uv - offset) * weight;
    }
    return colour;
}
