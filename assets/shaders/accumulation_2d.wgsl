// Full-target weighted accumulation of one sub-frame.
//
// The source is a finished render target.  When the source format is sRGB,
// `textureSample` decodes it to linear light, so the accumulation target
// (`Rgba16Float`) stores premultiplied **linear** values.  The instance weight
// scales every channel; the pipeline blend state decides whether the result is
// added to the accumulator or replaces the resolve target.

struct InstanceInput {
    @location(0) weight: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) weight: f32,
};

@group(0) @binding(0) var source_tex: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    instance: InstanceInput,
) -> VertexOutput {
    var positions: array<vec2<f32>, 3> = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    let p = positions[vertex_index];
    var out: VertexOutput;
    out.clip_position = vec4<f32>(p, 0.0, 1.0);
    out.uv = vec2<f32>((p.x + 1.0) * 0.5, (1.0 - p.y) * 0.5);
    out.weight = instance.weight;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(source_tex, source_sampler, in.uv) * in.weight;
}
