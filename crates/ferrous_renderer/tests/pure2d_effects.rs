//! Exact-value and behaviour tests for layer effects (blur, drop shadow,
//! glow).
//!
//! Every test draws a known shape into an offscreen layer, applies one effect
//! chain and reads pixels back.  Where the maths is exact (radius zero, shadow
//! opacity, glow intensity) the expected 8-bit code is computed here directly;
//! the Gaussian spread itself is tested for the properties that matter
//! (identity at zero radius, monotone falloff and left/right symmetry of a hard
//! edge) instead of a brittle per-pixel golden.

use ferrous_2d::render::{
    CompositeMode2d, Effect2d, Layer2d, LayerError, ShapeInstance, MAX_EFFECTS_PER_LAYER,
};
use ferrous_renderer::headless::{Pure2dHarness, DEFAULT_FORMAT};
use glam::Vec2;

const SIZE: u32 = 16;

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

fn harness() -> Pure2dHarness {
    Pure2dHarness::new(SIZE, SIZE, DEFAULT_FORMAT).expect("no usable adapter")
}

fn within(pixel: u8, expected: u8) -> bool {
    i16::from(pixel).abs_diff(i16::from(expected)) <= 2
}

fn rect(center: Vec2, size: Vec2, colour: [f32; 4]) -> ShapeInstance {
    ShapeInstance::rect(center, size, colour)
}

#[test]
fn zero_radius_blur_is_the_identity() {
    let mut harness = harness();
    harness.set_clear_color(wgpu::Color::TRANSPARENT);
    harness
        .push_layer_with_effects(
            Layer2d::new(1.0, CompositeMode2d::Normal, 0.0),
            &[Effect2d::Blur { radius: 0.0 }],
        )
        .expect("push");
    harness.draw_shape(rect(Vec2::ZERO, Vec2::new(8.0, 8.0), [1.0; 4]));
    harness.pop_layer().expect("pop");
    harness.render_frame_blocking();

    let inside = harness.read_pixel(SIZE / 2, SIZE / 2);
    let outside = harness.read_pixel(SIZE / 2 + 6, SIZE / 2);
    assert_eq!(inside[3], 255, "inside alpha: {inside:?}");
    assert_eq!(outside[3], 0, "outside alpha: {outside:?}");
}

#[test]
fn blur_spreads_a_hard_edge_and_stays_symmetric() {
    let mut harness = harness();
    harness.set_clear_color(wgpu::Color::TRANSPARENT);
    harness
        .push_layer_with_effects(
            Layer2d::new(1.0, CompositeMode2d::Normal, 0.0),
            &[Effect2d::Blur { radius: 4.0 }],
        )
        .expect("push");
    // Left half-plane: x from -8 to 0.
    harness.draw_shape(rect(Vec2::new(-4.0, 0.0), Vec2::new(8.0, 16.0), [1.0; 4]));
    harness.pop_layer().expect("pop");
    harness.render_frame_blocking();

    // The edge sits between the centres of pixels 7 and 8, so those two are
    // the symmetric pair to compare.
    let left_of_edge = harness.read_pixel(SIZE / 2 - 1, SIZE / 2)[3];
    let right_of_edge = harness.read_pixel(SIZE / 2, SIZE / 2)[3];
    let deep_inside = harness.read_pixel(2, SIZE / 2)[3];
    let far_outside = harness.read_pixel(SIZE - 1, SIZE / 2)[3];

    assert_eq!(deep_inside, 255, "blur must not fade the interior");
    assert_eq!(
        far_outside, 0,
        "a 4 px blur must not reach the far side of the target"
    );
    assert!(
        (1..255).contains(&left_of_edge),
        "the edge must be soft inside: {left_of_edge}"
    );
    assert!(
        (1..255).contains(&right_of_edge),
        "the edge must be soft outside: {right_of_edge}"
    );
    // Pixels 7 and 8 straddle the edge, so their coverages complement:
    // alpha(inside) + alpha(outside) must stay at the edge's full coverage.
    let sum = u16::from(left_of_edge) + u16::from(right_of_edge);
    assert!(
        sum.abs_diff(255) <= 2,
        "the blur must be symmetric: {left_of_edge} + {right_of_edge} = {sum}"
    );
}

#[test]
fn drop_shadow_is_tinted_offset_and_faded() {
    let mut harness = harness();
    harness.set_clear_color(wgpu::Color::TRANSPARENT);
    harness
        .push_layer_with_effects(
            Layer2d::new(1.0, CompositeMode2d::Normal, 0.0),
            &[Effect2d::DropShadow {
                color: [1.0, 0.0, 0.0, 1.0],
                offset: [4.0, 0.0],
                blur: 0.0,
                opacity: 0.5,
            }],
        )
        .expect("push");
    harness.draw_shape(rect(Vec2::ZERO, Vec2::new(4.0, 4.0), [1.0; 4]));
    harness.pop_layer().expect("pop");
    harness.render_frame_blocking();

    // 4 px right of the shape's right edge: only the shadow.
    let shadow = harness.read_pixel(SIZE / 2 + 2, SIZE / 2);
    assert!(
        within(shadow[0], encode(0.5)),
        "shadow red should be colour * opacity: {shadow:?}"
    );
    assert!(
        shadow[1] < 4 && shadow[2] < 4,
        "shadow must be red: {shadow:?}"
    );
    assert!(
        within(shadow[3], 128),
        "shadow alpha should be opacity * 255: {shadow:?}"
    );

    // The shape is opaque and covers its own footprint.
    let content = harness.read_pixel(SIZE / 2, SIZE / 2);
    assert_eq!(content, [255, 255, 255, 255], "content: {content:?}");

    // One pixel left of the shape: no shadow there (the offset is positive).
    let left = harness.read_pixel(SIZE / 2 - 3, SIZE / 2);
    assert_eq!(
        left[3], 0,
        "shadow must not extend against the offset: {left:?}"
    );
}

#[test]
fn drop_shadow_stays_behind_the_content() {
    let mut harness = harness();
    harness.set_clear_color(wgpu::Color::TRANSPARENT);
    harness
        .push_layer_with_effects(
            Layer2d::new(1.0, CompositeMode2d::Normal, 0.0),
            &[Effect2d::DropShadow {
                color: [0.0, 0.0, 0.0, 1.0],
                offset: [2.0, 0.0],
                blur: 0.0,
                opacity: 1.0,
            }],
        )
        .expect("push");
    harness.draw_shape(rect(Vec2::ZERO, Vec2::new(8.0, 8.0), [1.0; 4]));
    harness.pop_layer().expect("pop");
    harness.render_frame_blocking();

    // Covered by both the opaque content and its offset shadow: the content
    // must win, untouched.
    let overlap = harness.read_pixel(SIZE / 2 + 1, SIZE / 2);
    assert_eq!(
        overlap,
        [255, 255, 255, 255],
        "the shadow must not darken the content: {overlap:?}"
    );
    // Past the content's right edge: only the black shadow.
    let shadow = harness.read_pixel(SIZE / 2 + 5, SIZE / 2);
    assert_eq!(shadow, [0, 0, 0, 255], "shadow: {shadow:?}");
}

#[test]
fn glow_adds_a_tinted_light() {
    let mut harness = harness();
    harness.set_clear_color(wgpu::Color::TRANSPARENT);
    harness
        .push_layer_with_effects(
            Layer2d::new(1.0, CompositeMode2d::Normal, 0.0),
            &[Effect2d::Glow {
                color: [0.0, 1.0, 0.0, 1.0],
                radius: 0.0,
                intensity: 0.5,
            }],
        )
        .expect("push");
    harness.draw_shape(rect(Vec2::ZERO, Vec2::new(8.0, 8.0), [0.0, 0.0, 0.0, 1.0]));
    harness.pop_layer().expect("pop");
    harness.render_frame_blocking();

    let centre = harness.read_pixel(SIZE / 2, SIZE / 2);
    assert_eq!(centre[3], 255, "content coverage is preserved: {centre:?}");
    assert!(
        within(centre[1], encode(0.5)),
        "glow green should be intensity: {centre:?}"
    );
    assert!(
        centre[0] < 4 && centre[2] < 4,
        "glow must stay green: {centre:?}"
    );
}

#[test]
fn effects_compose_with_group_opacity() {
    let backdrop = 0.25_f32;
    let mut harness = harness();
    harness.set_clear_color(wgpu::Color::TRANSPARENT);
    harness.draw_shape(rect(
        Vec2::ZERO,
        Vec2::new(16.0, 16.0),
        [backdrop, backdrop, backdrop, 1.0],
    ));
    harness
        .push_layer_with_effects(
            Layer2d {
                opacity: 0.5,
                blend: CompositeMode2d::Normal,
                depth: 0.0,
            },
            &[Effect2d::DropShadow {
                color: [1.0, 0.0, 0.0, 1.0],
                offset: [4.0, 0.0],
                blur: 0.0,
                opacity: 1.0,
            }],
        )
        .expect("push");
    harness.draw_shape(rect(Vec2::ZERO, Vec2::new(4.0, 4.0), [0.0, 0.0, 0.0, 1.0]));
    harness.pop_layer().expect("pop");
    harness.render_frame_blocking();

    // Shadow coverage 1, faded by the group opacity: the layer contributes
    // (1, 0, 0, 1) * 0.5 over the 0.25 backdrop, so
    // red = 0.5 + 0.25 * 0.5 = 0.625 and green = 0.25 * 0.5 = 0.125.
    let pixel = harness.read_pixel(SIZE / 2 + 2, SIZE / 2);
    assert!(
        within(pixel[0], encode(0.625)),
        "shadow must obey the layer opacity: {pixel:?}"
    );
    assert!(
        within(pixel[1], encode(0.125)),
        "the backdrop must show through the faded shadow: {pixel:?}"
    );
}

#[test]
fn invalid_parameters_and_overflow_are_typed() {
    let mut harness = harness();
    let err = harness
        .push_layer_with_effects(Layer2d::default(), &[Effect2d::Blur { radius: f32::NAN }])
        .expect_err("NaN radius");
    assert_eq!(
        err,
        LayerError::InvalidEffect {
            index: 0,
            reason: "blur radius must be finite and non-negative"
        }
    );

    let effects = [Effect2d::Blur { radius: 1.0 }; MAX_EFFECTS_PER_LAYER + 1];
    let err = harness
        .push_layer_with_effects(Layer2d::default(), &effects)
        .expect_err("too many");
    assert_eq!(
        err,
        LayerError::TooManyEffects {
            max: MAX_EFFECTS_PER_LAYER
        }
    );
}
