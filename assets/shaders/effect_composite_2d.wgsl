// Combines a layer image with a blurred copy of itself to add a drop shadow or
// a glow.
//
// Inputs are linear and premultiplied.  `content_tex` is the layer as it was
// before this effect; `blur_tex` is its Gaussian blur.  The host offsets the
// blur sample by `params.offset` pixels (Y down) and tints it with
// `params.color`.
//
// mode 0 (drop shadow): the tinted blur goes *behind* the content:
//     shadow.rgb = color.rgb * color.a * strength * blur.a
//     shadow.a   = color.a * strength * blur.a
//     out        = shadow + content * (1 - shadow.a)
//
// mode 1 (glow): the tinted blur is *added* as light around the content:
//     glow.rgb   = color.rgb * color.a * strength * blur.a
//     out        = content + vec4(glow.rgb, 0)
//
// `strength` is the effect opacity (shadow) or intensity (glow), clamped to
// `0..=1` for shadows; glows may exceed 1 because the layer target is HDR
// (`Rgba16Float`) and the final composite clamps when it reaches the target.

struct EffectParams {
    color: vec4<f32>,
    offset: vec2<f32>,
    mode: f32,
    strength: f32,
};

@group(0) @binding(0) var content_tex: texture_2d<f32>;
@group(0) @binding(1) var content_sampler: sampler;
@group(0) @binding(2) var blur_tex: texture_2d<f32>;
@group(0) @binding(3) var blur_sampler: sampler;
@group(0) @binding(4) var<uniform> params: EffectParams;

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
    let size = vec2<f32>(textureDimensions(blur_tex));
    let uv = (in.uv * size - params.offset) / max(size, vec2<f32>(1.0));
    let blurred = textureSample(blur_tex, blur_sampler, uv);

    let tint = params.color;
    let coverage = blurred.a * tint.a * params.strength;
    if (params.mode < 0.5) {
        // Drop shadow behind the content.
        let shadow = vec4<f32>(tint.rgb * coverage, coverage);
        return shadow + content * (1.0 - shadow.a);
    }
    // Glow: additive light, no extra coverage.
    return content + vec4<f32>(tint.rgb * coverage, 0.0);
}
