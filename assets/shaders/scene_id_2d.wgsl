// 2D scene ID-buffer shader.
//
// Renders one quad per pickable item into an R32Uint target. The fragment
// shader re-evaluates the exact CPU shape test in the item's local space, so
// the id written for a pixel is the same item the CPU picker would return.
// The CPU reference for the encoding lives in `ferrous_2d::picking::IdBuffer`.

struct Uniforms {
    view_c0: vec2<f32>,
    view_c1: vec2<f32>,
    view_c3: vec2<f32>,
    size: vec2<f32>,
};

struct Item {
    // World AABB: min.xy, max.xy.
    aabb: vec4<f32>,
    // Inverse local transform: c0.xy, c1.xy.
    inv0: vec4<f32>,
    // Inverse local transform translation: t.xy, unused, unused.
    inv1: vec4<f32>,
    // Local bounds: min.xy, max.xy.
    local: vec4<f32>,
    // Rect: size.xy, corner_radius, stroke_width.
    // Circle: radius in x.
    shape: vec4<f32>,
    // id + 1 (0 is the clear value), kind, flags, path point offset.
    info: vec4<u32>,
    // Path point count, clip head, unused, unused.
    more: vec4<u32>,
    // Depth fraction, local tolerance, unused, unused.
    depth_tol: vec4<f32>,
};

struct Clip {
    inv0: vec4<f32>,
    inv1: vec4<f32>,
    // Local rect: min.xy, max.xy.
    rect: vec4<f32>,
    // Parent index or 0xffffffff, state (0 disabled, 1 enabled, 2 invalid).
    info: vec4<u32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var<storage, read> items: array<Item>;
@group(0) @binding(2) var<storage, read> points: array<vec2<f32>>;
@group(0) @binding(3) var<storage, read> clips: array<Clip>;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec2<f32>,
    @location(1) @interpolate(flat) item_index: u32,
};

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VsOut {
    let corner = vec2<f32>(f32(vertex_index & 1u), f32(vertex_index >> 1u));
    let item = items[instance_index];
    let world = mix(item.aabb.xy, item.aabb.zw, corner);
    let pixel = uniforms.view_c0 * world.x + uniforms.view_c1 * world.y + uniforms.view_c3;
    var ndc = (pixel / uniforms.size) * 2.0 - vec2<f32>(1.0);
    ndc.y = -ndc.y;

    var out: VsOut;
    out.clip = vec4<f32>(ndc, item.depth_tol.x, 1.0);
    out.world = world;
    out.item_index = instance_index;
    return out;
}

fn apply_inverse(inv0: vec4<f32>, inv1: vec4<f32>, point: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(
        inv0.x * point.x + inv0.z * point.y + inv1.x,
        inv0.y * point.x + inv0.w * point.y + inv1.y,
    );
}

fn inside_rounded_rect(point: vec2<f32>, half: vec2<f32>, radius_in: f32, tolerance: f32) -> bool {
    let radius = clamp(radius_in, 0.0, min(half.x, half.y));
    let inner = max(half - vec2<f32>(radius), vec2<f32>(0.0));
    let outer = half + vec2<f32>(tolerance);
    let local = abs(point);
    if (local.x > outer.x || local.y > outer.y) {
        return false;
    }
    if (local.x <= inner.x + tolerance || local.y <= inner.y + tolerance) {
        return true;
    }
    return length(local - inner) <= radius + tolerance;
}

fn cross2(a: vec2<f32>, b: vec2<f32>, point: vec2<f32>) -> f32 {
    return (b.x - a.x) * (point.y - a.y) - (point.x - a.x) * (b.y - a.y);
}

fn point_in_polygon(point: vec2<f32>, offset: u32, count: u32) -> bool {
    if (count < 3u) {
        return false;
    }
    var inside = false;
    var previous = points[offset + count - 1u];
    for (var index = 0u; index < count; index = index + 1u) {
        let current = points[offset + index];
        if ((current.y > point.y) != (previous.y > point.y)) {
            let crossing = (previous.x - current.x) * (point.y - current.y) / (previous.y - current.y) + current.x;
            if (point.x < crossing) {
                inside = !inside;
            }
        }
        previous = current;
    }
    return inside;
}

fn winding_number(point: vec2<f32>, offset: u32, count: u32) -> i32 {
    if (count < 3u) {
        return 0;
    }
    var winding = 0;
    var previous = points[offset + count - 1u];
    for (var index = 0u; index < count; index = index + 1u) {
        let current = points[offset + index];
        if (previous.y <= point.y) {
            if (current.y > point.y && cross2(previous, current, point) > 0.0) {
                winding = winding + 1;
            }
        } else if (current.y <= point.y && cross2(previous, current, point) < 0.0) {
            winding = winding - 1;
        }
        previous = current;
    }
    return winding;
}

fn closest_on_segment(point: vec2<f32>, start: vec2<f32>, end: vec2<f32>) -> vec2<f32> {
    let direction = end - start;
    let length_squared = dot(direction, direction);
    if (length_squared <= 1e-12) {
        return start;
    }
    let t = clamp(dot(point - start, direction) / length_squared, 0.0, 1.0);
    return start + direction * t;
}

fn distance_to_polyline(
    point: vec2<f32>,
    offset: u32,
    count: u32,
    closed: bool,
    round_cap: bool,
) -> f32 {
    if (count == 0u) {
        return 1e30;
    }
    if (count == 1u) {
        return distance(point, points[offset]);
    }
    var best = 1e30;
    for (var index = 1u; index < count; index = index + 1u) {
        best = min(best, distance(point, closest_on_segment(point, points[offset + index - 1u], points[offset + index])));
    }
    if (closed) {
        best = min(best, distance(point, closest_on_segment(point, points[offset + count - 1u], points[offset])));
    } else if (round_cap) {
        best = min(best, distance(point, points[offset]));
        best = min(best, distance(point, points[offset + count - 1u]));
    }
    return best;
}

fn shape_hit(item: Item, world: vec2<f32>) -> bool {
    let kind = item.info.y;
    let tolerance = item.depth_tol.y;
    let local = apply_inverse(item.inv0, item.inv1, world);

    if (kind == 0u) {
        return inside_rounded_rect(local, item.shape.xy * 0.5, item.shape.z, tolerance);
    }
    if (kind == 1u) {
        let reach = item.shape.x + tolerance;
        return dot(local, local) <= reach * reach;
    }
    if (kind == 2u) {
        let count = item.more.x;
        let offset = item.info.w;
        let flags = item.info.z;
        let closed = (flags & 1u) != 0u;
        let filled = (flags & 2u) != 0u;
        let even_odd = (flags & 4u) != 0u;
        let has_stroke = (flags & 8u) != 0u;
        let round_cap = (flags & 16u) != 0u;
        var hit = false;
        if (filled && closed) {
            if (even_odd) {
                hit = point_in_polygon(local, offset, count);
            } else {
                hit = winding_number(local, offset, count) != 0;
            }
        }
        if (!hit && has_stroke) {
            let reach = item.shape.w * 0.5 + tolerance;
            hit = distance_to_polyline(local, offset, count, closed, round_cap) <= reach;
        }
        return hit;
    }
    if (kind == 3u || kind == 4u) {
        let low = item.local.xy - vec2<f32>(tolerance);
        let high = item.local.zw + vec2<f32>(tolerance);
        return local.x >= low.x && local.y >= low.y && local.x <= high.x && local.y <= high.y;
    }
    return false;
}

fn clipped(item: Item, world: vec2<f32>) -> bool {
    var index = item.more.y;
    for (var step = 0u; step < 64u; step = step + 1u) {
        if (index == 0xffffffffu) {
            return true;
        }
        let clip = clips[index];
        let state = clip.info.y;
        if (state == 2u) {
            return false;
        }
        if (state == 1u) {
            let local = apply_inverse(clip.inv0, clip.inv1, world);
            if (local.x < clip.rect.x || local.y < clip.rect.y || local.x > clip.rect.z || local.y > clip.rect.w) {
                return false;
            }
        }
        index = clip.info.x;
    }
    return true;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) u32 {
    let item = items[in.item_index];
    if (!clipped(item, in.world)) {
        discard;
    }
    if (!shape_hit(item, in.world)) {
        discard;
    }
    return item.info.x;
}
