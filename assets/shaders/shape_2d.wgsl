struct CameraUniform {
    view_proj: mat4x4<f32>,
    resolution: vec2<f32>,
    padding: vec2<f32>,   // Explicitly matching the 80-byte Rust struct
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct VertexInput {
    @builtin(vertex_index) vertex_index: u32,
    @location(0) transform_c0: vec4<f32>,
    @location(1) transform_c1: vec4<f32>,
    @location(2) transform_c2: vec4<f32>,
    @location(3) transform_c3: vec4<f32>,
    @location(4) color: vec4<f32>,
    @location(5) params: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) local_pos: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) params: vec4<f32>,
    @location(3) world_pos: vec2<f32>,
    @location(4) local_size: vec2<f32>, 
    @location(5) world_pixel_size: f32,
    // Triangle SDF: the three world-space vertices, constant per instance.
    // Only meaningful when `params.w > 1.5` (SHAPE_KIND_TRIANGLE); the
    // rounded-rect path never reads them.  Marked `flat` so no interpolation
    // work is done for the legacy path.
    @location(6) @interpolate(flat) tri_a: vec2<f32>,
    @location(7) @interpolate(flat) tri_b: vec2<f32>,
    @location(8) @interpolate(flat) tri_c: vec2<f32>,
};

// Shape-kind flag lives in `params.w` (see ferrous_2d::render::types).
// 0.0/1.0 = legacy rounded-rect/circle family (ignored by the shader).
// 2.0     = triangle SDF, emitted only by ShapeInstance::triangle.
const SHAPE_KIND_TRIANGLE: f32 = 2.0;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    let transform = mat4x4<f32>(
        in.transform_c0,
        in.transform_c1,
        in.transform_c2,
        in.transform_c3
    );

    let local_size = vec2<f32>(
        length(transform[0].xyz),
        length(transform[1].xyz)
    );

    // --- High-Precision Pixel Size Calculation ---
    // view_proj[0][0] is (2.0 / width_in_world_units) in Ortho
    let res = max(camera.resolution, vec2<f32>(1.0, 1.0));
    let world_pixel_size = 2.0 / (abs(camera.view_proj[0][0]) * res.x);
    // Guard against degenerate instances: a zero axis in local_size would
    // divide by zero below.
    let safe_local_size = max(local_size, vec2<f32>(0.001));

    // Canonical quad corners in {0,1}^2.  The rounded-rect family re-centres
    // them to [-0.5, 0.5]^2 (so the transform's origin is the primitive
    // centre); the triangle family keeps them at the corner origin because
    // c3 = a and (0,0)/(1,0)/(0,1) map to a/b/c.
    let corner = vec2<f32>(
        f32(in.vertex_index & 1u),
        f32(in.vertex_index >> 1u)
    );

    let is_triangle = in.params.w > 1.5;

    var expanded_local_pos: vec2<f32>;
    if (is_triangle) {
        // Expand the quad about the triangle centroid (uv = 1/3, 1/3) by the
        // amount needed to keep the AA band inside the rasterised quad.  The
        // inradius is the per-edge distance scale from the centroid, so
        // `1 + pad/inradius` guarantees `pad` world units of margin on the
        // closest edge.  Capped at 2x, like the rounded-rect path.
        let centroid = vec2<f32>(1.0 / 3.0, 1.0 / 3.0);
        let c0 = transform[0].xy;
        let c1 = transform[1].xy;
        let area2 = abs(c0.x * c1.y - c0.y * c1.x);
        let perimeter = length(c0) + length(c1) + length(c1 - c0);
        let inradius = area2 / max(perimeter, 1e-3);
        let pad = world_pixel_size * 4.0;
        let expansion = 1.0 + pad / max(inradius, pad);
        expanded_local_pos = (corner - centroid) * expansion + centroid;
    } else {
        // Cap the expansion so the quad never grows more than 100% per axis.
        // Previously a 1px line expanded 9x in height, smearing AA into
        // neighboring geometry and creating halos on adjacent lines.
        let max_padding = safe_local_size * 0.5;
        let padding = min(vec2<f32>(world_pixel_size * 4.0), max_padding);
        let expansion_factor = 1.0 + (padding * 2.0 / safe_local_size);
        expanded_local_pos = (corner - vec2<f32>(0.5, 0.5)) * expansion_factor;
    }

    let world_pos_4 = transform * vec4<f32>(expanded_local_pos, 0.0, 1.0);
    
    var out: VertexOutput;
    out.clip_position = camera.view_proj * world_pos_4;
    out.local_pos = expanded_local_pos; 
    out.color = in.color;
    out.params = in.params;
    out.world_pos = world_pos_4.xy;
    out.local_size = local_size;
    out.world_pixel_size = world_pixel_size;
    // World-space triangle vertices (only consumed by the triangle branch).
    out.tri_a = transform[3].xy;
    out.tri_b = transform[3].xy + transform[0].xy;
    out.tri_c = transform[3].xy + transform[1].xy;
    
    return out;
}

// Standard signed distance to a triangle (Inigo Quilez), winding-agnostic
// (negative inside, positive outside, magnitude = Euclidean distance in the
// same units as `p`).  Because the sign is derived from the winding, filled
// triangles render two-sided.
fn sd_triangle(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>, c: vec2<f32>) -> f32 {
    let e0 = b - a;
    let e1 = c - b;
    let e2 = a - c;
    let v0 = p - a;
    let v1 = p - b;
    let v2 = p - c;
    let pq0 = v0 - e0 * clamp(dot(v0, e0) / max(dot(e0, e0), 1e-8), 0.0, 1.0);
    let pq1 = v1 - e1 * clamp(dot(v1, e1) / max(dot(e1, e1), 1e-8), 0.0, 1.0);
    let pq2 = v2 - e2 * clamp(dot(v2, e2) / max(dot(e2, e2), 1e-8), 0.0, 1.0);
    let s = sign(e0.x * e2.y - e0.y * e2.x);
    let d = min(
        min(
            vec2<f32>(dot(pq0, pq0), s * (v0.x * e0.y - v0.y * e0.x)),
            vec2<f32>(dot(pq1, pq1), s * (v1.x * e1.y - v1.y * e1.x)),
        ),
        vec2<f32>(dot(pq2, pq2), s * (v2.x * e2.y - v2.y * e2.x)),
    );
    return -sqrt(d.x) * sign(d.y);
}

// Winding sign of a triangle: +1 for CCW geometry, -1 for CW.  Used to
// normalise the per-edge inside tests / signed distances so the selective-AA
// path stays two-sided regardless of vertex winding.
fn tri_winding(a: vec2<f32>, b: vec2<f32>, c: vec2<f32>) -> f32 {
    return sign((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x));
}

// Signed distance to the clamped segment `u -> v`: negative inside the
// triangle, positive outside.  The clamp to the segment endpoints gives
// correct rounding where two marked edges meet at a vertex.
fn sd_edge_segment(p: vec2<f32>, u: vec2<f32>, v: vec2<f32>, w: f32) -> f32 {
    let e = v - u;
    let t = clamp(dot(p - u, e) / max(dot(e, e), 1e-8), 0.0, 1.0);
    let dist = length(p - (u + t * e));
    // `w * cross(e, p - u) > 0` on the inside side for both windings.
    let inside = w * (e.x * (p.y - u.y) - e.y * (p.x - u.x));
    return select(dist, -dist, inside > 0.0);
}

// Half-plane inside test for a hard (non-AA) edge.  `>= 0` includes a point
// that lands exactly on a shared internal edge so neighbouring triangles do
// not both discard it (a one-pixel crack).  At the seam both triangles then
// emit full coverage; that overlap is invisible for opaque fills and is the
// standard tessellation trade-off.
fn edge_inside(p: vec2<f32>, u: vec2<f32>, v: vec2<f32>, w: f32) -> bool {
    let e = v - u;
    return (w * (e.x * (p.y - u.y) - e.y * (p.x - u.x))) >= 0.0;
}

// Selective-AA triangle coverage: hard-clip every unmarked edge, then fade
// only near the marked edges.  The nearest marked feature wins, so two marked
// edges meeting at a vertex round the corner correctly.
fn triangle_masked_alpha(
    p: vec2<f32>,
    a: vec2<f32>,
    b: vec2<f32>,
    c: vec2<f32>,
    mask: u32,
    filter_width: f32,
) -> f32 {
    let w = tri_winding(a, b, c);
    if (w == 0.0) {
        return 0.0;
    }
    // Hard clipping against internal (bit-clear) edges.
    if ((mask & 1u) == 0u && !edge_inside(p, a, b, w)) { return 0.0; }
    if ((mask & 2u) == 0u && !edge_inside(p, b, c, w)) { return 0.0; }
    if ((mask & 4u) == 0u && !edge_inside(p, c, a, w)) { return 0.0; }

    // Nearest marked edge governs the AA band.
    var best_abs = -1.0;
    var best = 0.0;
    if ((mask & 1u) != 0u) {
        best = sd_edge_segment(p, a, b, w);
        best_abs = abs(best);
    }
    if ((mask & 2u) != 0u) {
        let s = sd_edge_segment(p, b, c, w);
        if (best_abs < 0.0 || abs(s) < best_abs) { best = s; best_abs = abs(s); }
    }
    if ((mask & 4u) != 0u) {
        let s = sd_edge_segment(p, c, a, w);
        if (best_abs < 0.0 || abs(s) < best_abs) { best = s; best_abs = abs(s); }
    }
    if (best_abs < 0.0) {
        // No marked edges: hard fill (fully covered inside the triangle).
        return 1.0;
    }
    return 1.0 - smoothstep(-filter_width, filter_width, best);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // --- Triangle path (additive; selected only by params.w == 2.0) ---
    if (in.params.w > 1.5) {
        let smoothing = max(in.params.z, 0.5);
        let filter_width = max(in.world_pixel_size * smoothing, 1e-6);
        // params.x carries the per-edge AA mask (see ShapeInstance::
        // polygon_triangle).  0b111 uses the exact Euclidean triangle SDF
        // (unchanged from the original primitive); any other mask uses the
        // selective per-edge path so shared internal edges stay hard.
        let mask = u32(in.params.x + 0.5);
        var alpha: f32;
        if (mask >= 7u) {
            let d = sd_triangle(in.world_pos, in.tri_a, in.tri_b, in.tri_c);
            alpha = 1.0 - smoothstep(-filter_width, filter_width, d);
        } else {
            alpha = triangle_masked_alpha(
                in.world_pos, in.tri_a, in.tri_b, in.tri_c, mask, filter_width,
            );
        }
        if (alpha < 0.01) {
            discard;
        }
        return vec4<f32>(in.color.rgb, in.color.a * alpha);
    }

    // --- Rounded-rectangle / circle path (unchanged) ---
    let local_size = in.local_size;
    let p = in.local_pos * local_size;
    
    // Exact Euclidean Distance to rounded rectangle
    let radius = in.params.y;
    let q = abs(p) - (local_size * 0.5) + vec2<f32>(radius);
    let d = min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - radius;

    // --- Anti-Aliasing (Stable Pre-calculated) ---
    // We use the pixel size calculated in VS for stability, 
    // especially on diagonals where dpdx fluctuates.
    let grad_len = in.world_pixel_size;
    
    // Pixel-scale stabilization:
    // Ensure the line is always at least 1.5 pixels thick.
    let target_pixel_width = 1.8; // Slightly thicker for better stability on diagonals
    let min_world_width = grad_len * target_pixel_width;
    let actual_width = in.params.x; 
    
    var final_d = d;
    var alpha_multiplier = 1.0;
    
    // Only run the thin-line stabilization for stroked/bordered primitives
    // (actual_width > 0). Filled shapes have params.x == 0; without this guard
    // they hit the branch below and get alpha_multiplier = 0.2 → rendered at
    // 20% opacity.
    if (actual_width > 0.0 && actual_width < min_world_width) {
        let expansion = (min_world_width - actual_width) * 0.5;
        final_d -= expansion;
        alpha_multiplier = clamp(actual_width / min_world_width, 0.2, 1.0);
    }

    // Smoothing range (usually 1-2 pixels)
    let smoothing = in.params.z; 
    let filter_width = grad_len * smoothing;
    
    // Main shape alpha
    var alpha = 1.0 - smoothstep(-filter_width, filter_width, final_d);

    let final_alpha = alpha * alpha_multiplier;

    if (final_alpha < 0.01) {
        discard;
    }

    return vec4<f32>(in.color.rgb, in.color.a * final_alpha);
}
