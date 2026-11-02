// Full-target composite of a finished offscreen Pure2D layer with a
// backdrop-aware blend mode (overlay, darken, lighten).
//
// Unlike `composite_2d.wgsl`, this pass cannot rely on fixed-function blend
// factors: the result depends on the destination colour, so the host binds a
// snapshot of the destination as `backdrop_tex` and the pipeline writes with
// blending disabled.
//
// Both textures are linear and premultiplied; the layer is first faded by the
// group opacity.  With straight colours `s` (source, the layer) and `b`
// (backdrop), coverage `As`/`Ad`:
//
//   Co = Cs*(1-Ad) + As*Ad*B(b,s) + Cd*(1-As)
//   Ao = As + Ad*(1-As)
//
// `B` is selected by the instance's `mode` (4 = overlay, 5 = darken,
// 6 = lighten).  The overlay condition is evaluated on the source, matching the
// engine's blend contract.

struct InstanceInput {
    @location(0) opacity: f32,
    @location(1) mode: u32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) opacity: f32,
    @location(2) @interpolate(flat) mode: u32,
};

@group(0) @binding(0) var layer_tex: texture_2d<f32>;
@group(0) @binding(1) var layer_sampler: sampler;
@group(0) @binding(2) var backdrop_tex: texture_2d<f32>;
@group(0) @binding(3) var backdrop_sampler: sampler;

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
    out.mode = instance.mode;
    return out;
}

fn overlay_channel(b: f32, s: f32) -> f32 {
    if (s < 0.5) {
        return 2.0 * b * s;
    }
    return 1.0 - 2.0 * (1.0 - b) * (1.0 - s);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let opacity = clamp(in.opacity, 0.0, 1.0);
    let layer = textureSample(layer_tex, layer_sampler, in.uv) * opacity;
    let backdrop = textureSample(backdrop_tex, backdrop_sampler, in.uv);

    let source_alpha = layer.a;
    let backdrop_alpha = backdrop.a;
    let cs = select(
        vec3<f32>(0.0),
        layer.rgb / max(source_alpha, 1e-6),
        source_alpha > 0.0,
    );
    let cb = select(
        vec3<f32>(0.0),
        backdrop.rgb / max(backdrop_alpha, 1e-6),
        backdrop_alpha > 0.0,
    );

    var blended: vec3<f32>;
    if (in.mode == 4u) {
        blended = vec3<f32>(
            overlay_channel(cb.r, cs.r),
            overlay_channel(cb.g, cs.g),
            overlay_channel(cb.b, cs.b),
        );
    } else if (in.mode == 5u) {
        blended = min(cb, cs);
    } else {
        blended = max(cb, cs);
    }

    let colour = layer.rgb * (1.0 - backdrop_alpha)
        + blended * (source_alpha * backdrop_alpha)
        + backdrop.rgb * (1.0 - source_alpha);
    let alpha = source_alpha + backdrop_alpha * (1.0 - source_alpha);
    return vec4<f32>(colour, alpha);
}
