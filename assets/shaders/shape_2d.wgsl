// 2D SDF shapes: rounded rect / circle / line family + filled triangles.
//
// Instance layout = `ferrous_2d::render::ShapeGpu` (64 bytes):
//   c0, c1, c3 : 2D affine model (x basis, y basis, translation)
//   dims       : (border_thickness, corner_radius)
//   color      : linear RGBA (f16x4)
//   flags      : bits 0..15 min line width (logical px, 8.8 fixed),
//                16 triangle, 18..20 triangle edge-AA mask,
//                21/22 hard start/end cap, 23 stroked line,
//                24 joined segment, 25/26 round join start/end,
//                28..31 dash pattern index.
//
// Everything that is constant per instance is passed `flat`, so only one
// vec4 (local_pos.xy, world_pos.xy) is interpolated per fragment.

struct CameraUniform {
    view_proj: mat4x4<f32>,
    resolution: vec2<f32>,
    // x = device pixels per logical pixel (resolves min_px to world units),
    // y = legacy line-stabiliser A/B flag (>0.5 = old behaviour).
    padding: vec2<f32>,
    // Physical-pixel origin of this viewport inside the framebuffer.  The
    // single main camera uses (0, 0); the multi-view path passes the sub-rect
    // origin because `@builtin(position)` is always in full framebuffer space.
    viewport_origin: vec2<f32>,
    // Keeps `dash` (a vec4 array) 16-byte aligned.
    _pad: vec2<f32>,
    // 16 dash patterns Ã— 8 lengths (two vec4 each); pattern 0 is unused.
    dash: array<vec4<f32>, 32>,
    // Gradient table (D-0030): headers, geometry and stops (512 vec4).
    gradients: array<vec4<f32>, 512>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

const FLAG_TRIANGLE: u32 = 65536u;      // 1 << 16
const MASK_SHIFT: u32 = 18u;
const FLAG_CAP_START: u32 = 2097152u;   // 1 << 21
const FLAG_CAP_END: u32 = 4194304u;     // 1 << 22
const FLAG_LINE: u32 = 8388608u;        // 1 << 23
const FLAG_RING: u32 = 131072u;          // 1 << 17
const FLAG_JOINED: u32 = 16777216u;      // 1 << 24
const FLAG_JOIN_ROUND_START: u32 = 33554432u; // 1 << 25
const FLAG_JOIN_ROUND_END: u32 = 67108864u;   // 1 << 26
const FLAG_ARC: u32 = 134217728u;        // 1 << 27
const FLAG_DASH_PX: u32 = 262144u;       // 1 << 18
const DASH_SHIFT: u32 = 28u;
const DASH_MASK: u32 = 15u;
const MIN_PX_MASK: u32 = 65535u;
const MAX_MIN_PX: f32 = 8.0;

// Rasteriser subpixel grid for the hard-edge fill rule.  D3D12, Vulkan and
// Metal flatten clip coordinates to framebuffer coordinates with at least 8
// fractional bits (Metal guarantees 8; D3D/Vulkan use a fixed 1/256 grid), so
// snapping triangle endpoints to 1/256 reproduces the hardware's own edge
// positions exactly.  Chosen as the minimum guaranteed precision on purpose.
const SUBPIXEL: f32 = 256.0;

// 1-pixel box filter: coverage goes from 1 at half a pixel inside the edge to
// 0 at half a pixel outside. `d` and `px` are both in world units.
fn coverage(d: f32, px: f32) -> f32 {
    return clamp(0.5 - d / max(px, 1e-9), 0.0, 1.0);
}

// Premultiplied fragment output. `ShapeInstance::color` stays straight linear
// RGBA in the public API; every Pure2D pipeline blends premultiplied.
fn premultiply(color: vec4<f32>, alpha: f32) -> vec4<f32> {
    let a = color.a * alpha;
    return vec4<f32>(color.rgb * a, a);
}

// How far the quad must extend past an endpoint so it covers the join/cap:
// miter wedge `half * |tan(turn/2)|` (capped at the miter limit 4), a half
// width for round/square caps or round joins, zero for a flat cap.
fn endpoint_ext(
    n: vec2<f32>,
    dir: vec2<f32>,
    half: f32,
    round_join: bool,
    is_cap: bool,
    cap_style: f32,
) -> f32 {
    if (is_cap) {
        return select(0.0, half, cap_style > 0.5);
    }
    if (round_join) {
        return half;
    }
    let c = clamp(dot(normalize(n), dir), 0.25, 1.0);
    return half * sqrt(max(1.0 - c * c, 0.0)) / c;
}

struct VertexInput {
    @builtin(vertex_index) vertex_index: u32,
    @location(0) c0: vec2<f32>,
    @location(1) c1: vec2<f32>,
    @location(2) c3: vec2<f32>,
    @location(3) dims: vec2<f32>,
    @location(4) color: vec4<f32>,
    @location(5) flags: u32,
    @location(6) ext0: vec4<f32>,
    @location(7) ext1: f32,
    @location(8) gradient: u32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    // xy = local position (rect family, in [-0.5,0.5]-scaled units),
    // zw = world position (triangle family).
    @location(0) pos: vec4<f32>,
    @location(1) @interpolate(flat) color: vec4<f32>,
    // Rect family: (local_size.x, local_size.y, border, radius).
    @location(2) @interpolate(flat) shape: vec4<f32>,
    // x = AA filter width (world units), y = world units per pixel.
    @location(3) @interpolate(flat) aa: vec2<f32>,
    @location(4) @interpolate(flat) flags: u32,
    // Triangle family: world-space vertices a, b (xy, zw) and c.
    // Joined segment: a, b in tri_ab and (s0, cap_style) in tri_c.
    @location(5) @interpolate(flat) tri_ab: vec4<f32>,
    @location(6) @interpolate(flat) tri_c: vec2<f32>,
    // Joined segment: (n_start.xy, n_end.xy) joint plane normals.
    @location(7) @interpolate(flat) seg_planes: vec4<f32>,
    // Triangle family: the three snapped framebuffer-space vertices used by the
    // exact hard-edge fill rule. `tri_fb_ab` = (a.xy, b.xy), `tri_fb_c` =
    // (c.xy, framebuffer winding sign, 0).
    @location(8) @interpolate(flat) tri_fb_ab: vec4<f32>,
    @location(9) @interpolate(flat) tri_fb_c: vec4<f32>,
    @location(10) @interpolate(flat) gradient: u32,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    let local_size = vec2<f32>(length(in.c0), length(in.c1));

    // view_proj[0][0] is (2 / world width) in ortho.
    let res = max(camera.resolution, vec2<f32>(1.0, 1.0));
    let world_pixel_size = 2.0 / (abs(camera.view_proj[0][0]) * res.x);
    let legacy = camera.padding.y > 0.5;
    // Device pixels per logical pixel; clamp so the floor is >= 1 device px.
    let dpp = max(camera.padding.x, 1.0);
    let min_px_logical = min(f32(in.flags & MIN_PX_MASK) / 256.0, MAX_MIN_PX);
    let min_px_world = min_px_logical * dpp * world_pixel_size;
    let safe_local_size = max(local_size, vec2<f32>(0.001));

    let is_triangle = (in.flags & FLAG_TRIANGLE) != 0u;
    let is_joined = (in.flags & FLAG_JOINED) != 0u;
    let is_arc = (in.flags & FLAG_ARC) != 0u;
    let is_line = (in.flags & FLAG_LINE) != 0u;
    let is_ring = (in.flags & FLAG_RING) != 0u;

    var corner = vec2<f32>(f32(in.vertex_index & 1u), f32(in.vertex_index >> 1u));
    var local: vec2<f32>;
    var world: vec2<f32>;
    var seg_planes = vec4<f32>(0.0);
    var joined_s0 = 0.0;
    var joined_cap = 0.0;
    var joined_ab = vec4<f32>(in.c3, in.c3);
    var joined_len = 1.0;
    var joined_width = in.dims.x;
    // Triangle family only; zero for every other path.
    var tri_fb_ab = vec4<f32>(0.0);
    var tri_fb_c = vec4<f32>(0.0);

    if (is_arc) {
        // Analytic arc/circle: c3 = centre, dims.x = radius, dims.y = width.
        // A full square quad covers the whole circle; the fragment clips it.
        let radius = in.dims.x;
        let width = in.dims.y;
        let eff_w = max(width, min_px_world);
        let extent = radius + eff_w * 0.5 + world_pixel_size * 1.5;
        world = in.c3 + (corner - vec2<f32>(0.5, 0.5)) * (extent * 2.0);
        local = vec2<f32>(0.0, 0.0);
    } else if (is_joined) {
        // Joined segment: c0 = b - a, c3 = midpoint, width in dims.x, cap style
        // in dims.y, joint plane normals in ext0, dash phase s0 in ext1.
        let c0 = in.c0;
        let len = max(length(c0), 1e-6);
        let dir = c0 / len;
        let perp = vec2<f32>(-dir.y, dir.x);
        let a = in.c3 - c0 * 0.5;
        joined_len = len;
        joined_width = max(in.dims.x, 0.0);
        joined_ab = vec4<f32>(a, a + c0);
        joined_cap = in.dims.y;
        joined_s0 = in.ext1;
        seg_planes = in.ext0;

        let eff_w = max(joined_width, min_px_world);
        let half = eff_w * 0.5;
        let n_start = in.ext0.xy;
        let n_end = in.ext0.zw;
        let start_cap = length(n_start) < 0.5;
        let end_cap = length(n_end) < 0.5;
        let start_round = ((in.flags & FLAG_JOIN_ROUND_START) != 0u)
            || (start_cap && joined_cap > 0.5 && joined_cap < 1.5);
        let end_round = ((in.flags & FLAG_JOIN_ROUND_END) != 0u)
            || (end_cap && joined_cap > 0.5 && joined_cap < 1.5);
        let ext_s = endpoint_ext(n_start, dir, half, start_round, start_cap, joined_cap);
        let ext_e = endpoint_ext(n_end, dir, half, end_round, end_cap, joined_cap);

        let pad = world_pixel_size * 1.5;
        let t0 = -ext_s - pad;
        let t1 = len + ext_e + pad;
        let t = t0 + corner.x * (t1 - t0);
        let y = (corner.y - 0.5) * (eff_w + pad * 2.0);
        world = a + dir * t + perp * y;
        local = vec2<f32>(corner.x, corner.y - 0.5);
    } else if (is_triangle) {
        // Only the first strip triangle (0,0),(1,0),(0,1) maps onto a,b,c; the
        // second one lies entirely outside. Collapse vertex 3 onto vertex 2 so
        // it is degenerate and rasterises nothing (halves the fragment work).
        if (in.vertex_index == 3u) {
            corner = vec2<f32>(0.0, 1.0);
        }
        let mask = (in.flags >> MASK_SHIFT) & 7u;
        if (mask == 0u) {
            // All edges hard: the exact triangle is the coverage. No padding,
            // so neighbouring triangles tile by the rasteriser's fill rule.
            local = corner;
        } else {
            // Grow about the centroid so the AA band fits inside the triangle
            // (same rule as before: pad / inradius, capped at 2x).
            let centroid = vec2<f32>(1.0 / 3.0, 1.0 / 3.0);
            let area2 = abs(in.c0.x * in.c1.y - in.c0.y * in.c1.x);
            let perimeter = length(in.c0) + length(in.c1) + length(in.c1 - in.c0);
            let inradius = area2 / max(perimeter, 1e-3);
            // 1 px box coverage needs ~1.5 px of AA room; keep the legacy 4 px.
            let pad = world_pixel_size * select(1.5, 4.0, legacy);
            let expansion = 1.0 + pad / max(inradius, pad);
            local = (corner - centroid) * expansion + centroid;
        }
        world = in.c3 + in.c0 * local.x + in.c1 * local.y;

        // Exact hard-edge fill rule needs the *unexpanded* triangle in
        // framebuffer space, snapped to the rasteriser's subpixel grid.
        //
        // The rasteriser flattens clip coordinates to framebuffer coordinates
        // with at least 8 fractional bits on D3D12, Vulkan and Metal (Metal
        // guarantees 8; D3D/Vulkan use a fixed 1/256 subpixel grid).  Snapping
        // the endpoints here to that same 1/256 grid lets the fragment shader
        // reproduce the rasteriser's decision with exact integer arithmetic.
        let va = in.c3;
        let vb = in.c3 + in.c0;
        let vc = in.c3 + in.c1;
        let ca = camera.view_proj * vec4<f32>(va, 0.0, 1.0);
        let cb = camera.view_proj * vec4<f32>(vb, 0.0, 1.0);
        let cc = camera.view_proj * vec4<f32>(vc, 0.0, 1.0);
        let res_fb = max(camera.resolution, vec2<f32>(1.0, 1.0));
        let origin = camera.viewport_origin;
        let na = ca.xy / ca.w;
        let nb = cb.xy / cb.w;
        let nc = cc.xy / cc.w;
        // NDC -> the view's own pixel rect, then shift by its framebuffer
        // origin.  The rasteriser's clip positions are in full framebuffer
        // coordinates, so a sub-viewport whose origin is not (0, 0) must be
        // offset here for the hard-edge fill rule to compare like with like.
        let fa = origin + vec2<f32>((na.x * 0.5 + 0.5) * res_fb.x, (1.0 - (na.y * 0.5 + 0.5)) * res_fb.y);
        let fb = origin + vec2<f32>((nb.x * 0.5 + 0.5) * res_fb.x, (1.0 - (nb.y * 0.5 + 0.5)) * res_fb.y);
        let fc = origin + vec2<f32>((nc.x * 0.5 + 0.5) * res_fb.x, (1.0 - (nc.y * 0.5 + 0.5)) * res_fb.y);
        let sa = round(fa * SUBPIXEL) / SUBPIXEL;
        let sb = round(fb * SUBPIXEL) / SUBPIXEL;
        let sc = round(fc * SUBPIXEL) / SUBPIXEL;
        // Framebuffer-space winding sign (Y down), matching the rasteriser's
        // own traversal after the viewport Y flip.
        let signed_area = (sb.x - sa.x) * (sc.y - sa.y) - (sb.y - sa.y) * (sc.x - sa.x);
        let winding = select(-1.0, 1.0, signed_area >= 0.0);
        tri_fb_ab = vec4<f32>(sa, sb);
        tri_fb_c = vec4<f32>(sc, winding, 0.0);
    } else {
        if (legacy) {
            // Cap the expansion so the quad never grows more than 100% per axis.
            let max_padding = safe_local_size * 0.5;
            let padding = min(vec2<f32>(world_pixel_size * 4.0), max_padding);
            let expansion_factor = 1.0 + (padding * 2.0 / safe_local_size);
            local = (corner - vec2<f32>(0.5, 0.5)) * expansion_factor;
        } else {
            // X: 1.5 px of AA room (capped for tiny shapes).
            var pad_x = min(world_pixel_size * 1.5, safe_local_size.x * 0.5);
            // Y: AA room plus half of the extra world width needed to reach
            // `min_px`. The floor may exceed the real width by a lot when
            // zoomed out (e.g. 15 mm line at 1 px), so it is not capped.
            var pad_y = min(world_pixel_size * 1.5, safe_local_size.y * 0.5);
            if (is_line) {
                pad_y = world_pixel_size * 1.5 + max(0.0, min_px_world - local_size.y) * 0.5;
            }
            if (is_ring) {
                // A minimum ring thickness can grow outwards past the original
                // silhouette; pad both axes by half the extra thickness.
                let extra = max(0.0, min_px_world - in.dims.x) * 0.5;
                pad_x = pad_x + extra;
                pad_y = pad_y + extra;
            }
            let expansion_factor = vec2<f32>(
                1.0 + pad_x * 2.0 / safe_local_size.x,
                1.0 + pad_y * 2.0 / safe_local_size.y,
            );
            local = (corner - vec2<f32>(0.5, 0.5)) * expansion_factor;
        }
        world = in.c3 + in.c0 * local.x + in.c1 * local.y;
    }

    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(world, 0.0, 1.0);
    out.pos = vec4<f32>(local, world);
    out.color = in.color;
    if (is_arc) {
        out.shape = vec4<f32>(in.dims.x, in.dims.y, 0.0, 0.0);
        out.tri_ab = vec4<f32>(in.c3, in.dims.x, in.dims.y);
        out.tri_c = vec2<f32>(in.ext1, 0.0);
        out.seg_planes = in.ext0;
    } else if (is_joined) {
        out.shape = vec4<f32>(joined_len, 1.0, joined_width, joined_cap);
        out.tri_ab = joined_ab;
        out.tri_c = vec2<f32>(joined_s0, joined_cap);
        out.seg_planes = seg_planes;
    } else {
        out.shape = vec4<f32>(local_size, in.dims.x, in.dims.y);
        out.tri_ab = vec4<f32>(in.c3, in.c3 + in.c0);
        out.tri_c = in.c3 + in.c1;
        out.seg_planes = vec4<f32>(0.0);
    }
    // x = legacy 4 px filter half-width (only read in legacy mode);
    // y = world units per device pixel.
    out.aa = vec2<f32>(
        max(world_pixel_size * select(1.0, 2.0, legacy), 1e-6),
        world_pixel_size,
    );
    out.flags = in.flags;
    out.tri_fb_ab = tri_fb_ab;
    out.tri_fb_c = tri_fb_c;
    out.gradient = in.gradient;
    return out;
}

// Signed distance to a triangle (Inigo Quilez), winding-agnostic.
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

fn tri_winding(a: vec2<f32>, b: vec2<f32>, c: vec2<f32>) -> f32 {
    return sign((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x));
}

// Signed distance to the clamped segment u -> v (negative inside).
fn sd_edge_segment(p: vec2<f32>, u: vec2<f32>, v: vec2<f32>, w: f32) -> f32 {
    let e = v - u;
    let t = clamp(dot(p - u, e) / max(dot(e, e), 1e-8), 0.0, 1.0);
    let dist = length(p - (u + t * e));
    let inside = w * (e.x * (p.y - u.y) - e.y * (p.x - u.x));
    return select(dist, -dist, inside > 0.0);
}

// --- Exact hard-edge fill rule ---------------------------------------------
//
// Hard triangle edges must reproduce the rasteriser's own top-left fill rule;
// a world-space tolerance band cannot do that (it either leaves a one-pixel
// hole or double-blends a shared edge).  The vertex stage snaps the triangle's
// three framebuffer-space vertices to the 1/256 subpixel grid the hardware uses
// (8 fractional bits is the minimum guaranteed by D3D12 / Vulkan / Metal) and
// passes them `flat`.  Pixel centres are exact k + 0.5, so every edge cross
// product is an exact integer multiple of 1/256 / 256 = 1/65536.
//
// f32 loses the low bits of those products for large framebuffer coordinates
// (products of ~22-bit integers), so the sign is computed with exact 64-bit
// integer arithmetic built from 16-bit limbs instead of a tolerance.
struct EdgeProduct {
    neg: bool,
    hi: u32,
    lo: u32,
};

// Unsigned 32x32 -> 64-bit product as (hi, lo), using 16-bit limbs.
fn mul_u32_wide(a: u32, b: u32) -> vec2<u32> {
    let a0 = a & 0xFFFFu;
    let a1 = a >> 16u;
    let b0 = b & 0xFFFFu;
    let b1 = b >> 16u;
    let p0 = a0 * b0;
    let p1 = a0 * b1;
    let p2 = a1 * b0;
    let p3 = a1 * b1;
    let lo0 = p0 & 0xFFFFu;
    // The low halves of p1/p2 plus p0's carry: each term is < 2^16, so the sum
    // fits in 18 bits.
    let mid = (p1 & 0xFFFFu) + (p2 & 0xFFFFu) + (p0 >> 16u);
    let hi = p3 + (p1 >> 16u) + (p2 >> 16u) + (mid >> 16u);
    return vec2<u32>(hi, ((mid & 0xFFFFu) << 16u) | lo0);
}

// i32 * i32 -> sign + 64-bit magnitude.  Converting the magnitude through u32
// also handles i32::MIN without signed overflow.
fn edge_product(a: i32, b: i32) -> EdgeProduct {
    let neg = (a < 0) != (b < 0);
    let ua = select(u32(a), 0u - u32(a), a < 0);
    let ub = select(u32(b), 0u - u32(b), b < 0);
    let m = mul_u32_wide(ua, ub);
    return EdgeProduct(neg, m.x, m.y);
}

fn wide_gt(ah: u32, al: u32, bh: u32, bl: u32) -> bool {
    return ah > bh || (ah == bh && al > bl);
}

fn wide_eq(ah: u32, al: u32, bh: u32, bl: u32) -> bool {
    return ah == bh && al == bl;
}

// Sign (-1, 0, +1) of `dx * ay - dy * bx`, evaluated exactly as a 64-bit
// two's-complement difference.
fn edge_sign(dx: i32, ay: i32, dy: i32, bx: i32) -> i32 {
    let p = edge_product(dx, ay);
    let q = edge_product(dy, bx);
    let p_zero = p.hi == 0u && p.lo == 0u;
    let q_zero = q.hi == 0u && q.lo == 0u;
    if (p_zero && q_zero) {
        return 0;
    }
    let p_neg = p.neg && !p_zero;
    let q_neg = q.neg && !q_zero;
    if (!p_neg && q_neg) {
        return 1;
    }
    if (p_neg && !q_neg) {
        return -1;
    }
    if (!p_neg) {
        if (wide_gt(p.hi, p.lo, q.hi, q.lo)) { return 1; }
        if (wide_eq(p.hi, p.lo, q.hi, q.lo)) { return 0; }
        return -1;
    }
    // Both terms are negative: P - Q = |Q| - |P|.
    if (wide_gt(q.hi, q.lo, p.hi, p.lo)) { return 1; }
    if (wide_eq(q.hi, q.lo, p.hi, p.lo)) { return 0; }
    return -1;
}

// Framebuffer coordinates (pixels) -> signed 1/256 subpixel units.
fn fb_units(v: vec2<f32>) -> vec2<i32> {
    return vec2<i32>(i32(round(v.x * 256.0)), i32(round(v.y * 256.0)));
}

// Top-left fill rule for hard edge u -> v of a triangle whose framebuffer
// winding sign is `winding`.  `E = dx*(p.y-u.y) - dy*(p.x-u.x)` and
// `F = winding*E`; the pixel is covered when F > 0, or F == 0 and the edge is
// a top edge (winding*dy == 0 && winding*dx > 0) or a left edge
// (winding*dy < 0) in the Y-down framebuffer.
fn hard_edge_inside(
    p_fb: vec2<f32>,
    u_fb: vec2<f32>,
    v_fb: vec2<f32>,
    winding: f32,
) -> bool {
    let p = fb_units(p_fb);
    let u = fb_units(u_fb);
    let v = fb_units(v_fb);
    let dx = v.x - u.x;
    let dy = v.y - u.y;
    let e = edge_sign(dx, p.y - u.y, dy, p.x - u.x);
    let f = select(-e, e, winding > 0.0);
    if (f > 0) {
        return true;
    }
    if (f < 0) {
        return false;
    }
    // On the line: only the top/left copy owns it.
    var sx = dx;
    var sy = dy;
    if (winding < 0.0) {
        sx = -dx;
        sy = -dy;
    }
    return (sy == 0 && sx > 0) || (sy < 0);
}

// Selective-AA coverage: hard-clip unmarked edges, fade only near marked ones.
fn triangle_masked_alpha(
    p: vec2<f32>,
    p_fb: vec2<f32>,
    a: vec2<f32>,
    b: vec2<f32>,
    c: vec2<f32>,
    fb_ab: vec4<f32>,
    fb_c: vec4<f32>,
    mask: u32,
    px: f32,
    legacy: bool,
) -> f32 {
    let w = tri_winding(a, b, c);
    if (w == 0.0) {
        return 0.0;
    }
    let fa = fb_ab.xy;
    let fb = fb_ab.zw;
    let fc = fb_c.xy;
    let winding = fb_c.z;
    // Hard edges clip exactly with the rasteriser's rule.
    if ((mask & 1u) == 0u && !hard_edge_inside(p_fb, fa, fb, winding)) { return 0.0; }
    if ((mask & 2u) == 0u && !hard_edge_inside(p_fb, fb, fc, winding)) { return 0.0; }
    if ((mask & 4u) == 0u && !hard_edge_inside(p_fb, fc, fa, winding)) { return 0.0; }

    // The SDF min is built from soft edges only, so a hard edge can never bleed
    // anti-aliasing into the neighbour it is shared with.
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
        return 1.0;
    }
    if (legacy) {
        return 1.0 - smoothstep(-px * 2.0, px * 2.0, best);
    }
    return coverage(best, px);
}

// Dash coverage along the polyline distance `s` (metres). Pattern `id` is a
// table of alternating dash/gap lengths. Very dense patterns (< 3 px period)
// are drawn continuous to avoid moirÃ©.
fn dash_alpha(s: f32, id: u32, px: f32, scale: f32) -> f32 {
    if (id == 0u) {
        return 1.0;
    }
    // Table slot 0 is unused; pattern id 1 lives at dash[2..4] so ids fit in
    // bits 28..31 (up to 15).
    let base = id * 2u;
    let v0 = camera.dash[base];
    let v1 = camera.dash[base + 1u];
    var lens: array<f32, 8>;
    lens[0] = v0.x * scale; lens[1] = v0.y * scale;
    lens[2] = v0.z * scale; lens[3] = v0.w * scale;
    lens[4] = v1.x * scale; lens[5] = v1.y * scale;
    lens[6] = v1.z * scale; lens[7] = v1.w * scale;

    var period = 0.0;
    for (var i = 0u; i < 8u; i = i + 1u) {
        if (lens[i] <= 0.0) { break; }
        period = period + lens[i];
    }
    if (period <= 0.0 || period / max(px, 1e-9) < 3.0) {
        return 1.0;
    }
    var sm = s - floor(s / period) * period;
    var acc = 0.0;
    for (var i = 0u; i < 8u; i = i + 1u) {
        let l = lens[i];
        if (l <= 0.0) { break; }
        if (sm < acc + l) {
            if ((i & 1u) == 1u) { return 0.0; }
            let inside = min(sm - acc, acc + l - sm);
            return clamp(inside / max(px, 1e-9) + 0.5, 0.0, 1.0);
        }
        acc = acc + l;
    }
    return 0.0;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let legacy = camera.padding.y > 0.5;
    let filter_width = in.aa.x;
    let px = in.aa.y;
    let dpp = max(camera.padding.x, 1.0);
    let min_px_world =
        min(f32(in.flags & MIN_PX_MASK) / 256.0, MAX_MIN_PX) * dpp * px;

    // --- Analytic arc / circle path ---
    if ((in.flags & FLAG_ARC) != 0u) {
        let TAU = 6.28318530718;
        let centre = in.tri_ab.xy;
        let radius = in.tri_ab.z;
        let width = in.tri_ab.w;
        let start = in.seg_planes.x;
        let sweep = in.seg_planes.y;
        let cap_style = in.seg_planes.z;
        let s0 = in.tri_c.x;
        let pf = in.pos.zw;
        let v = pf - centre;
        let rp = length(v);
        let half_w = max(width, min_px_world) * 0.5;
        let d_band = abs(rp - radius) - half_w;
        let sw = abs(sweep);
        let full = sw >= TAU - 1e-4;
        let sgn = select(-1.0, 1.0, sweep > 0.0);
        var rel = (atan2(v.y, v.x) - start) * sgn;
        rel = rel - floor(rel / TAU) * TAU;

        var d = d_band;
        var s = s0 + radius * rel;
        if (!full && rel > sw) {
            let after = rel - sw;
            let before = TAU - rel;
            if (after <= before) {
                let end_ang = start + sweep;
                let rr = vec2<f32>(cos(end_ang), sin(end_ang));
                let ep = centre + rr * radius;
                s = s0 + radius * sw;
                if (cap_style > 0.5 && cap_style < 1.5) {
                    d = length(pf - ep) - half_w;
                } else {
                    let dist_plane = abs(v.x * rr.y - v.y * rr.x);
                    let ext = select(0.0, half_w, cap_style > 1.5);
                    d = max(d_band, dist_plane - ext);
                }
            } else {
                let rr = vec2<f32>(cos(start), sin(start));
                let sp = centre + rr * radius;
                s = s0;
                if (cap_style > 0.5 && cap_style < 1.5) {
                    d = length(pf - sp) - half_w;
                } else {
                    let dist_plane = abs(v.x * rr.y - v.y * rr.x);
                    let ext = select(0.0, half_w, cap_style > 1.5);
                    d = max(d_band, dist_plane - ext);
                }
            }
        }

        var alpha = coverage(d, px);
        if (min_px_world <= 0.0 && width > 0.0 && width < px) {
            alpha = alpha * clamp(width / px, 0.0, 1.0);
        }
        let dash_id = (in.flags >> DASH_SHIFT) & DASH_MASK;
        if (dash_id > 0u) {
            let dash_scale = select(1.0, max(camera.padding.x, 1.0) * px,
                (in.flags & FLAG_DASH_PX) != 0u);
            alpha = alpha * dash_alpha(s, dash_id, px, dash_scale);
        }
        if (alpha < 0.01) {
            discard;
        }
        return premultiply(shade_color(in.color, in.pos.zw, in.gradient), alpha);
    }

    // --- Joined segment path ---
    if ((in.flags & FLAG_JOINED) != 0u) {
        let a = in.tri_ab.xy;
        let b = in.tri_ab.zw;
        let c0 = b - a;
        let len = max(length(c0), 1e-6);
        let dir = c0 / len;
        let perp_dir = vec2<f32>(-dir.y, dir.x);
        let p = in.pos.zw;
        let width = max(in.shape.z, 0.0);
        let cap_style = in.shape.w;
        let half_w = max(width, min_px_world) * 0.5;

        let rel = p - a;
        let t = dot(rel, dir);
        let dline = abs(dot(rel, perp_dir)) - half_w;

        let planes = in.seg_planes;
        let n_start = planes.xy;
        let n_end = planes.zw;
        let start_cap = length(n_start) < 0.5;
        let end_cap = length(n_end) < 0.5;
        let start_round = ((in.flags & FLAG_JOIN_ROUND_START) != 0u)
            || (start_cap && cap_style > 0.5 && cap_style < 1.5);
        let end_round = ((in.flags & FLAG_JOIN_ROUND_END) != 0u)
            || (end_cap && cap_style > 0.5 && cap_style < 1.5);

        var d = dline;
        if (t < 0.0) {
            if (start_round) {
                d = length(p - a) - half_w;
            } else if (start_cap) {
                if (cap_style < 0.5) {
                    d = max(d, -t);                 // flat boundary at a
                } else {
                    d = max(d, -(t + half_w));      // square extends half_w
                }
            }
        } else if (t > len) {
            if (end_round) {
                d = length(p - b) - half_w;
            } else if (end_cap) {
                if (cap_style < 0.5) {
                    d = max(d, t - len);            // flat boundary at b
                } else {
                    d = max(d, t - (len + half_w)); // square extends half_w
                }
            }
        }

        var alpha = coverage(d, px);
        // No floor requested: preserve the average ink of sub-pixel lines.
        if (min_px_world <= 0.0 && width > 0.0 && width < px) {
            alpha = alpha * clamp(width / px, 0.0, 1.0);
        }
        // Complementary hard cuts on the bisector planes: start allows
        // `dot > 0`, end allows `dot <= 0` (the plane itself belongs to the
        // incoming segment only). One segment owns every pixel: no gap and no
        // double blending, even for translucent strokes.
        if (!start_cap && dot(p - a, n_start) <= 0.0) {
            alpha = 0.0;
        }
        if (!end_cap && dot(p - b, n_end) > 0.0) {
            alpha = 0.0;
        }
        let dash_id = (in.flags >> DASH_SHIFT) & DASH_MASK;
        if (dash_id > 0u) {
            let dash_scale = select(1.0, max(camera.padding.x, 1.0) * px,
                (in.flags & FLAG_DASH_PX) != 0u);
            alpha = alpha * dash_alpha(in.tri_c.x + t, dash_id, px, dash_scale);
        }
        if (alpha < 0.01) {
            discard;
        }
        return premultiply(shade_color(in.color, in.pos.zw, in.gradient), alpha);
    }

    // --- Triangle path ---
    if ((in.flags & FLAG_TRIANGLE) != 0u) {
        let mask = (in.flags >> MASK_SHIFT) & 7u;
        if (mask == 0u) {
            // Hard-edged fill (e.g. wall interiors): the rasterised triangle is
            // exactly the coverage, so this is a plain colour write.
            return premultiply(shade_color(in.color, in.pos.zw, in.gradient), 1.0);
        }
        let a = in.tri_ab.xy;
        let b = in.tri_ab.zw;
        let c = in.tri_c;
        let p = in.pos.zw;
        var alpha: f32;
        if (mask == 7u) {
            let d = sd_triangle(p, a, b, c);
            alpha = select(coverage(d, px), 1.0 - smoothstep(-filter_width, filter_width, d), legacy);
        } else {
            alpha = triangle_masked_alpha(
                p,
                in.clip_position.xy,
                a,
                b,
                c,
                in.tri_fb_ab,
                in.tri_fb_c,
                mask,
                px,
                legacy,
            );
        }
        if (alpha < 0.01) {
            discard;
        }
        return premultiply(shade_color(in.color, in.pos.zw, in.gradient), alpha);
    }

    // --- Rounded-rectangle / circle / line path ---
    let local_size = in.shape.xy;
    let p = in.pos.xy * local_size;
    let actual_width = in.shape.z;
    let radius = in.shape.w;
    let is_line = (in.flags & FLAG_LINE) != 0u;
    let is_ring = (in.flags & FLAG_RING) != 0u;

    // Per-cap hard flags: push that side of the analysis box out so the SDF is
    // saturated near the true cap plane, then hard-clip at the plane below.
    let hard_start = (in.flags & FLAG_CAP_START) != 0u;
    let hard_end = (in.flags & FLAG_CAP_END) != 0u;
    let cap_push = select(px * 2.0 + 1.0, filter_width * 4.0 + 1.0, legacy);
    let e_left = select(0.0, cap_push, hard_start);
    let e_right = select(0.0, cap_push, hard_end);
    var half = local_size * 0.5;
    let center_shift = (e_right - e_left) * 0.5;
    let half_x = half.x + (e_left + e_right) * 0.5;

    // Line width resolution.
    var radius_eff = radius;
    var alpha_multiplier = 1.0;
    if (!legacy && is_line && actual_width > 0.0) {
        if (min_px_world > 0.0) {
            // Minimum width floor: widen in world units, keep opacity intact.
            let eff_w = max(actual_width, min_px_world);
            let scale = eff_w / actual_width;
            half.y = eff_w * 0.5;
            radius_eff = radius * scale;
        } else if (actual_width < px) {
            // No floor requested: preserve the average "ink" of a sub-pixel
            // line by drawing it 1 px wide and scaling alpha proportionally.
            let eff_w = px;
            half.y = eff_w * 0.5;
            radius_eff = radius * (eff_w / actual_width);
            alpha_multiplier = clamp(actual_width / px, 0.0, 1.0);
        }
    }

    // Exact Euclidean distance to the rounded rectangle.
    let q = vec2<f32>(abs(p.x - center_shift), abs(p.y))
        - vec2<f32>(half_x, half.y) + vec2<f32>(radius_eff);
    let d = min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - radius_eff;

    var final_d = d;
    if (legacy) {
        // Old thin-line stabiliser: widen to ~1.8 px and fade the alpha.
        let min_world_width = px * 1.8;
        if (actual_width > 0.0 && actual_width < min_world_width) {
            final_d -= (min_world_width - actual_width) * 0.5;
            alpha_multiplier = clamp(actual_width / min_world_width, 0.2, 1.0);
        }
    }

    // Ring / hollow outline: turn the filled-shape SDF into a band centred on
    // the boundary. `d` is negative inside, so `abs(d + t/2) - t/2` is the
    // exact band distance. Thickness honours the same 1-px floor.
    if (is_ring) {
        let t_eff = max(actual_width, min_px_world);
        final_d = abs(final_d + t_eff * 0.5) - t_eff * 0.5;
        alpha_multiplier = 1.0;
    }

    var final_alpha = select(
        coverage(final_d, px),
        (1.0 - smoothstep(-filter_width, filter_width, final_d)),
        legacy,
    ) * alpha_multiplier;

    if (hard_start && p.x < -half.x) {
        final_alpha = 0.0;
    }
    if (hard_end && p.x > half.x) {
        final_alpha = 0.0;
    }

    if (final_alpha < 0.01) {
        discard;
    }

    return premultiply(shade_color(in.color, in.pos.zw, in.gradient), final_alpha);
}

// --- Gradient fill (D-0030) -----------------------------------------------
//
// The camera uniform carries a flat table built by the host:
//   [kind, stop_count, first_stop, 0]
//   [start.xy, end.xy] or [center.xy, radius, 0]
//   per stop: [r, g, b, a] (linear) and [offset, 0, 0, 0]
// `kind` 0 = linear, 1 = radial; `first_stop` is an absolute vec4 index.
// Gradient space is world space, which every shape path already carries in
// `pos.zw`.

fn grad_srgb_to_linear(channel: f32) -> f32 {
    let c = clamp(channel, 0.0, 1.0);
    if (c <= 0.04045) {
        return c / 12.92;
    }
    return pow((c + 0.055) / 1.055, 2.4);
}

fn grad_linear_to_srgb(channel: f32) -> f32 {
    let c = clamp(channel, 0.0, 1.0);
    if (c <= 0.0031308) {
        return c * 12.92;
    }
    return 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

// 8x8 ordered dither in [-0.5, 0.5], built from the standard Bayer matrix.
fn grad_bayer(p: vec2<f32>) -> f32 {
    var x = u32(p.x) & 7u;
    var y = u32(p.y) & 7u;
    var v = 0u;
    for (var bit = 0u; bit < 3u; bit = bit + 1u) {
        let xb = (x >> bit) & 1u;
        let yb = (y >> bit) & 1u;
        v = v | ((xb ^ yb) << (bit * 2u + 1u)) | (yb << (bit * 2u));
    }
    return f32(v) / 64.0 - 0.5;
}

// Straight-alpha linear colour of a gradient at world position `p`.
fn gradient_color(p: vec2<f32>, base: u32) -> vec4<f32> {
    let header = camera.gradients[base];
    let kind = u32(header.x + 0.5);
    let count = u32(header.y + 0.5);
    let first = u32(header.z + 0.5);
    if (count == 0u) {
        return vec4<f32>(1.0);
    }
    let geo = camera.gradients[base + 1u];
    var t: f32;
    if (kind == 0u) {
        let a = geo.xy;
        let d = geo.zw - a;
        t = dot(p - a, d) / max(dot(d, d), 1e-9);
    } else {
        t = length(p - geo.xy) / max(geo.z, 1e-6);
    }
    t = clamp(t, 0.0, 1.0);

    // Offsets are sorted by the host. Two stops at the same offset make a
    // hard edge, which the segment search naturally produces.
    var i = 0u;
    loop {
        if (i + 1u >= count) { break; }
        let o1 = camera.gradients[first + i * 2u + 3u].x;
        if (t <= o1) { break; }
        i = i + 1u;
    }
    let c0 = camera.gradients[first + i * 2u];
    let o0 = camera.gradients[first + i * 2u + 1u].x;
    let c1 = camera.gradients[first + min(i + 1u, count - 1u) * 2u];
    let o1 = camera.gradients[first + min(i + 1u, count - 1u) * 2u + 1u].x;
    let span = o1 - o0;
    let f = select(0.0, clamp((t - o0) / span, 0.0, 1.0), span > 1e-6);
    var color = mix(c0, c1, f);

    // Dither in encoded space: a quarter of a code is invisible on its own but
    // breaks flat ramps into noise. It fades to zero at a stop so the authored
    // sRGB byte survives exactly there.
    let stop_distance = min(abs(t - o0), abs(t - o1));
    let fade = smoothstep(0.0, 0.004, stop_distance);
    let noise = grad_bayer(p) * fade / 255.0;
    color.r = grad_srgb_to_linear(grad_linear_to_srgb(color.r) + noise);
    color.g = grad_srgb_to_linear(grad_linear_to_srgb(color.g) + noise);
    color.b = grad_srgb_to_linear(grad_linear_to_srgb(color.b) + noise);
    return color;
}

// The instance colour at `p`: its gradient, or its solid colour.
fn shade_color(solid: vec4<f32>, p: vec2<f32>, gradient: u32) -> vec4<f32> {
    if (gradient == 0u) {
        return solid;
    }
    return gradient_color(p, gradient - 1u);
}
