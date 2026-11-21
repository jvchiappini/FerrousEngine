//! GPU regression test for the multi-viewport Pure2D camera.
//!
//! The hard-edge fill rule in `shape_2d.wgsl` compares `@builtin(position)`
//! (which is always in **full framebuffer** coordinates) against the triangle's
//! framebuffer-space vertices.  Before viewport origins existed in the 2D
//! camera uniform, the vertex stage mapped NDC onto `0..view_size` only, so any
//! view whose origin was not `(0, 0)` clipped hard edges at the wrong place.
//!
//! This test renders the same mixed triangle fan in two side-by-side views (the
//! second at `x = width / 2`) and requires the right view's pixels to equal the
//! left view's pixels shifted by `width / 2`.  It fails before the fix: the
//! right view's hard edges are offset by the view's origin.
//!
//! Headless GPU test: it skips cleanly (printing `[skip] ...`) when no adapter
//! is available, following `tests/pure2d_headless.rs`.

use std::sync::Mutex;

use ferrous_2d::render::ShapeInstance;
use ferrous_renderer::headless::Pure2dHarness;
use glam::{Mat4, Vec2};
use wgpu::TextureFormat;

/// One software/real device per process at a time, like `pure2d_headless.rs`.
static GPU_LOCK: Mutex<()> = Mutex::new(());

const VIEW: u32 = 256;
const TOTAL_W: u32 = VIEW * 2;
const TOTAL_H: u32 = VIEW;
const HALF_WORLD: f32 = VIEW as f32 * 0.5;

/// A quad fanned from its centroid.  The vertices are deliberately off the
/// pixel grid so the hard edge crosses pixel centres.
fn fan() -> Vec<(Vec2, Vec2, Vec2, u8)> {
    let hull = [
        Vec2::new(-70.5, -60.5),
        Vec2::new(60.5, -40.5),
        Vec2::new(70.5, 60.5),
        Vec2::new(-50.5, 50.5),
    ];
    let centroid = hull.iter().fold(Vec2::ZERO, |acc, v| acc + *v) / hull.len() as f32;
    hull.iter()
        .enumerate()
        .map(|(i, a)| {
            let b = hull[(i + 1) % hull.len()];
            // Mixed triangles: the outer edge is anti-aliased, the two spokes
            // stay hard, so the hard-edge fill rule is exercised.
            (centroid, *a, b, 0b010)
        })
        .collect()
}

fn harness() -> Option<Pure2dHarness> {
    match Pure2dHarness::new(TOTAL_W, TOTAL_H, TextureFormat::Rgba8Unorm) {
        Ok(h) => Some(h),
        Err(err) => {
            println!("[skip] headless GPU unavailable: {err}");
            None
        }
    }
}

/// Renders the same fan into the left and the right view.
fn render_both(h: &mut Pure2dHarness, view_proj: Mat4) {
    h.begin_frame();
    {
        let renderer = h.renderer_mut();
        for rect in [
            [0.0, 0.0, VIEW as f32, VIEW as f32],
            [VIEW as f32, 0.0, VIEW as f32, VIEW as f32],
        ] {
            renderer.begin_2d_view(rect, view_proj);
            for (a, b, c, mask) in fan() {
                renderer.draw_2d_shape(ShapeInstance::polygon_triangle(
                    a,
                    b,
                    c,
                    [0.8, 0.4, 0.2, 0.9],
                    mask,
                ));
            }
            renderer.end_2d_view();
        }
    }
    h.render_frame();
}

#[test]
fn multiview_hard_edges_match_the_left_view_shifted() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness() else {
        return;
    };

    let view_proj = Mat4::orthographic_rh(
        -HALF_WORLD,
        HALF_WORLD,
        -HALF_WORLD,
        HALF_WORLD,
        -100.0,
        100.0,
    );
    render_both(&mut h, view_proj);
    let pixels = h.read_pixels();

    let mut mismatches = 0u32;
    let mut first = None;
    for y in 0..VIEW {
        for x in 0..VIEW {
            let left = ((y * TOTAL_W + x) * 4) as usize;
            let right = ((y * TOTAL_W + x + VIEW) * 4) as usize;
            if pixels[left..left + 4] != pixels[right..right + 4] {
                mismatches += 1;
                if first.is_none() {
                    first = Some((
                        x,
                        y,
                        pixels[left..left + 4].to_vec(),
                        pixels[right..right + 4].to_vec(),
                    ));
                }
            }
        }
    }

    if let Some((x, y, left, right)) = first {
        panic!(
            "multi-view hard edges differ at ({x}, {y}): left {left:?} vs right {right:?} \
             ({mismatches} mismatched pixels); the right view's framebuffer mapping must \
             include its viewport origin"
        );
    }
}
