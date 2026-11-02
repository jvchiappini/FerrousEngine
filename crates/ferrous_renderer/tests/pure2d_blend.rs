//! Exact-value tests for the backdrop-aware layer blend modes.
//!
//! A full-target backdrop of one grey is drawn, a full-target source layer of
//! another grey is composited over it with each mode, and one pixel is read
//! back.  With opaque source and backdrop the formulas reduce to the per-mode
//! blend `B(b, s)`, so the expected values are computed here directly and the
//! only tolerance is the 8-bit sRGB quantisation of the target.

use ferrous_2d::render::{CompositeMode2d, Layer2d, ShapeInstance};
use ferrous_renderer::headless::{Pure2dHarness, DEFAULT_FORMAT};
use glam::Vec2;

/// Backdrop grey, linear light.
const BACKDROP: f32 = 0.25;
/// Source grey, linear light.
const SOURCE: f32 = 0.75;

/// Linear component to the 8-bit sRGB code the `Rgba8UnormSrgb` target stores.
fn encode(linear: f32) -> u8 {
    let c = linear.clamp(0.0, 1.0);
    let srgb = if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let code = (srgb * 255.0).round().clamp(0.0, 255.0) as u8;
    code
}

/// The per-mode blend at the test greys (opaque source and backdrop).
fn expected(mode: CompositeMode2d) -> f32 {
    let b = BACKDROP;
    let s = SOURCE;
    match mode {
        CompositeMode2d::Normal => s,
        CompositeMode2d::Add => (s + b).min(1.0),
        CompositeMode2d::Multiply => s * b,
        CompositeMode2d::Screen => s + b - s * b,
        CompositeMode2d::Overlay => {
            if s < 0.5 {
                2.0 * b * s
            } else {
                1.0 - 2.0 * (1.0 - b) * (1.0 - s)
            }
        }
        CompositeMode2d::Darken => b.min(s),
        CompositeMode2d::Lighten => b.max(s),
    }
}

fn render_mode(mode: CompositeMode2d) -> [u8; 4] {
    render_pair(mode, BACKDROP, SOURCE)
}

fn render_pair(mode: CompositeMode2d, backdrop: f32, source: f32) -> [u8; 4] {
    let mut harness = match Pure2dHarness::new(8, 8, DEFAULT_FORMAT) {
        Ok(harness) => harness,
        Err(error) => panic!("no usable adapter: {error}"),
    };
    harness.set_clear_color(wgpu::Color::TRANSPARENT);
    harness.draw_shape(ShapeInstance::rect(
        Vec2::ZERO,
        Vec2::new(8.0, 8.0),
        [backdrop, backdrop, backdrop, 1.0],
    ));
    harness
        .push_layer(Layer2d::new(1.0, mode, 1.0))
        .expect("layer push");
    harness.draw_shape(ShapeInstance::rect(
        Vec2::ZERO,
        Vec2::new(8.0, 8.0),
        [source, source, source, 1.0],
    ));
    harness.pop_layer().expect("layer pop");
    harness.render_frame_blocking();
    harness.read_pixel(4, 4)
}

#[test]
fn backdrop_modes_match_the_formulas() {
    let modes = [
        CompositeMode2d::Normal,
        CompositeMode2d::Add,
        CompositeMode2d::Multiply,
        CompositeMode2d::Screen,
        CompositeMode2d::Overlay,
        CompositeMode2d::Darken,
        CompositeMode2d::Lighten,
    ];
    for mode in modes {
        let pixel = render_mode(mode);
        let want = encode(expected(mode));
        for channel in &pixel[..3] {
            let difference = i16::from(*channel) - i16::from(want);
            assert!(
                difference.unsigned_abs() <= 2,
                "{}: got {pixel:?}, expected {want}",
                mode.label()
            );
        }
        assert_eq!(pixel[3], 255, "{}: alpha", mode.label());
    }
}

#[test]
fn backdrop_modes_are_distinguishable() {
    // Guards against a wiring mistake that renders every advanced mode as
    // normal.  Lighten needs a source darker than the backdrop to differ.
    for (mode, backdrop, source) in [
        (CompositeMode2d::Overlay, BACKDROP, SOURCE),
        (CompositeMode2d::Darken, BACKDROP, SOURCE),
        (CompositeMode2d::Lighten, SOURCE, BACKDROP),
    ] {
        let pixel = render_pair(mode, backdrop, source);
        let normal = encode(source);
        let difference = i16::from(pixel[0]) - i16::from(normal);
        assert!(
            difference.unsigned_abs() > 2,
            "{} rendered like normal ({pixel:?})",
            mode.label()
        );
    }
}
