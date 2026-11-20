//! Pure2D memory inventory is flat at steady state.
//!
//! The test renders a scene with shapes, text, an offscreen layer and a blur
//! effect for a warm-up period, then compares two [`Renderer::memory_report`]
//! snapshots 30 frames apart.  It fails if a per-frame resource (a layer or
//! effect-target pool, an effect uniform, a composite buffer, ...) is
//! registered again each frame or keeps growing instead of being reused.
//!
//! The GPU test skips cleanly, with a printed reason, when no adapter is
//! available.

mod common;

use ferrous_2d::render::{CompositeMode2d, Effect2d, FontId2d, Layer2d, ShapeInstance};
use ferrous_font::TextAlign;
use ferrous_renderer::headless::{Pure2dHarness, DEFAULT_FORMAT};
use glam::{Mat4, Vec2, Vec3};

const SIZE: u32 = 256;
const WARMUP_FRAMES: usize = 30;
const MEASURE_FRAMES: usize = 30;

/// Allowed drift between the two snapshots, in bytes.
///
/// After 30 warm-up frames every pooled allocation exists, so the steady-state
/// delta is zero.  256 bytes leaves room for a single late 64-byte uniform or
/// buffer registration, while a leak of one 32-byte entry per frame over the 30
/// measured frames (960 bytes) already exceeds it.  The layer-pool counter is
/// asserted separately, where a leaked 256x256 RGBA16Float layer (512 KiB) is
/// unmistakable.
const TOLERANCE_BYTES: u64 = 256;

fn rect(center: Vec2, size: Vec2, color: [f32; 4], z: f32) -> ShapeInstance {
    ShapeInstance::rect_z(center, z, size, color)
}

/// Registers Roboto when the repository font is present, so a checkout without
/// assets still runs the shapes/layers part of the scene.
fn register_font(harness: &mut Pure2dHarness) -> Option<FontId2d> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/fonts/Roboto-Regular.ttf");
    let bytes = std::fs::read(path).ok()?;
    harness
        .register_font(&bytes, ferrous_font::charset::ascii())
        .ok()
}

fn draw_scene(harness: &mut Pure2dHarness, font: Option<FontId2d>) {
    harness.begin_frame();

    // Interleaved shapes, enough to exercise the batchers and the 2D draw list.
    for i in 0..12 {
        let x = -110.0 + i as f32 * 20.0;
        let y = 30.0 + (i % 3) as f32 * 24.0;
        harness.draw_shape(rect(
            Vec2::new(x, y),
            Vec2::splat(18.0),
            [0.2, 0.55, 0.9, 1.0],
            i as f32 * 0.01,
        ));
    }
    for i in 0..6 {
        harness.draw_shape(ShapeInstance::circle_z(
            Vec2::new(-90.0 + i as f32 * 36.0, 70.0),
            i as f32 * 0.01,
            12.0,
            [1.0, 0.45, 0.1, 1.0],
        ));
    }

    if let Some(font) = font {
        harness.draw_text(
            font,
            "flat inventory",
            Mat4::from_translation(Vec3::new(0.0, -70.0, 0.0)),
            24.0,
            TextAlign::Center,
            [0.95, 0.95, 0.95, 1.0],
        );
    }

    // One offscreen layer with an effect: allocates a layer texture, two effect
    // scratch targets and an effect uniform on first use, then reuses them.
    harness
        .push_layer_with_effects(
            Layer2d {
                bounds: None,
                opacity: 0.8,
                blend: CompositeMode2d::Screen,
                depth: 0.5,
            },
            &[Effect2d::Blur { radius: 2.0 }],
        )
        .expect("push an offscreen layer with a blur effect");
    harness.draw_shape(rect(
        Vec2::new(-40.0, -110.0),
        Vec2::new(70.0, 32.0),
        [1.0, 0.3, 0.2, 0.7],
        0.0,
    ));
    harness.draw_shape(rect(
        Vec2::new(0.0, -110.0),
        Vec2::new(70.0, 32.0),
        [0.1, 1.0, 0.5, 0.7],
        0.1,
    ));
    harness.pop_layer().expect("pop the offscreen layer");

    harness.render_frame();
}

#[test]
fn memory_totals_are_flat_after_warm_up() {
    let _gpu = common::lock();

    let mut harness = match Pure2dHarness::new(SIZE, SIZE, DEFAULT_FORMAT) {
        Ok(harness) => harness,
        Err(error) => {
            println!("[skip] headless GPU unavailable: {error}");
            return;
        }
    };
    let font = register_font(&mut harness);

    for _ in 0..WARMUP_FRAMES {
        draw_scene(&mut harness, font);
    }

    let before = harness.renderer().memory_report();
    let layers_before = harness.layer_textures_allocated();

    for _ in 0..MEASURE_FRAMES {
        draw_scene(&mut harness, font);
    }

    let after = harness.renderer().memory_report();
    let layers_after = harness.layer_textures_allocated();

    assert!(
        before.total_bytes > 0,
        "the inventory must register the renderer's resources"
    );
    assert_eq!(
        layers_before, layers_after,
        "the layer texture pool must be reused, not reallocated per frame"
    );
    assert!(
        after.total_bytes.abs_diff(before.total_bytes) <= TOLERANCE_BYTES,
        "total bytes drifted by {} over {MEASURE_FRAMES} frames (before {}, after {}); \
         a per-frame cache is growing",
        after.total_bytes.abs_diff(before.total_bytes),
        before.total_bytes,
        after.total_bytes,
    );
    assert!(
        after.gpu_bytes.abs_diff(before.gpu_bytes) <= TOLERANCE_BYTES,
        "GPU bytes drifted by {} over {MEASURE_FRAMES} frames (before {}, after {})",
        after.gpu_bytes.abs_diff(before.gpu_bytes),
        before.gpu_bytes,
        after.gpu_bytes,
    );
    assert!(
        after.cpu_bytes.abs_diff(before.cpu_bytes) <= TOLERANCE_BYTES,
        "CPU bytes drifted by {} over {MEASURE_FRAMES} frames (before {}, after {})",
        after.cpu_bytes.abs_diff(before.cpu_bytes),
        before.cpu_bytes,
        after.cpu_bytes,
    );
}
