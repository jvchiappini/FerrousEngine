// ---------------------------------------------------------------------------
// Shared Pure2D camera layout and gradient evaluation.
//
// Prepended to the shape and text shaders.  Declares the camera uniform (it
// mirrors `ferrous_2d::render::Uniform2d`, including the dash and gradient
// tables) and the colour-ramp helpers used by both pipelines.
//
// Gradient table layout (built on the CPU by `GradientTable2d`):
//   [kind, stop_count, first_stop_slot, 0]   kind 0 = linear, 1 = radial
//   [start.xy, end.xy] or [center.xy, radius, 0]
//   per stop: [r, g, b, a] (linear) and [offset, 0, 0, 0]
// `first_stop_slot` is an absolute slot index; stops are sorted by offset.
// ---------------------------------------------------------------------------

struct CameraUniform {
    view_proj: mat4x4<f32>,
    resolution: vec2<f32>,
    // x = device pixels per logical pixel (resolves min_px to world units),
    // y = legacy line-stabiliser A/B flag (>0.5 = old behaviour).
    padding: vec2<f32>,
    // Physical-pixel origin of this viewport inside the framebuffer.
    viewport_origin: vec2<f32>,
    // Keeps `dash` (a vec4 array) 16-byte aligned.
    _pad: vec2<f32>,
    // 16 dash patterns x 8 lengths (two vec4 each); pattern 0 is unused.
    dash: array<vec4<f32>, 32>,
    // Gradient table: headers, geometry and stops (512 vec4).
    gradients: array<vec4<f32>, 512>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

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

    // Stops are sorted by the host.  Two stops at the same offset make a hard
    // edge, which the segment search naturally produces.
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
    // breaks flat ramps into noise.  It fades to zero at a stop so the authored
    // sRGB byte survives exactly there.
    let stop_distance = min(abs(t - o0), abs(t - o1));
    let fade = smoothstep(0.0, 0.004, stop_distance);
    let noise = grad_bayer(p) * fade / 255.0;
    color.r = grad_srgb_to_linear(grad_linear_to_srgb(color.r) + noise);
    color.g = grad_srgb_to_linear(grad_linear_to_srgb(color.g) + noise);
    color.b = grad_srgb_to_linear(grad_linear_to_srgb(color.b) + noise);
    return color;
}

// The colour at `p`: the referenced gradient, or the solid colour.
fn shade_color(solid: vec4<f32>, p: vec2<f32>, gradient: u32) -> vec4<f32> {
    if (gradient == 0u) {
        return solid;
    }
    return gradient_color(p, gradient - 1u);
}
