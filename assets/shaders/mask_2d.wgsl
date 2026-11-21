// Multiplies a finished Pure2D layer by a mask source's coverage.
//
// Both textures are linear and premultiplied.  With content `c` and mask `m`
// (the mask source's offscreen layer, cleared to transparent black):
//
//   coverage = mode == 0 ? m.a : dot(m.rgb, vec3(0.2126, 0.7152, 0.0722))
//   coverage = invert != 0 ? 1 - coverage : coverage
//   out      = c * coverage
//
// The luma mode reads the premultiplied mask colour, so a fully transparent
// mask pixel has luma 0 and (inverted) becomes fully visible.  Scaling all
// four channels is the correct premultiplied alpha multiply: the result stays
// premultiplied and the layer's group opacity and blend mode apply afterwards.
//
// Uniform layout (std140):
//   mode:   f32   offset 0  (0 = alpha, 1 = luma)
//   invert: f32   offset 4  (1 = inverted)
//   _pad:   vec2  offset 8

struct MaskParams {
    mode: f32,
    invert: f32,
    _pad: vec2<f32>,
};

@group(0) @binding(0) var content_tex: texture_2d<f32>;
@group(0) @binding(1) var content_sampler: sampler;
@group(0) @binding(2) var mask_tex: texture_2d<f32>;
@group(0) @binding(3) var mask_sampler: sampler;
@group(0) @binding(4) var<uniform> params: MaskParams;

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

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let content = textureSample(content_tex, content_sampler, in.uv);
    let mask = textureSample(mask_tex, mask_sampler, in.uv);

    var coverage = mask.a;
    if (params.mode > 0.5) {
        coverage = dot(mask.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    }
    coverage = clamp(coverage, 0.0, 1.0);
    if (params.invert > 0.5) {
        coverage = 1.0 - coverage;
    }
    return content * coverage;
}
