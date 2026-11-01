//! Per-draw blend-mode tests for the Pure2D compositing contract.
//!
//! The expected values are computed from the documented formulas in
//! premultiplied linear light; the target is `Rgba8Unorm`, so the readback is
//! the linear value quantised to 8 bits.  Each test skips cleanly when no GPU
//! adapter is available.

use std::sync::Mutex;

use ferrous_2d::render::{BlendMode2d, ShapeInstance};
use ferrous_renderer::headless::Pure2dHarness;
use glam::Vec2;
use wgpu::TextureFormat;

static GPU_LOCK: Mutex<()> = Mutex::new(());

const SIZE: u32 = 64;

fn harness() -> Option<Pure2dHarness> {
    match Pure2dHarness::new(SIZE, SIZE, TextureFormat::Rgba8Unorm) {
        Ok(h) => Some(h),
        Err(err) => {
            println!("[skip] headless GPU unavailable: {err}");
            None
        }
    }
}

fn at(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * SIZE + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

fn quantise(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Blend one premultiplied source over a straight-alpha destination, returning
/// straight RGBA (what a compositor would report).
fn expected(mode: BlendMode2d, src: [f32; 4], dst: [f32; 4]) -> [u8; 4] {
    let alpha = src[3] + dst[3] * (1.0 - src[3]);
    let rgb = match mode {
        BlendMode2d::Normal => {
            let mut out = [0.0; 3];
            for c in 0..3 {
                out[c] = src[c] + dst[c] * dst[3] * (1.0 - src[3]);
            }
            out
        }
        BlendMode2d::Add => {
            let mut out = [0.0; 3];
            for c in 0..3 {
                out[c] = src[c] + dst[c] * dst[3];
            }
            out
        }
        BlendMode2d::Multiply => {
            let mut out = [0.0; 3];
            for c in 0..3 {
                out[c] = src[c] * dst[c] + dst[c] * dst[3] * (1.0 - src[3]);
            }
            out
        }
        BlendMode2d::Screen => {
            let mut out = [0.0; 3];
            for c in 0..3 {
                out[c] = src[c] + dst[c] * dst[3] * (1.0 - src[c]);
            }
            out
        }
        _ => unreachable!("non_exhaustive enum covered"),
    };
    let inv = if alpha > 0.0 { 1.0 / alpha } else { 0.0 };
    [
        quantise(rgb[0] * inv),
        quantise(rgb[1] * inv),
        quantise(rgb[2] * inv),
        quantise(alpha),
    ]
}

fn assert_near(actual: [u8; 4], expected: [u8; 4], what: &str) {
    for channel in 0..4 {
        let diff = actual[channel].abs_diff(expected[channel]);
        assert!(
            diff <= 2,
            "{what}: channel {channel} was {} (expected {}); full {actual:?} vs {expected:?}",
            actual[channel],
            expected[channel]
        );
    }
}

/// Each mode over a 2x2 grid of opaque backdrops matches its formula.
#[test]
fn each_blend_mode_matches_its_formula_on_a_colour_grid() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness() else {
        return;
    };

    let backdrops = [
        [0.0f32, 0.0, 0.5, 1.0],
        [0.5, 0.0, 0.0, 1.0],
        [0.0, 0.5, 0.0, 1.0],
        [0.25, 0.25, 0.25, 1.0],
    ];
    let cells = [
        (-16.0f32, 16.0),
        (16.0, 16.0),
        (-16.0, -16.0),
        (16.0, -16.0),
    ];
    let src = [1.0f32, 1.0, 1.0, 0.5];
    let src_premul = [0.5f32, 0.5, 0.5, 0.5];

    for mode in BlendMode2d::ALL {
        h.begin_frame();
        h.set_blend_mode(BlendMode2d::Normal);
        for (i, colour) in backdrops.iter().enumerate() {
            h.draw_shape(ShapeInstance::rect(
                Vec2::new(cells[i].0, cells[i].1),
                Vec2::splat(28.0),
                *colour,
            ));
        }
        h.set_blend_mode(mode);
        for (i, _) in backdrops.iter().enumerate() {
            h.draw_shape(ShapeInstance::rect(
                Vec2::new(cells[i].0, cells[i].1),
                Vec2::splat(20.0),
                src,
            ));
        }
        h.render_frame();
        let pixels = h.read_pixels();

        for (i, colour) in backdrops.iter().enumerate() {
            let x = (32.0 + cells[i].0) as u32;
            let y = (32.0 - cells[i].1) as u32;
            assert_near(
                at(&pixels, x, y),
                expected(mode, src_premul, *colour),
                &format!("{mode:?} over {colour:?}"),
            );
        }
    }
}

/// Changing the blend mode between two draws of the same kind splits their
/// coalesced run into separate draw calls, and z order is respected across
/// modes.
#[test]
fn mode_change_breaks_the_batch_and_z_order_is_kept() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness() else {
        return;
    };

    h.begin_frame();
    // z = 0 red normal, z = 1 white add, z = 2 blue normal.
    h.draw_shape(ShapeInstance::rect_z(
        Vec2::ZERO,
        0.0,
        Vec2::splat(48.0),
        [1.0, 0.0, 0.0, 1.0],
    ));
    h.set_blend_mode(BlendMode2d::Add);
    h.draw_shape(ShapeInstance::rect_z(
        Vec2::ZERO,
        1.0,
        Vec2::splat(36.0),
        [1.0, 1.0, 1.0, 0.5],
    ));
    h.set_blend_mode(BlendMode2d::Normal);
    h.draw_shape(ShapeInstance::rect_z(
        Vec2::ZERO,
        2.0,
        Vec2::splat(20.0),
        [0.0, 0.0, 1.0, 1.0],
    ));
    let report = h.render_frame();
    let pixels = h.read_pixels();

    assert_eq!(
        report.shape_draw_calls, 3,
        "a mode change must split the shape run"
    );
    // The z = 2 blue rect wins at the centre even though the mode changed.
    assert_near(
        at(&pixels, 32, 32),
        [0, 0, 255, 255],
        "z order across blend modes",
    );
    // Outside the blue rect the add blend remains visible: red + 50 % white is
    // (1, 0.5, 0) in premultiplied terms over the red backdrop.
    let outside = at(&pixels, 32 - 14, 32);
    assert!(
        outside[0] > 240 && outside[1] > 100 && outside[1] < 160,
        "add blend must be visible around the blue rect, got {outside:?}"
    );
}

/// Same frame twice gives identical bytes even with per-draw blend modes.
#[test]
fn blend_modes_are_deterministic() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness() else {
        return;
    };

    let mut render = || {
        h.begin_frame();
        h.draw_shape(ShapeInstance::rect(
            Vec2::ZERO,
            Vec2::splat(40.0),
            [0.0, 0.25, 0.5, 1.0],
        ));
        h.set_blend_mode(BlendMode2d::Screen);
        h.draw_shape(ShapeInstance::rect(
            Vec2::new(4.0, 4.0),
            Vec2::splat(24.0),
            [1.0, 0.0, 0.0, 0.5],
        ));
        h.set_blend_mode(BlendMode2d::Multiply);
        h.draw_shape(ShapeInstance::rect(
            Vec2::new(-4.0, -4.0),
            Vec2::splat(24.0),
            [0.0, 1.0, 0.0, 0.5],
        ));
        h.render_frame();
        h.read_pixels()
    };

    assert_eq!(
        render(),
        render(),
        "same frame twice must give identical bytes"
    );
}

/// The blend state resets to Normal at the start of every frame.
#[test]
fn blend_mode_resets_each_frame() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness() else {
        return;
    };

    h.begin_frame();
    h.set_blend_mode(BlendMode2d::Multiply);
    h.draw_shape(ShapeInstance::rect(
        Vec2::ZERO,
        Vec2::splat(20.0),
        [1.0, 1.0, 1.0, 1.0],
    ));
    h.render_frame();
    assert_eq!(h.blend_mode(), BlendMode2d::Multiply);

    h.begin_frame();
    assert_eq!(
        h.blend_mode(),
        BlendMode2d::Normal,
        "begin_frame must reset the blend state"
    );
}
