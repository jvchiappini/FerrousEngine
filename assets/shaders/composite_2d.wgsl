// Full-target composite of a finished offscreen Pure2D layer.
//
// The layer texture is linear and premultiplied (Rgba16Float, or the target
// format as a fallback).  The fragment multiplies all four channels by the
// layer's group opacity, so the output stays premultiplied and the pipeline's
// premultiplied blend state composites it into the parent target.  UVs map the
// full-screen triangle 1:1 onto the layer texture (row 0 = top).

struct InstanceInput {
    @location(0) opacity: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) opacity: f32,
};

@group(0) @binding(0) var layer_tex: texture_2d<f32>;
@group(0) @binding(1) var layer_sampler: sampler;

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
    out.opacity = instance.opacity;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let colour = textureSample(layer_tex, layer_sampler, in.uv);
    let opacity = clamp(in.opacity, 0.0, 1.0);
    return vec4<f32>(colour.rgb * opacity, colour.a * opacity);
}
