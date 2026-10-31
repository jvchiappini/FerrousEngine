// Procedural, camera-following 2D background grid.
//
// One full-screen triangle; every fragment unprojects its pixel centre through
// the inverse view-projection and evaluates the grid analytically. No CPU
// geometry, no per-frame upload beyond the 176-byte uniform.
//
// LOD: `fine` is the smallest user-step × 10^k that measures >= 8 device px.
// Lines at `fine` are faded in with `smoothstep(8, 32, fine_px)`; the coarser
// ×10 level is always drawn, so zooming never makes the grid disappear or
// jump. Fine lines coinciding with the coarse level are skipped (coarse owns
// them), which avoids double blending.

struct GridUniform {
    inv_view_proj: mat4x4<f32>,
    resolution: vec2<f32>,
    // Pixel viewport [x, y, w, h] in the render target (multi-view).
    viewport: vec4<f32>,
    world_per_px: f32,
    dpp: f32,
    // [base_step, major_every, minor_width_px, major_width_px]
    params: vec4<f32>,
    // [axis_width_px, flags(axes), 0, 0]
    params2: vec4<f32>,
    minor_color: vec4<f32>,
    major_color: vec4<f32>,
    axis_x_color: vec4<f32>,
    axis_y_color: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> grid: GridUniform;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    var p = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0),
    );
    return vec4<f32>(p[vi], 0.0, 1.0);
}

// Source-over for two premultiplied-alpha-style colours (both straight alpha).
fn over(dst: vec4<f32>, src: vec4<f32>) -> vec4<f32> {
    let a = src.a + dst.a * (1.0 - src.a);
    if (a <= 0.0) {
        return vec4<f32>(0.0);
    }
    let rgb = (src.rgb * src.a + dst.rgb * dst.a * (1.0 - src.a)) / a;
    return vec4<f32>(rgb, a);
}

// Distance from the fragment to the nearest line of `step`, in device px.
fn line_distance_px(world: f32, step: f32) -> f32 {
    let s = max(step, 1e-9);
    return abs(fract(world / s - 0.5) - 0.5) * s / max(grid.world_per_px, 1e-9);
}

// 1-px box coverage for a line of `width_dev` device px.
fn line_coverage(world: f32, step: f32, width_dev: f32) -> f32 {
    let d = line_distance_px(world, step);
    return clamp(width_dev * 0.5 + 0.5 - d, 0.0, 1.0);
}

// Is the nearest line of `step` also a major line?
fn is_major(world: f32, major_step: f32, step: f32) -> bool {
    let m = max(major_step, step);
    let d = abs(fract(world / m + 0.5) - 0.5) * m;
    return d < step * 0.25;
}

// Is the nearest fine line also owned by the coarse (x10) level?
fn is_coarse_line(world: f32, fine: f32) -> bool {
    let i = round(world / max(fine, 1e-9));
    return abs(i - 10.0 * round(i / 10.0)) < 0.01;
}

@fragment
fn fs_main(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let vp = grid.viewport;
    let vw = max(vp.z, 1.0);
    let vh = max(vp.w, 1.0);
    let local = vec2<f32>((frag.x - vp.x) / vw, (frag.y - vp.y) / vh);
    let ndc = vec2<f32>(local.x * 2.0 - 1.0, 1.0 - local.y * 2.0);
    let wp = (grid.inv_view_proj * vec4<f32>(ndc, 0.0, 1.0)).xy;

    let px = max(grid.world_per_px, 1e-9);
    let base = max(grid.params.x, 1e-9);
    let major_every = max(grid.params.y, 1.0);
    let minor_w = grid.params.z * grid.dpp;
    let major_w = grid.params.w * grid.dpp;
    let axis_w = grid.params2.x * grid.dpp;

    // Finest level whose spacing is at least 8 device px (never a sub-pixel
    // mush). `k` is clamped at 0 so the grid never gets finer than the user's
    // own base step.
    let want = max(8.0 * px / base, 1e-9);
    let k = max(ceil(log2(want) / log2(10.0)), 0.0);
    let fine = base * pow(10.0, k);
    let coarse = fine * 10.0;
    let fine_px = fine / px;
    let w = smoothstep(8.0, 32.0, fine_px);
    let major_step = fine * major_every;

    var cov_minor = 0.0;
    var cov_major = 0.0;
    var cov_axis_x = 0.0;
    var cov_axis_y = 0.0;

    // ── Coarse level (always fully visible) ──────────────────────────────
    for (var axis = 0u; axis < 2u; axis = axis + 1u) {
        let c = select(wp.x, wp.y, axis == 1u);
        let mj = is_major(c, major_step, fine);
        let wd = select(minor_w, major_w, mj);
        let cov = line_coverage(c, coarse, wd);
        if (mj) {
            cov_major = max(cov_major, cov);
        } else {
            cov_minor = max(cov_minor, cov);
        }
    }

    // ── Fine level (fades in between 8 and 32 px) ────────────────────────
    for (var axis = 0u; axis < 2u; axis = axis + 1u) {
        let c = select(wp.x, wp.y, axis == 1u);
        if (!is_coarse_line(c, fine)) {
            let mj = is_major(c, major_step, fine);
            let wd = select(minor_w, major_w, mj);
            let cov = line_coverage(c, fine, wd) * w;
            if (mj) {
                cov_major = max(cov_major, cov);
            } else {
                cov_minor = max(cov_minor, cov);
            }
        }
    }

    // ── Axes (x = 0 vertical, y = 0 horizontal) ─────────────────────────
    if (grid.params2.y > 0.5) {
        // Axes are single lines at world x = 0 / y = 0 (distance to 0 directly).
        cov_axis_y = clamp(axis_w * 0.5 + 0.5 - abs(wp.x) / px, 0.0, 1.0);
        cov_axis_x = clamp(axis_w * 0.5 + 0.5 - abs(wp.y) / px, 0.0, 1.0);
    }

    var out = vec4<f32>(0.0);
    out = over(out, vec4<f32>(grid.minor_color.rgb, grid.minor_color.a * cov_minor));
    out = over(out, vec4<f32>(grid.major_color.rgb, grid.major_color.a * cov_major));
    out = over(out, vec4<f32>(grid.axis_x_color.rgb, grid.axis_x_color.a * cov_axis_x));
    out = over(out, vec4<f32>(grid.axis_y_color.rgb, grid.axis_y_color.a * cov_axis_y));

    if (out.a < 0.002) {
        discard;
    }
    return out;
}
