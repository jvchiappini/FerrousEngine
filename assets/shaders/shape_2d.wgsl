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
    // 16 dash patterns × 8 lengths (two vec4 each); pattern 0 is unused.
    dash: array<vec4<f32>, 32>,
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

// 1-pixel box filter: coverage goes from 1 at half a pixel inside the edge to
// 0 at half a pixel outside. `d` and `px` are both in world units.
fn coverage(d: f32, px: f32) -> f32 {
    return clamp(0.5 - d / max(px, 1e-9), 0.0, 1.0);
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

// Half-plane test for a hard (non-AA) edge.
//
// Two neighbouring triangles see their shared edge in opposite directions, so
// the side test has opposite signs. On the line itself (`c == 0`) both would
// include the pixel, double-blending translucent fills into a darker diagonal.
// The direction tie-break makes exactly one of the two copies own the line.
fn edge_inside(p: vec2<f32>, u: vec2<f32>, v: vec2<f32>, w: f32) -> bool {
    let e = v - u;
    let c = w * (e.x * (p.y - u.y) - e.y * (p.x - u.x));
    if (c > 0.0) {
        return true;
    }
    if (c < 0.0) {
        return false;
    }
    return (e.x > 0.0) || (e.x == 0.0 && e.y > 0.0);
}

// Selective-AA coverage: hard-clip unmarked edges, fade only near marked ones.
fn triangle_masked_alpha(
    p: vec2<f32>,
    a: vec2<f32>,
    b: vec2<f32>,
    c: vec2<f32>,
    mask: u32,
    px: f32,
    legacy: bool,
) -> f32 {
    let w = tri_winding(a, b, c);
    if (w == 0.0) {
        return 0.0;
    }
    if ((mask & 1u) == 0u && !edge_inside(p, a, b, w)) { return 0.0; }
    if ((mask & 2u) == 0u && !edge_inside(p, b, c, w)) { return 0.0; }
    if ((mask & 4u) == 0u && !edge_inside(p, c, a, w)) { return 0.0; }

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
// are drawn continuous to avoid moiré.
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
        return vec4<f32>(in.color.rgb, in.color.a * alpha);
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
        return vec4<f32>(in.color.rgb, in.color.a * alpha);
    }

    // --- Triangle path ---
    if ((in.flags & FLAG_TRIANGLE) != 0u) {
        let mask = (in.flags >> MASK_SHIFT) & 7u;
        if (mask == 0u) {
            // Hard-edged fill (e.g. wall interiors): the rasterised triangle is
            // exactly the coverage, so this is a plain colour write.
            return in.color;
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
            alpha = triangle_masked_alpha(p, a, b, c, mask, px, legacy);
        }
        if (alpha < 0.01) {
            discard;
        }
        return vec4<f32>(in.color.rgb, in.color.a * alpha);
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

    return vec4<f32>(in.color.rgb, in.color.a * final_alpha);
}
