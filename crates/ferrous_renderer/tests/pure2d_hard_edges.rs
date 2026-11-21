//! GPU regression tests for hard / selectively anti-aliased triangle edges.
//!
//! `shape_2d.wgsl` used to decide hard triangle edges with a world-space
//! `EDGE_EPS` band and a hand-written tie-break that did not match the
//! rasteriser's fill rule.  Shared hard edges could then lose the single pixel
//! the rasteriser assigned to them (a 1-px hole) or be blended twice.  These
//! tests draw translucent triangle fans and check the read-back bytes:
//!
//! * all-hard fan (`edge_aa_mask == 0`): every pixel centre strictly inside the
//!   polygon must show exactly one coverage blend;
//! * mixed fan (a random subset of the *outer* edges anti-aliased): same
//!   interior guarantee away from the AA band, no hole anywhere inside, and no
//!   pixel anywhere may show the double-coverage value;
//! * deterministic half-integer quads split along `y = x` and `y = -x`, whose
//!   diagonals pass exactly through pixel centres, to force the top-left tie,
//!   including a mixed triangle beside an all-hard one on the same hard edge.
//!
//! Colour is `[1, 1, 1, 0.5]` over an opaque black clear with `Rgba8Unorm`, so
//! alpha blending yields red channel 128 for one coverage and 191 for two
//! (0.5 then 0.75 coverage).  0 means a hole.  The tolerance is 1 LSB for the
//! UNORM encode only; there is no spatial tolerance.  A 0.05 px sliver at the
//! silhouette is skipped: which side of the boundary those pixel centres fall
//! on is decided by the rasteriser's subpixel rounding, not by the edge rule.
//!
//! Headless GPU tests: they skip cleanly (printing `[skip] ...`) when no
//! adapter is available, following `tests/pure2d_headless.rs`.

use std::sync::Mutex;

use ferrous_2d::render::ShapeInstance;
use ferrous_renderer::headless::Pure2dHarness;
use glam::Vec2;
use wgpu::TextureFormat;

/// One software/real device per process at a time, like `pure2d_headless.rs`.
static GPU_LOCK: Mutex<()> = Mutex::new(());

const SIZE: u32 = 256;
const WORLD_HALF: f64 = SIZE as f64 * 0.5;
/// Linear RGBA written by every triangle: half-transparent white.
const WHITE_HALF: [f32; 4] = [1.0, 1.0, 1.0, 0.5];
/// `0.5 * 255`, the single-coverage encode.
const SINGLE: i32 = 128;
/// Anything at or above this is two full blends (0.75 * 255 = 191.25).
const DOUBLE_MIN: i32 = 176;
/// Mixed outer AA edges are partial up to ~1 px inside; only assert the exact
/// single value deeper than this many pixels from every polygon edge.
const AA_MARGIN: f64 = 2.0;
/// Skip a 0.05 px sliver at the silhouette: for those pixel centres the
/// rasteriser's subpixel rounding, not the hard-edge rule, decides coverage.
/// The polygon itself is built from the same f32 vertex values the shader gets.
const INSIDE_MARGIN: f64 = 0.05;
/// UNORM encode rounding only.
const ENCODE_TOL: i32 = 1;

// ---------------------------------------------------------------------------
// Skip / harness helpers
// ---------------------------------------------------------------------------

fn harness() -> Option<Pure2dHarness> {
    match Pure2dHarness::new(SIZE, SIZE, TextureFormat::Rgba8Unorm) {
        Ok(h) => Some(h),
        Err(err) => {
            println!("[skip] headless GPU unavailable: {err}");
            None
        }
    }
}

// ---------------------------------------------------------------------------
// SplitMix64 PRNG (fixed seed, printed on failure)
// ---------------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn f64(&mut self) -> f64 {
        ((self.next_u64() >> 11) as f64) / ((1u64 << 53) as f64)
    }

    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.f64()
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }

    fn bool(&mut self) -> bool {
        (self.next_u64() >> 63) != 0
    }
}

// ---------------------------------------------------------------------------
// Polygon / pixel geometry (all in f64)
// ---------------------------------------------------------------------------

fn pixel_world(x: u32, y: u32) -> [f64; 2] {
    [x as f64 + 0.5 - WORLD_HALF, WORLD_HALF - (y as f64 + 0.5)]
}

fn cross(o: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

/// Monotone-chain convex hull (counter-clockwise) so the fan triangulation is
/// always valid.  The generated points are already near-convex, so the hull
/// normally keeps every vertex.
fn convex_hull(mut pts: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    pts.sort_by(|a, b| {
        a[0].partial_cmp(&b[0])
            .unwrap()
            .then(a[1].partial_cmp(&b[1]).unwrap())
    });
    pts.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-12 && (a[1] - b[1]).abs() < 1e-12);
    if pts.len() < 3 {
        return pts;
    }
    let mut lower: Vec<[f64; 2]> = Vec::with_capacity(pts.len());
    for &p in &pts {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], p) <= 0.0 {
            lower.pop();
        }
        lower.push(p);
    }
    let mut upper: Vec<[f64; 2]> = Vec::with_capacity(pts.len());
    for &p in pts.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], p) <= 0.0 {
            upper.pop();
        }
        upper.push(p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Random convex polygon: 5-9 vertices, random radius/rotation and a
/// non-integer centre offset.  Returned in counter-clockwise order.
fn random_convex_polygon(rng: &mut Rng) -> Vec<[f64; 2]> {
    for _ in 0..32 {
        let n = 5 + rng.below(5); // 5..=9
        let rot = rng.range(0.0, std::f64::consts::TAU);
        let cx = rng.range(-8.0, 8.0);
        let cy = rng.range(-8.0, 8.0);
        let base_r = rng.range(40.0, 78.0);
        let mut pts = Vec::with_capacity(n);
        for k in 0..n {
            let jitter = rng.range(-0.2, 0.2) * std::f64::consts::TAU / n as f64;
            let ang = rot + std::f64::consts::TAU * k as f64 / n as f64 + jitter;
            let r = base_r * rng.range(0.94, 1.06);
            pts.push([cx + r * ang.cos(), cy + r * ang.sin()]);
        }
        let hull = convex_hull(pts);
        if hull.len() >= 3 {
            return hull;
        }
    }
    unreachable!("convex hull always has at least 3 vertices");
}

/// `Some(min perpendicular distance to the polygon edges)` when `p` is
/// strictly inside a counter-clockwise convex polygon, else `None`.
fn inside_distance(poly: &[[f64; 2]], p: [f64; 2]) -> Option<f64> {
    let mut min_d = f64::INFINITY;
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        let ex = b[0] - a[0];
        let ey = b[1] - a[1];
        let c = ex * (p[1] - a[1]) - ey * (p[0] - a[0]);
        if c <= 0.0 {
            return None;
        }
        let len = (ex * ex + ey * ey).sqrt();
        min_d = min_d.min(c / len);
    }
    Some(min_d)
}

// ---------------------------------------------------------------------------
// Drawing / readback
// ---------------------------------------------------------------------------

/// Fans `verts` from their centroid.  `aa[i] == true` marks the *outer* edge
/// `verts[i] -> verts[i+1]` anti-aliased, which makes fan triangle `i` mixed
/// (its two spokes stay hard).
fn render_fan(h: &mut Pure2dHarness, verts: &[Vec2], aa: &[bool]) {
    assert_eq!(verts.len(), aa.len());
    let n = verts.len();
    let centroid = verts.iter().fold(Vec2::ZERO, |acc, v| acc + *v) / n as f32;
    h.begin_frame();
    for i in 0..n {
        let mask = if aa[i] { 0b010 } else { 0 };
        h.draw_shape(ShapeInstance::polygon_triangle(
            centroid,
            verts[i],
            verts[(i + 1) % n],
            WHITE_HALF,
            mask,
        ));
    }
    h.render_frame();
}

fn render_two(h: &mut Pure2dHarness, a: (Vec2, Vec2, Vec2, u8), b: (Vec2, Vec2, Vec2, u8)) {
    h.begin_frame();
    h.draw_shape(ShapeInstance::polygon_triangle(
        a.0, a.1, a.2, WHITE_HALF, a.3,
    ));
    h.draw_shape(ShapeInstance::polygon_triangle(
        b.0, b.1, b.2, WHITE_HALF, b.3,
    ));
    h.render_frame();
}

fn red(pixels: &[u8], x: u32, y: u32) -> i32 {
    pixels[((y * SIZE + x) * 4) as usize] as i32
}

fn fail(seed: u64, what: &str, x: u32, y: u32, expected: &str, actual: i32) -> ! {
    panic!(
        "seed {seed}: {what} at pixel ({x}, {y}): expected {expected}, actual red = {actual} \
         (0 = hole, {SINGLE} = single coverage, 191 = double coverage)"
    );
}

/// No pixel anywhere may receive two full blends.
fn assert_no_double_anywhere(seed: u64, pixels: &[u8]) {
    for y in 0..SIZE {
        for x in 0..SIZE {
            let r = red(pixels, x, y);
            if r >= DOUBLE_MIN {
                fail(seed, "double coverage", x, y, "< 176", r);
            }
        }
    }
}

fn assert_all_hard(seed: u64, poly: &[[f64; 2]], pixels: &[u8]) {
    for y in 0..SIZE {
        for x in 0..SIZE {
            let Some(d) = inside_distance(poly, pixel_world(x, y)) else {
                continue;
            };
            if d <= INSIDE_MARGIN {
                continue;
            }
            let r = red(pixels, x, y);
            if (r - SINGLE).abs() > ENCODE_TOL {
                fail(seed, "all-hard strictly-inside pixel", x, y, "128 +/- 1", r);
            }
        }
    }
    assert_no_double_anywhere(seed, pixels);
}

fn assert_mixed(seed: u64, poly: &[[f64; 2]], pixels: &[u8]) {
    for y in 0..SIZE {
        for x in 0..SIZE {
            let p = pixel_world(x, y);
            let Some(d) = inside_distance(poly, p) else {
                continue;
            };
            // Pixels within a fraction of a subpixel of the silhouette are
            // genuinely owned by the rasteriser's rounding; skip them.
            if d <= INSIDE_MARGIN {
                continue;
            }
            let r = red(pixels, x, y);
            // Strictly inside the silhouette there is always at least half a
            // pixel of coverage: never a hole, never a double blend.
            if r < 50 {
                fail(seed, "mixed strictly-inside hole", x, y, ">= 50", r);
            }
            if r >= DOUBLE_MIN {
                fail(seed, "mixed strictly-inside double", x, y, "< 176", r);
            }
            if d > AA_MARGIN && (r - SINGLE).abs() > ENCODE_TOL {
                fail(
                    seed,
                    "mixed interior beyond the AA band",
                    x,
                    y,
                    "128 +/- 1",
                    r,
                );
            }
        }
    }
    assert_no_double_anywhere(seed, pixels);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// 50 seeded random convex polygons (rotated, non-integer centres), drawn once
/// all-hard and once with a random subset of outer edges anti-aliased.
#[test]
fn hard_edges_random_fans_no_holes_no_double() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness() else {
        return;
    };

    for seed in 0..50u64 {
        let mut rng = Rng::new(seed);
        let hull = random_convex_polygon(&mut rng);
        // The shader receives f32 vertices; evaluate "inside" against those
        // same values (cast back to f64) so the test polygon is exactly the
        // one drawn.
        let verts: Vec<Vec2> = hull
            .iter()
            .map(|p| Vec2::new(p[0] as f32, p[1] as f32))
            .collect();
        let poly: Vec<[f64; 2]> = verts.iter().map(|v| [v.x as f64, v.y as f64]).collect();

        render_fan(&mut h, &verts, &vec![false; verts.len()]);
        let pixels = h.read_pixels();
        assert_all_hard(seed, &poly, &pixels);

        let mut aa = vec![false; verts.len()];
        for bit in aa.iter_mut() {
            *bit = rng.bool();
        }
        // Make sure the case exercises both mixed and all-hard fan triangles.
        if !aa.iter().any(|&b| b) {
            aa[0] = true;
        }
        if aa.iter().all(|&b| b) {
            aa[0] = false;
        }

        render_fan(&mut h, &verts, &aa);
        let pixels = h.read_pixels();
        assert_mixed(seed, &poly, &pixels);
    }
}

/// Two deterministic quads with half-integer corners.  Their diagonals lie
/// exactly on pixel centres (`i + j == 255` for `y = x`, `i == j` for
/// `y = -x`), so the top-left ownership of a hard edge is decided on the line
/// itself.  Both the all-hard and the mixed (diagonal hard, outer edges AA)
/// splits must tile with no hole and no double blend.
#[test]
fn hard_edges_half_integer_diagonals_top_left() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness() else {
        return;
    };

    let p0 = Vec2::new(-50.5, -50.5);
    let p1 = Vec2::new(50.5, -50.5);
    let p2 = Vec2::new(50.5, 50.5);
    let p3 = Vec2::new(-50.5, 50.5);
    let square = [
        [p0.x as f64, p0.y as f64],
        [p1.x as f64, p1.y as f64],
        [p2.x as f64, p2.y as f64],
        [p3.x as f64, p3.y as f64],
    ];
    // Skip pixels that sit on the square's AA outer silhouette (the top edge
    // lies on a row of pixel centres because the corners are half-integer).
    let inside =
        |x: u32, y: u32| inside_distance(&square, pixel_world(x, y)).is_some_and(|d| d > 1.0);

    // `y = x` split: p0-p2 diagonal.  A/B all-hard first.
    render_two(&mut h, (p0, p1, p2, 0), (p0, p2, p3, 0));
    let pixels = h.read_pixels();
    for i in 78..=177u32 {
        let (x, y) = (i, 255 - i);
        if !inside(x, y) {
            continue;
        }
        let r = red(&pixels, x, y);
        if (r - SINGLE).abs() > ENCODE_TOL {
            fail(0, "y=x all-hard diagonal", x, y, "128 +/- 1", r);
        }
    }
    assert_no_double_anywhere(0, &pixels);

    // `y = x` mixed: diagonal hard (bit 2 / bit 0), outer edges AA.
    render_two(&mut h, (p0, p1, p2, 0b011), (p0, p2, p3, 0b110));
    let pixels = h.read_pixels();
    for i in 78..=177u32 {
        let (x, y) = (i, 255 - i);
        if !inside(x, y) {
            continue;
        }
        let r = red(&pixels, x, y);
        if (r - SINGLE).abs() > ENCODE_TOL {
            fail(0, "y=x mixed diagonal", x, y, "128 +/- 1", r);
        }
    }
    assert_no_double_anywhere(0, &pixels);

    // `y = -x` split: p1-p3 diagonal.  All-hard.
    render_two(&mut h, (p0, p1, p3, 0), (p1, p2, p3, 0));
    let pixels = h.read_pixels();
    for i in 78..=177u32 {
        if !inside(i, i) {
            continue;
        }
        let r = red(&pixels, i, i);
        if (r - SINGLE).abs() > ENCODE_TOL {
            fail(0, "y=-x all-hard diagonal", i, i, "128 +/- 1", r);
        }
    }
    assert_no_double_anywhere(0, &pixels);

    // `y = -x` mixed against an all-hard neighbour: the mixed copy's expanded
    // quad reaches across the hard diagonal while the all-hard copy owns the
    // rasterised pixels there.  With a world-space tie-break both copies can
    // write, which is exactly the double blend this test forbids.
    render_two(&mut h, (p0, p1, p3, 0b101), (p1, p2, p3, 0));
    let pixels = h.read_pixels();
    for i in 78..=177u32 {
        if !inside(i, i) {
            continue;
        }
        let r = red(&pixels, i, i);
        if (r - SINGLE).abs() > ENCODE_TOL {
            fail(0, "y=-x mixed vs all-hard diagonal", i, i, "128 +/- 1", r);
        }
    }
    assert_no_double_anywhere(0, &pixels);

    // `y = -x` mixed: C edges = (outer a-b AA, diagonal b-c hard, outer c-a AA)
    // and D edges = (outer a-b AA, outer b-c AA, diagonal c-a hard).
    render_two(&mut h, (p0, p1, p3, 0b101), (p1, p2, p3, 0b011));
    let pixels = h.read_pixels();
    for i in 78..=177u32 {
        if !inside(i, i) {
            continue;
        }
        let r = red(&pixels, i, i);
        if (r - SINGLE).abs() > ENCODE_TOL {
            fail(0, "y=-x mixed diagonal", i, i, "128 +/- 1", r);
        }
    }
    assert_no_double_anywhere(0, &pixels);
}
