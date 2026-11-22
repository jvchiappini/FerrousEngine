// Shared rounded-clip mask for the GUI quad, text/icon and SVG pipelines.
//
// The clip is a rounded rectangle in framebuffer pixels, delivered per draw
// segment through a dynamically-offset uniform at group 2 binding 0. The
// fragment stage evaluates the rounded-rectangle signed distance field at the
// fragment's framebuffer position and turns it into an alpha mask, so quads,
// text runs and images inside a rounded clip are cut exactly at the corner
// instead of at the rectangular scissor bounds.

struct SegmentUniforms {
    // x, y, width, height of the clip rectangle in framebuffer pixels.
    clip_rect: vec4<f32>,
    // Per-corner radii: top-left, top-right, bottom-left, bottom-right.
    clip_radii: vec4<f32>,
    // x: 1 when the segment carries a rounded clip, 0 otherwise.
    // y: 1 when a blurred backdrop texture is bound, 0 for the tint fallback.
    clip_params: vec4<f32>,
};

@group(2) @binding(0)
var<uniform> segment: SegmentUniforms;

fn clip_sdf(p: vec2<f32>, b: vec2<f32>, r: vec4<f32>) -> f32 {
    var radius: f32;
    if p.x <= 0.0 && p.y <= 0.0 { radius = r.x; }
    else if p.x > 0.0 && p.y <= 0.0 { radius = r.y; }
    else if p.x <= 0.0 && p.y > 0.0 { radius = r.z; }
    else { radius = r.w; }
    let q = abs(p) - b + vec2<f32>(radius);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - radius;
}

/// 1 inside the rounded clip, 0 outside, with a one-pixel anti-aliased band.
fn clip_coverage(pixel: vec2<f32>) -> f32 {
    if segment.clip_params.x < 0.5 {
        return 1.0;
    }
    let center = segment.clip_rect.xy + segment.clip_rect.zw * 0.5;
    let distance = clip_sdf(pixel - center, segment.clip_rect.zw * 0.5, segment.clip_radii);
    return 1.0 - smoothstep(-0.5, 0.5, distance);
}
