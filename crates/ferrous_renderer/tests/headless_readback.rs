//! Typed, allocation-reusing and pipelined readback tests.
//!
//! Every test creates a windowless Pure2D harness.  When no GPU adapter is
//! available the helper prints the reason and the test returns early instead of
//! being `#[ignore]`d, so the suite stays honest on CPU-only CI.

use std::sync::Mutex;

use ferrous_2d::render::ShapeInstance;
use ferrous_renderer::headless::{HarnessError, OffscreenTarget, Pure2dHarness, ReadbackError};
use glam::Vec2;
use wgpu::TextureFormat;

static GPU_LOCK: Mutex<()> = Mutex::new(());

const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

fn harness(width: u32, height: u32, format: TextureFormat) -> Option<Pure2dHarness> {
    match Pure2dHarness::new(width, height, format) {
        Ok(h) => Some(h),
        Err(err) => {
            println!("[skip] headless GPU unavailable: {err}");
            None
        }
    }
}

fn render_rect(h: &mut Pure2dHarness, color: [f32; 4]) {
    h.begin_frame();
    h.draw_shape(ShapeInstance::rect(Vec2::ZERO, Vec2::splat(64.0), color));
    h.render_frame();
}

fn at(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * width + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

#[test]
fn typed_readback_matches_legacy_readback() {
    let _guard = GPU_LOCK.lock().unwrap();
    let Some(mut h) = harness(64, 64, TextureFormat::Rgba8Unorm) else {
        return;
    };
    render_rect(&mut h, RED);

    let legacy = h.read_pixels();
    let mut typed = Vec::new();
    h.read_pixels_into(&mut typed)
        .expect("typed readback must succeed");

    assert_eq!(typed, legacy, "typed and legacy readback must be identical");
    assert_eq!(typed.len(), 64 * 64 * 4);
    assert_eq!(h.target().staging_allocations(), 1);
}

#[test]
fn staging_buffer_and_output_allocation_are_reused() {
    let _guard = GPU_LOCK.lock().unwrap();
    let Some(mut h) = harness(64, 64, TextureFormat::Rgba8Unorm) else {
        return;
    };
    render_rect(&mut h, RED);

    let mut out = Vec::new();
    h.read_pixels_into(&mut out).expect("first readback");
    let capacity = out.capacity();
    let ptr = out.as_ptr();
    let allocations = h.target().staging_allocations();

    for _ in 0..100 {
        h.read_pixels_into(&mut out).expect("steady-state readback");
    }

    assert_eq!(
        h.target().staging_allocations(),
        allocations,
        "100 reads must reuse the one persistent staging buffer"
    );
    assert_eq!(out.capacity(), capacity, "the output Vec must not grow");
    assert_eq!(out.as_ptr(), ptr, "the output Vec must not reallocate");
}

#[test]
fn pipelined_readback_returns_frames_in_submission_order() {
    let _guard = GPU_LOCK.lock().unwrap();
    let Some(mut h) = harness(32, 32, TextureFormat::Rgba8Unorm) else {
        return;
    };

    // Frame N's copy overlaps frame N+1's rendering: two copies may be in
    // flight, and the oldest is consumed before submitting the next frame.
    render_rect(&mut h, RED);
    h.submit_readback().expect("submit red");
    render_rect(&mut h, GREEN);
    h.submit_readback().expect("submit green");

    let mut out = Vec::new();
    assert!(h.take_readback(&mut out).expect("take red"));
    assert_eq!(at(&out, 32, 16, 16), [255, 0, 0, 255], "first frame is red");

    render_rect(&mut h, BLUE);
    h.submit_readback().expect("submit blue");

    assert!(h.take_readback(&mut out).expect("take green"));
    assert_eq!(
        at(&out, 32, 16, 16),
        [0, 255, 0, 255],
        "second frame is green"
    );

    assert!(h.take_readback(&mut out).expect("take blue"));
    assert_eq!(
        at(&out, 32, 16, 16),
        [0, 0, 255, 255],
        "third frame is blue"
    );

    assert!(
        !h.take_readback(&mut out).expect("nothing pending"),
        "take_readback must return false when no copy is pending"
    );
}

#[test]
fn pipelined_readback_rejects_a_third_submit_while_two_are_pending() {
    let _guard = GPU_LOCK.lock().unwrap();
    // A deliberately small two-slot ring: the default ring is three slots.
    let mut h = match Pure2dHarness::with_readback_ring(16, 16, TextureFormat::Rgba8Unorm, 2) {
        Ok(h) => h,
        Err(err) => {
            println!("[skip] headless GPU unavailable: {err}");
            return;
        }
    };
    render_rect(&mut h, RED);
    assert_eq!(h.submit_readback().expect("first submit"), 0);
    assert_eq!(h.submit_readback().expect("second submit"), 1);
    let pending = h.pending_readbacks();
    let next = h.next_frame_index();
    assert_eq!(
        h.submit_readback(),
        Err(ReadbackError::Busy),
        "the two-slot ring must report Busy, not silently drop a frame"
    );
    assert_eq!(
        h.pending_readbacks(),
        pending,
        "a rejected submit must not change the pending queue"
    );
    assert_eq!(
        h.next_frame_index(),
        next,
        "a rejected submit must not consume a frame index"
    );

    let mut out = Vec::new();
    assert!(h.take_readback(&mut out).expect("take oldest"));
    assert!(h.take_readback(&mut out).expect("take newest"));
    assert_eq!(h.pending_readbacks(), 0);
}

#[test]
fn read_pixel_matches_full_readback() {
    let _guard = GPU_LOCK.lock().unwrap();
    let Some(mut h) = harness(64, 64, TextureFormat::Rgba8Unorm) else {
        return;
    };
    h.begin_frame();
    h.draw_shape(ShapeInstance::rect(
        Vec2::new(-16.0, 8.0),
        Vec2::new(24.0, 12.0),
        RED,
    ));
    h.draw_shape(ShapeInstance::circle(Vec2::new(18.0, -10.0), 9.0, BLUE));
    h.render_frame();

    let full = h.read_pixels();
    for &(x, y) in &[(0u32, 0u32), (32, 32), (20, 28), (48, 40), (63, 63)] {
        assert_eq!(
            h.read_pixel(x, y),
            at(&full, 64, x, y),
            "single-texel read at ({x}, {y}) must match the full readback"
        );
        assert_eq!(
            h.read_pixel_checked(x, y).expect("typed pixel"),
            at(&full, 64, x, y)
        );
    }
}

#[test]
fn unsupported_format_returns_typed_error_instead_of_panicking() {
    let _guard = GPU_LOCK.lock().unwrap();
    let Some(h) = harness(16, 16, TextureFormat::Rgba8Unorm) else {
        return;
    };
    let target = OffscreenTarget::new(h.device(), 16, 16, TextureFormat::Rgba16Float);

    let mut out = Vec::new();
    let err = target
        .read_rgba8_into(h.device(), h.queue(), &mut out)
        .expect_err("Rgba16Float readback must fail cleanly");
    assert_eq!(
        err,
        ReadbackError::UnsupportedFormat {
            format: TextureFormat::Rgba16Float
        }
    );

    let err = target
        .read_pixel_into(h.device(), h.queue(), 0, 0)
        .expect_err("single-pixel Rgba16Float readback must fail cleanly");
    assert!(matches!(err, ReadbackError::UnsupportedFormat { .. }));
}

#[test]
fn out_of_bounds_pixel_returns_typed_error() {
    let _guard = GPU_LOCK.lock().unwrap();
    let Some(h) = harness(8, 8, TextureFormat::Rgba8Unorm) else {
        return;
    };
    let err = h
        .read_pixel_checked(8, 0)
        .expect_err("x == width must be out of bounds");
    assert_eq!(
        err,
        ReadbackError::OutOfBounds {
            x: 8,
            y: 0,
            width: 8,
            height: 8
        }
    );
}

#[test]
fn harness_rejects_unsupported_formats_with_a_typed_error() {
    let _guard = GPU_LOCK.lock().unwrap();
    let err = Pure2dHarness::new(8, 8, TextureFormat::Rgba16Float)
        .err()
        .expect("Rgba16Float harness creation must fail");
    assert_eq!(
        err,
        HarnessError::UnsupportedFormat {
            format: TextureFormat::Rgba16Float
        }
    );
}

#[test]
fn with_clear_color_sets_the_frame_background() {
    let _guard = GPU_LOCK.lock().unwrap();
    let clear = wgpu::Color {
        r: 0.0,
        g: 0.0,
        b: 1.0,
        a: 1.0,
    };
    let mut h = match Pure2dHarness::with_clear_color(16, 16, TextureFormat::Rgba8Unorm, clear) {
        Ok(h) => h,
        Err(err) => {
            println!("[skip] headless GPU unavailable: {err}");
            return;
        }
    };

    h.begin_frame();
    h.render_frame();
    assert_eq!(
        h.read_pixel(8, 8),
        [0, 0, 255, 255],
        "the explicit clear colour must be the background"
    );
}
