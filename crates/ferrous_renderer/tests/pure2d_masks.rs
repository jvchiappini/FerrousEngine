//! Exact-value and behaviour tests for layer masks (alpha and luma).
//!
//! Every test draws known content and a known mask source into two offscreen
//! layers and reads pixels back.  The expected 8-bit codes are computed from the
//! documented formulas; each test also samples a pixel covered by **both** the
//! content and the mask contribution, because that is the pixel where a wrong
//! composite formula shows up.

use ferrous_2d::render::{
    CompositeMode2d, Effect2d, Layer2d, LayerError, Mask2d, MaskMode2d, ShapeInstance,
};
use ferrous_renderer::headless::{Pure2dHarness, DEFAULT_FORMAT};
use glam::Vec2;

mod common;

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

/// Draws `colour` over the whole target.
fn full(colour: [f32; 4]) -> ShapeInstance {
    rect(Vec2::ZERO, Vec2::new(SIZE as f32, SIZE as f32), colour)
}

/// Opens a masked layer, draws `content`, then draws `mask`, and closes both.
fn masked(
    harness: &mut Pure2dHarness,
    desc: Layer2d,
    content: ShapeInstance,
    mask: &Mask2d,
    mask_shape: ShapeInstance,
) {
    harness
        .push_masked_layer(desc, &[], *mask)
        .expect("masked layer push");
    harness.draw_shape(content);
    harness.begin_mask().expect("begin mask");
    harness.draw_shape(mask_shape);
    harness.end_mask().expect("end mask");
    harness.pop_layer().expect("pop");
}

#[test]
fn alpha_mask_scales_coverage_exactly() {
    let _gpu = common::lock();
    let mut harness = harness();
    harness.set_clear_color(wgpu::Color::TRANSPARENT);
    masked(
        &mut harness,
        Layer2d::default(),
        rect(Vec2::ZERO, Vec2::new(8.0, 8.0), [1.0; 4]),
        &Mask2d::new(MaskMode2d::Alpha),
        // Alpha 0.5 masks also inside the content: the overlap pixel.
        full([1.0, 1.0, 1.0, 0.5]),
    );
    harness.render_frame_blocking();

    let overlap = harness.read_pixel(SIZE / 2, SIZE / 2);
    assert!(
        within(overlap[0], encode(0.5)),
        "content * alpha 0.5 must be half white: {overlap:?}"
    );
    assert!(overlap[1] == overlap[0] && overlap[2] == overlap[0]);
    assert!(
        within(overlap[3], 128),
        "masked alpha must be 0.5 * 255: {overlap:?}"
    );

    // Outside the content the mask covers, but there is nothing to show.
    let outside = harness.read_pixel(1, 1);
    assert_eq!(outside[3], 0, "outside the content: {outside:?}");
}

#[test]
fn luma_mask_uses_linear_luminance() {
    let _gpu = common::lock();
    let mut harness = harness();
    harness.set_clear_color(wgpu::Color::TRANSPARENT);
    masked(
        &mut harness,
        Layer2d::default(),
        full([1.0; 4]),
        &Mask2d::new(MaskMode2d::Luma),
        // Opaque mid grey: Rec. 709 luminance is exactly 0.5.
        full([0.5, 0.5, 0.5, 1.0]),
    );
    harness.render_frame_blocking();

    let overlap = harness.read_pixel(SIZE / 2, SIZE / 2);
    assert!(
        within(overlap[0], encode(0.5)),
        "luma 0.5 must halve the content: {overlap:?}"
    );
    assert!(
        within(overlap[3], 128),
        "luma 0.5 coverage must be 0.5 * 255: {overlap:?}"
    );
}

#[test]
fn inverted_alpha_and_luma_flip_the_coverage() {
    let _gpu = common::lock();
    {
        let mut harness = harness();
        harness.set_clear_color(wgpu::Color::TRANSPARENT);
        masked(
            &mut harness,
            Layer2d::default(),
            full([1.0; 4]),
            &Mask2d::new(MaskMode2d::Alpha).inverted(),
            full([1.0, 1.0, 1.0, 0.25]),
        );
        harness.render_frame_blocking();

        let overlap = harness.read_pixel(SIZE / 2, SIZE / 2);
        assert!(
            within(overlap[0], encode(0.75)),
            "inverted alpha 0.25 must leave 0.75 of the content: {overlap:?}"
        );
        assert!(within(overlap[3], 191), "alpha must be 0.75: {overlap:?}");
    }

    // Inverted luma: the left half is grey (coverage 0.5), the right half is
    // transparent (luma 0, so fully visible once inverted).
    let mut harness = harness();
    harness.set_clear_color(wgpu::Color::TRANSPARENT);
    masked(
        &mut harness,
        Layer2d::default(),
        full([1.0; 4]),
        &Mask2d::new(MaskMode2d::Luma).inverted(),
        rect(
            Vec2::new(-4.0, 0.0),
            Vec2::new(8.0, 16.0),
            [0.5, 0.5, 0.5, 1.0],
        ),
    );
    harness.render_frame_blocking();

    let masked_half = harness.read_pixel(SIZE / 2 - 2, SIZE / 2);
    assert!(
        within(masked_half[0], encode(0.5)),
        "inverted luma over grey must halve: {masked_half:?}"
    );
    let transparent_half = harness.read_pixel(SIZE / 2 + 2, SIZE / 2);
    assert_eq!(
        transparent_half,
        [255, 255, 255, 255],
        "a transparent mask pixel must be fully visible when inverted: {transparent_half:?}"
    );
}

#[test]
fn mask_composes_with_opacity_and_a_non_normal_blend() {
    let _gpu = common::lock();
    let backdrop = 0.5_f32;
    let mut harness = harness();
    harness.set_clear_color(wgpu::Color::TRANSPARENT);
    harness.draw_shape(full([backdrop, backdrop, backdrop, 1.0]));
    masked(
        &mut harness,
        Layer2d::new(0.25, CompositeMode2d::Multiply, 0.0),
        full([0.0, 0.0, 0.0, 1.0]),
        &Mask2d::new(MaskMode2d::Alpha),
        full([1.0, 1.0, 1.0, 0.5]),
    );
    harness.render_frame_blocking();

    // Content is black, mask leaves alpha 0.5, group opacity 0.25 => As =
    // 0.125.  Multiply over an opaque 0.5 backdrop is Cd * (1 - As) =
    // 0.5 * 0.875 = 0.4375.
    let overlap = harness.read_pixel(SIZE / 2, SIZE / 2);
    assert!(
        within(overlap[0], encode(0.4375)),
        "mask * opacity * multiply must be 0.4375: {overlap:?}"
    );
    assert!(overlap[1] == overlap[0] && overlap[2] == overlap[0]);
    assert_eq!(overlap[3], 255, "the opaque backdrop keeps its alpha");
}

#[test]
fn mask_clips_the_effect_chain() {
    let _gpu = common::lock();
    let mut harness = harness();
    harness.set_clear_color(wgpu::Color::TRANSPARENT);
    harness
        .push_masked_layer(
            Layer2d::new(1.0, CompositeMode2d::Normal, 0.0),
            &[Effect2d::DropShadow {
                color: [1.0, 0.0, 0.0, 1.0],
                offset: [4.0, 0.0],
                blur: 0.0,
                opacity: 1.0,
            }],
            Mask2d::new(MaskMode2d::Alpha),
        )
        .expect("push");
    harness.draw_shape(rect(Vec2::ZERO, Vec2::new(4.0, 4.0), [0.0, 0.0, 0.0, 1.0]));
    harness.begin_mask().expect("begin");
    // Opaque mask on the left half only.
    harness.draw_shape(rect(Vec2::new(-4.0, 0.0), Vec2::new(8.0, 16.0), [1.0; 4]));
    harness.end_mask().expect("end");
    harness.pop_layer().expect("pop");
    harness.render_frame_blocking();

    // The shadow sits 4 px to the right of the content, in the masked-out
    // half: clipping runs after the effect chain, so it leaves nothing.
    let clipped_shadow = harness.read_pixel(SIZE / 2 + 4, SIZE / 2);
    assert_eq!(
        clipped_shadow[3], 0,
        "the mask must clip the shadow: {clipped_shadow:?}"
    );
    // The pixel covered by both the content and the mask keeps the content.
    let overlap = harness.read_pixel(SIZE / 2 - 1, SIZE / 2);
    assert_eq!(
        overlap,
        [0, 0, 0, 255],
        "content inside the mask must survive: {overlap:?}"
    );
    // The content's right half is also masked out.
    let clipped_content = harness.read_pixel(SIZE / 2 + 1, SIZE / 2);
    assert_eq!(
        clipped_content[3], 0,
        "the mask must clip the content's covered half: {clipped_content:?}"
    );
}

#[test]
fn a_blurred_mask_feathers_the_edge() {
    let _gpu = common::lock();
    let mut harness = harness();
    harness.set_clear_color(wgpu::Color::TRANSPARENT);
    harness
        .push_masked_layer(Layer2d::default(), &[], Mask2d::new(MaskMode2d::Alpha))
        .expect("push");
    harness.draw_shape(full([1.0; 4]));
    harness.begin_mask().expect("begin");
    // The source carries the blur: a child layer with the effect chain, the
    // general way to feather a mask.
    harness
        .push_layer_with_effects(Layer2d::default(), &[Effect2d::Blur { radius: 2.0 }])
        .expect("mask child layer");
    harness.draw_shape(rect(Vec2::new(-4.0, 0.0), Vec2::new(8.0, 16.0), [1.0; 4]));
    harness.pop_layer().expect("pop mask child");
    harness.end_mask().expect("end");
    harness.pop_layer().expect("pop");
    harness.render_frame_blocking();

    let ramp: Vec<u8> = (5..11)
        .map(|x| harness.read_pixel(x, SIZE / 2)[3])
        .collect();
    assert_eq!(
        harness.read_pixel(2, SIZE / 2)[3],
        255,
        "deep inside the mask must be opaque"
    );
    assert_eq!(
        harness.read_pixel(13, SIZE / 2)[3],
        0,
        "far outside the mask must be clear"
    );
    assert!(
        ramp.windows(2).all(|pair| pair[0] >= pair[1]),
        "the feathered edge must fall monotonically: {ramp:?}"
    );
    assert!(
        (1..255).contains(&ramp[3]),
        "the edge pixel must be partially covered: {ramp:?}"
    );
}

#[test]
fn nested_masked_layers_multiply_their_coverage() {
    let _gpu = common::lock();
    let mut harness = harness();
    harness.set_clear_color(wgpu::Color::TRANSPARENT);
    harness
        .push_masked_layer(Layer2d::default(), &[], Mask2d::new(MaskMode2d::Alpha))
        .expect("outer push");
    harness
        .push_masked_layer(Layer2d::default(), &[], Mask2d::new(MaskMode2d::Alpha))
        .expect("inner push");
    harness.draw_shape(full([1.0; 4]));
    harness.begin_mask().expect("inner begin");
    harness.draw_shape(full([1.0, 1.0, 1.0, 0.5]));
    harness.end_mask().expect("inner end");
    harness.pop_layer().expect("inner pop");
    harness.begin_mask().expect("outer begin");
    harness.draw_shape(full([1.0, 1.0, 1.0, 0.5]));
    harness.end_mask().expect("outer end");
    harness.pop_layer().expect("outer pop");
    harness.render_frame_blocking();

    // 0.5 from the inner mask and 0.5 from the outer one: 0.25.
    let overlap = harness.read_pixel(SIZE / 2, SIZE / 2);
    assert!(
        within(overlap[0], encode(0.25)),
        "nested masks must multiply: {overlap:?}"
    );
    assert!(within(overlap[3], 64), "alpha must be 0.25: {overlap:?}");
}

#[test]
fn mask_misuse_is_typed() {
    let _gpu = common::lock();
    let mut harness = harness();
    assert_eq!(
        harness.begin_mask().expect_err("no masked layer"),
        LayerError::MaskSourceNotAllowed
    );

    harness
        .push_masked_layer(Layer2d::default(), &[], Mask2d::new(MaskMode2d::Alpha))
        .expect("push");
    assert_eq!(
        harness.end_mask().expect_err("no source open"),
        LayerError::MaskSourceMismatch
    );
    assert_eq!(
        harness.pop_layer().expect_err("no source drawn"),
        LayerError::MaskWithoutSource
    );

    harness
        .push_masked_layer(Layer2d::default(), &[], Mask2d::new(MaskMode2d::Alpha))
        .expect("push");
    harness.begin_mask().expect("begin");
    assert_eq!(
        harness.begin_mask().expect_err("already open"),
        LayerError::MaskSourceNotAllowed
    );
    assert_eq!(
        harness.pop_layer().expect_err("source still open"),
        LayerError::MaskSourceNotClosed
    );
    harness.end_mask().expect("end");
    harness.pop_layer().expect("pop");
}
