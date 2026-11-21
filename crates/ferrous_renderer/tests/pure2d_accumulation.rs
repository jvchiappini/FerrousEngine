//! Temporal accumulation tests for the Pure2D path: exact linear averages,
//! determinism, static-scene equality and typed errors.
//!
//! Every test skips cleanly (with a printed reason) when no GPU adapter is
//! available, mirroring the other headless tests.

use std::sync::Mutex;

use ferrous_2d::render::{Accumulation2d, AccumulationError, ShapeInstance};
use ferrous_renderer::headless::{Pure2dHarness, DEFAULT_FORMAT};
use glam::Vec2;

static GPU_LOCK: Mutex<()> = Mutex::new(());

fn harness(width: u32, height: u32, clear: wgpu::Color) -> Option<Pure2dHarness> {
    match Pure2dHarness::with_clear_color(width, height, DEFAULT_FORMAT, clear) {
        Ok(harness) => Some(harness),
        Err(error) => {
            println!("[skip] headless GPU unavailable: {error}");
            None
        }
    }
}

fn pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * width + x) * 4) as usize;
    [
        pixels[offset],
        pixels[offset + 1],
        pixels[offset + 2],
        pixels[offset + 3],
    ]
}

fn near(actual: [u8; 4], expected: [u8; 3], tolerance: u8, what: &str) {
    for (channel, (&got, &want)) in actual[..3].iter().zip(expected.iter()).enumerate() {
        assert!(
            got.abs_diff(want) <= tolerance,
            "{what}: channel {channel} = {got}, expected {want} +/- {tolerance} ({actual:?})"
        );
    }
}

/// Black then white at 0.5/0.5 is linear 0.5, which encodes to sRGB ~188.
#[test]
fn black_then_white_average_to_linear_half() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(4, 4, wgpu::Color::BLACK) else {
        return;
    };
    h.begin_accumulation_2d().unwrap();

    h.begin_frame();
    h.render_frame_blocking();
    h.accumulate_frame_2d(0.5).unwrap();

    h.set_clear_color(wgpu::Color::WHITE);
    h.begin_frame();
    h.render_frame_blocking();
    h.accumulate_frame_2d(0.5).unwrap();

    h.resolve_accumulation_2d().unwrap();
    let pixels = h.read_pixels();
    near(pixel(&pixels, 4, 1, 1), [188, 188, 188], 2, "linear half");
}

/// A square that appears in one of four sub-frames covers a pixel for exactly
/// one quarter of the accumulated time: linear 0.25, sRGB ~137.
#[test]
fn a_pixel_covered_in_one_of_four_subframes_is_linear_quarter() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(8, 8, wgpu::Color::BLACK) else {
        return;
    };
    h.begin_accumulation_2d().unwrap();

    for index in 0..4 {
        h.begin_frame();
        if index == 0 {
            // Bottom-left 4x4 block; the camera is centred, so this covers
            // composition x/y in [-4, 0).
            h.draw_shape(ShapeInstance::rect(
                Vec2::new(-2.0, -2.0),
                Vec2::new(4.0, 4.0),
                [1.0, 1.0, 1.0, 1.0],
            ));
        }
        h.render_frame_blocking();
        h.accumulate_frame_2d(1.0).unwrap();
    }

    h.resolve_accumulation_2d().unwrap();
    let pixels = h.read_pixels();
    // Readback row 0 is the top; the block is at the bottom-left.
    near(pixel(&pixels, 8, 1, 6), [137, 137, 137], 3, "one quarter");
    near(pixel(&pixels, 8, 6, 1), [0, 0, 0], 1, "uncovered");
}

/// Uneven weights normalise by their exact sum: black at 1 and white at 3 is
/// linear 0.75, sRGB ~225.
#[test]
fn uneven_weights_normalise() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(4, 4, wgpu::Color::BLACK) else {
        return;
    };
    h.begin_accumulation_2d().unwrap();

    h.begin_frame();
    h.render_frame_blocking();
    h.accumulate_frame_2d(1.0).unwrap();

    h.set_clear_color(wgpu::Color::WHITE);
    h.begin_frame();
    h.render_frame_blocking();
    h.accumulate_frame_2d(3.0).unwrap();

    h.resolve_accumulation_2d().unwrap();
    let pixels = h.read_pixels();
    near(
        pixel(&pixels, 4, 1, 1),
        [225, 225, 225],
        2,
        "three quarters",
    );
}

/// Two identical runs are byte-identical.
#[test]
fn accumulation_is_deterministic() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(4, 4, wgpu::Color::BLACK) else {
        return;
    };
    let run = |h: &mut Pure2dHarness| -> Vec<u8> {
        h.begin_accumulation_2d().unwrap();
        for (clear, weight) in [
            (wgpu::Color::BLACK, 0.25),
            (wgpu::Color::WHITE, 0.25),
            (wgpu::Color::BLACK, 0.5),
        ] {
            h.set_clear_color(clear);
            h.begin_frame();
            h.render_frame_blocking();
            h.accumulate_frame_2d(weight).unwrap();
        }
        h.resolve_accumulation_2d().unwrap();
        h.read_pixels()
    };
    let first = run(&mut h);
    let second = run(&mut h);
    assert_eq!(first, second);
}

/// A static scene accumulated N times equals one direct render byte for byte.
#[test]
fn a_static_scene_accumulated_n_times_equals_one_render() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(8, 8, wgpu::Color::BLACK) else {
        return;
    };
    let draw = |h: &mut Pure2dHarness| {
        h.begin_frame();
        h.draw_shape(ShapeInstance::rect(
            Vec2::new(1.0, 1.0),
            Vec2::new(3.0, 3.0),
            [1.0, 1.0, 1.0, 1.0],
        ));
        h.render_frame_blocking();
    };
    draw(&mut h);
    let single = h.read_pixels();

    h.begin_accumulation_2d().unwrap();
    for _ in 0..4 {
        draw(&mut h);
        h.accumulate_frame_2d(0.25).unwrap();
    }
    h.resolve_accumulation_2d().unwrap();
    let accumulated = h.read_pixels();
    assert_eq!(single, accumulated);
}

/// Misuse is a typed error, never a panic.
#[test]
fn accumulation_errors_are_typed() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(4, 4, wgpu::Color::BLACK) else {
        return;
    };
    assert_eq!(
        h.resolve_accumulation_2d().unwrap_err(),
        AccumulationError::NotBegun
    );
    assert_eq!(
        h.accumulate_frame_2d(1.0).unwrap_err(),
        AccumulationError::NotBegun
    );

    h.begin_accumulation_2d().unwrap();
    assert_eq!(
        h.resolve_accumulation_2d().unwrap_err(),
        AccumulationError::ZeroWeight
    );
    h.begin_frame();
    h.render_frame_blocking();
    assert_eq!(
        h.accumulate_frame_2d(-1.0).unwrap_err(),
        AccumulationError::InvalidWeight { weight: -1.0 }
    );
    assert!(matches!(
        h.accumulate_frame_2d(f32::NAN).unwrap_err(),
        AccumulationError::InvalidWeight { weight } if weight.is_nan()
    ));

    // A target above the device limit is rejected before any allocation.
    let limit = h.device().limits().max_texture_dimension_2d;
    assert!(matches!(
        Accumulation2d::new(h.renderer().context.device.clone(), limit + 1, 1),
        Err(AccumulationError::TooLarge { .. })
    ));
}
