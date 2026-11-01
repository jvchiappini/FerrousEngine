// ─── Pure2D MSDF text shader ────────────────────────────────────────────────
//
// One `TextInstance` per glyph quad.  The instance's model matrix maps the
// canonical local quad `[0, 1]²` (Y up) to the glyph rectangle in world units,
// so rotation / non-uniform scale / parent transforms compose exactly like
// shapes and sprites.
//
// The MSDF decode helpers live in `msdf_common.wgsl`, prepended to this module
// by the pipeline.  The fragment writes premultiplied linear colour
// (`vec4(rgb * a, a)`), matching the shape and sprite pipelines.

struct CameraUniform {
    view_proj: mat4x4<f32>,
    resolution: vec2<f32>,
    // x = device pixels per logical pixel, y = legacy flag (unused here).
    padding: vec2<f32>,
    // Physical-pixel viewport origin (0,0 for the single main camera).
    viewport_origin: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

// Font MSDF atlas at group 1 (same binding shape as the sprite texture).
@group(1) @binding(0)
var font_tex: texture_2d<f32>;
@group(1) @binding(1)
var font_sampler: sampler;

struct InstanceInput {
    @location(0) transform_c0: vec4<f32>,
    @location(1) transform_c1: vec4<f32>,
    @location(2) transform_c2: vec4<f32>,
    @location(3) transform_c3: vec4<f32>,
    @location(4) color: vec4<f32>,
    @location(5) uv_rect: vec4<f32>,
    // x = encoded MSDF range in atlas texels.
    @location(6) params: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    // Flat: atlas cell rectangle (needed for the screen px range).
    @location(2) @interpolate(flat) uv0: vec2<f32>,
    @location(3) @interpolate(flat) uv1: vec2<f32>,
    // Flat: quad size in world units and the encoded px range.
    @location(4) @interpolate(flat) quad_world: vec2<f32>,
    @location(5) @interpolate(flat) px_range: f32,
};

@vertex
fn vs_main(
    @builtin(vertex_index) v_idx: u32,
    instance: InstanceInput,
) -> VertexOutput {
    // Triangle strip over the unit square.
    let local = vec2<f32>(f32(v_idx & 1u), f32(v_idx >> 1u));

    let model = mat4x4<f32>(
        instance.transform_c0,
        instance.transform_c1,
        instance.transform_c2,
        instance.transform_c3,
    );
    let world = model * vec4<f32>(local, 0.0, 1.0);

    // Atlas v = 0 is the top row of the cell (em +y), so the quad's top edge
    // samples uv0.y.  X follows the usual left-to-right UV span.
    let uv0 = instance.uv_rect.xy;
    let uv1 = instance.uv_rect.zw;
    let uv = vec2<f32>(
        mix(uv0.x, uv1.x, local.x),
        mix(uv1.y, uv0.y, local.y),
    );

    var out: VertexOutput;
    out.clip_position = camera.view_proj * world;
    out.uv = uv;
    out.color = instance.color;
    out.uv0 = uv0;
    out.uv1 = uv1;
    out.quad_world = vec2<f32>(
        length(instance.transform_c0.xy),
        length(instance.transform_c1.xy),
    );
    out.px_range = max(instance.params.x, 1.0);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let sample = textureSample(font_tex, font_sampler, in.uv);
    let sig_dist = msdf_median(sample.r, sample.g, sample.b);

    let dims = textureDimensions(font_tex, 0);
    let atlas_dim = vec2<f32>(f32(dims.x), f32(dims.y));
    let cell = msdf_cell_texels(in.uv0, in.uv1, atlas_dim);

    // World units per device pixel, derived from the ortho projection exactly
    // like `shape_2d.wgsl`.  `quad_world / world_px` is the quad size in
    // screen pixels, which feeds Chlumsky's screen px range.
    let res = max(camera.resolution, vec2<f32>(1.0, 1.0));
    let world_px = 2.0 / (abs(camera.view_proj[0][0]) * res.x);
    let quad_px = in.quad_world / max(world_px, 1e-9);
    let screen_px_range = msdf_screen_px_range(quad_px, cell, in.px_range);
    let opacity = msdf_opacity(sig_dist, screen_px_range);
    let alpha = in.color.a * opacity;

    return vec4<f32>(in.color.rgb * alpha, alpha);
}
