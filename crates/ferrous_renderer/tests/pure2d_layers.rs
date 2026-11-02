//! Offscreen compositing layer tests for the Pure2D path.
//!
//! GPU tests skip cleanly (with a printed reason) when no adapter is available.

use std::sync::Mutex;

use ferrous_2d::render::{CompositeMode2d, Layer2d, LayerError, ShapeInstance, MAX_LAYER_DEPTH_2D};
use ferrous_font::{charset, TextAlign};
use ferrous_renderer::headless::Pure2dHarness;
use glam::{Mat4, Vec2};
use wgpu::TextureFormat;

static GPU_LOCK: Mutex<()> = Mutex::new(());

const SIZE: u32 = 64;

fn harness(clear: wgpu::Color) -> Option<Pure2dHarness> {
    match Pure2dHarness::with_clear_color(SIZE, SIZE, TextureFormat::Rgba8Unorm, clear) {
        Ok(h) => Some(h),
        Err(err) => {
            println!("[skip] headless GPU unavailable: {err}");
            None
        }
    }
}

fn black() -> wgpu::Color {
    wgpu::Color {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    }
}

fn white() -> wgpu::Color {
    wgpu::Color {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    }
}

fn at(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let i = ((y * SIZE + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

fn assert_near(actual: [u8; 4], expected: [u8; 4], tol: u8, what: &str) {
    for channel in 0..4 {
        let diff = actual[channel].abs_diff(expected[channel]);
        assert!(
            diff <= tol,
            "{what}: channel {channel} was {} (expected {} +/- {tol}); full {actual:?} vs {expected:?}",
            actual[channel],
            expected[channel]
        );
    }
}

fn rect(center: Vec2, size: f32, color: [f32; 4], z: f32) -> ShapeInstance {
    ShapeInstance::rect_z(center, z, Vec2::splat(size), color)
}

/// Group opacity is applied once to the finished layer, so overlapping opaque
/// children do not bleed into each other.
#[test]
fn group_opacity_has_no_internal_overlap() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(white()) else {
        return;
    };

    h.begin_frame();
    h.push_layer(Layer2d {
        opacity: 0.5,
        blend: CompositeMode2d::Normal,
        depth: 0.0,
    })
    .expect("push layer");
    h.draw_shape(rect(Vec2::new(-6.0, 0.0), 24.0, [1.0, 0.0, 0.0, 1.0], 0.0));
    h.draw_shape(rect(Vec2::new(0.0, 0.0), 24.0, [0.0, 0.0, 1.0, 1.0], 1.0));
    h.pop_layer().expect("pop layer");
    h.render_frame();
    let pixels = h.read_pixels();

    // Overlap: the layer is fully opaque blue there, so 50 % over white is
    // (0.5, 0.5, 1). Per-child opacity would leak red into the red channel.
    assert_near(
        at(&pixels, 32, 32),
        [128, 128, 255, 255],
        2,
        "overlap is blue at 50 % over white",
    );
    // Red-only area: (1, 0.5, 0.5).
    assert_near(
        at(&pixels, 32 - 14, 32),
        [255, 128, 128, 255],
        2,
        "red-only area at 50 % over white",
    );
}

/// A layer is one command in its parent's painter order at `depth`.
#[test]
fn layer_depth_orders_the_whole_layer() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(black()) else {
        return;
    };

    h.begin_frame();
    h.draw_shape(rect(Vec2::ZERO, 60.0, [1.0, 0.0, 0.0, 1.0], 1.0));
    h.push_layer(Layer2d {
        opacity: 1.0,
        blend: CompositeMode2d::Normal,
        depth: 2.0,
    })
    .expect("push");
    h.draw_shape(rect(Vec2::ZERO, 40.0, [0.0, 1.0, 0.0, 1.0], 0.0));
    h.pop_layer().expect("pop");
    h.draw_shape(rect(Vec2::ZERO, 20.0, [0.0, 0.0, 1.0, 1.0], 3.0));
    h.render_frame();
    let pixels = h.read_pixels();

    assert_near(at(&pixels, 32, 32), [0, 0, 255, 255], 2, "z = 3 wins");
    assert_near(
        at(&pixels, 32 + 14, 32),
        [0, 255, 0, 255],
        2,
        "layer sits above z = 1",
    );
    assert_near(
        at(&pixels, 32 + 24, 32),
        [255, 0, 0, 255],
        2,
        "root rect outside the layer",
    );
}

/// 0.5 nested inside 0.5 fades an opaque child to 0.25 over the backdrop.
#[test]
fn nested_layer_opacities_multiply() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(white()) else {
        return;
    };

    h.begin_frame();
    h.push_layer(Layer2d {
        opacity: 0.5,
        blend: CompositeMode2d::Normal,
        depth: 0.0,
    })
    .expect("outer");
    h.push_layer(Layer2d {
        opacity: 0.5,
        blend: CompositeMode2d::Normal,
        depth: 0.0,
    })
    .expect("inner");
    h.draw_shape(rect(Vec2::ZERO, 30.0, [1.0, 0.0, 0.0, 1.0], 0.0));
    h.pop_layer().expect("pop inner");
    h.pop_layer().expect("pop outer");
    let report = h.render_frame();
    assert_eq!(report.layer_passes, 3, "two layer passes + the target pass");
    println!(
        "[info] layer texture format on this adapter: {:?}",
        h.layer_texture_format()
    );
    let pixels = h.read_pixels();

    // 25 % red over white: (1, 0.75, 0.75).
    assert_near(
        at(&pixels, 32, 32),
        [255, 191, 191, 255],
        2,
        "nested group opacities",
    );
}

/// A Multiply layer over an opaque backdrop matches the phase-6 formula.
#[test]
fn layer_multiply_blend_matches_the_formula() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(black()) else {
        return;
    };

    h.begin_frame();
    h.draw_shape(rect(Vec2::ZERO, 60.0, [0.5, 1.0, 0.5, 1.0], 0.0));
    h.push_layer(Layer2d {
        opacity: 1.0,
        blend: CompositeMode2d::Multiply,
        depth: 1.0,
    })
    .expect("push");
    h.draw_shape(rect(Vec2::ZERO, 30.0, [1.0, 0.0, 0.0, 1.0], 0.0));
    h.pop_layer().expect("pop");
    h.render_frame();
    let pixels = h.read_pixels();

    // red (1,0,0) * backdrop (0.5,1,0.5) + backdrop * (1 - 1) = (0.5, 0, 0).
    assert_near(
        at(&pixels, 32, 32),
        [128, 0, 0, 255],
        2,
        "multiply layer over opaque backdrop",
    );
}

fn roboto_bytes() -> Option<Vec<u8>> {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let path = manifest.join("../../assets/fonts/Roboto-Regular.ttf");
    match std::fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(err) => {
            println!("[skip] Roboto-Regular.ttf unavailable at {path:?}: {err}");
            None
        }
    }
}

/// Text at layer opacity 1 renders the same inside a layer as outside it.
#[test]
fn text_inside_a_layer_matches_outside() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(bytes) = roboto_bytes() else { return };
    let Some(mut h) = harness(black()) else {
        return;
    };
    let font = match h.register_font(&bytes, charset::ascii()) {
        Ok(font) => font,
        Err(err) => {
            println!("[skip] font atlas unavailable: {err}");
            return;
        }
    };

    h.begin_frame();
    h.draw_text(font, "H", Mat4::IDENTITY, 56.0, TextAlign::Center, [1.0; 4]);
    h.render_frame();
    let outside = h.read_pixels();

    h.begin_frame();
    h.push_layer(Layer2d {
        opacity: 1.0,
        blend: CompositeMode2d::Normal,
        depth: 0.0,
    })
    .expect("push");
    h.draw_text(font, "H", Mat4::IDENTITY, 56.0, TextAlign::Center, [1.0; 4]);
    h.pop_layer().expect("pop");
    h.render_frame();
    let inside = h.read_pixels();

    let mut worst = 0u8;
    for (a, b) in outside.iter().zip(inside.iter()) {
        worst = worst.max(a.abs_diff(*b));
    }
    assert!(
        worst <= 2,
        "text inside a layer must match outside within 2 levels (worst {worst})"
    );
}

/// An empty layer draws nothing and allocates no texture.
#[test]
fn empty_layer_draws_nothing_and_costs_no_texture() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(black()) else {
        return;
    };

    h.begin_frame();
    h.push_layer(Layer2d::default()).expect("push");
    h.pop_layer().expect("pop");
    let report = h.render_frame();
    assert_eq!(report.layer_passes, 1, "empty layer adds no pass");
    assert_eq!(
        h.layer_textures_allocated(),
        0,
        "empty layer must not allocate a texture"
    );
    let pixels = h.read_pixels();
    assert_near(at(&pixels, 32, 32), [0, 0, 0, 255], 1, "empty frame");
}

/// Unbalanced push/pop reports once, closes the layer and still renders.
#[test]
fn unbalanced_layer_reports_and_still_renders() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(black()) else {
        return;
    };

    h.begin_frame();
    h.push_layer(Layer2d::default()).expect("push");
    h.draw_shape(rect(Vec2::ZERO, 30.0, [1.0, 0.0, 0.0, 1.0], 0.0));
    let report = h.render_frame();
    assert_eq!(
        report.layer_error,
        Some(LayerError::Unbalanced { open: 1 }),
        "an unclosed layer must be reported"
    );
    let pixels = h.read_pixels();
    assert_near(
        at(&pixels, 32, 32),
        [255, 0, 0, 255],
        2,
        "the layer content is still composited",
    );
    assert_eq!(h.layer_depth(), 0, "the layer was closed automatically");
    assert!(
        h.pop_layer().is_err(),
        "popping after an unbalanced frame reports underflow"
    );
}

/// The nesting limit is enforced with a typed error.
#[test]
fn layer_depth_limit_is_typed() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(black()) else {
        return;
    };

    h.begin_frame();
    for _ in 0..MAX_LAYER_DEPTH_2D {
        h.push_layer(Layer2d::default()).expect("push within limit");
    }
    let err = h.push_layer(Layer2d::default()).expect_err("too deep");
    assert_eq!(
        err,
        LayerError::TooDeep {
            max: MAX_LAYER_DEPTH_2D,
            requested: MAX_LAYER_DEPTH_2D + 1
        }
    );
    while h.layer_depth() > 0 {
        h.pop_layer().expect("pop");
    }
    let report = h.render_frame();
    assert_eq!(report.layer_passes, 1, "empty nested layers add no pass");

    assert!(h.pop_layer().is_err(), "underflow must be typed");
}

/// Textures are pooled: 100 frames with three layers create three textures in
/// frame 1 and none afterwards.
#[test]
fn layer_texture_pool_is_reused_across_frames() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(black()) else {
        return;
    };

    fn render_three(h: &mut Pure2dHarness) {
        h.begin_frame();
        for i in 0..3 {
            h.push_layer(Layer2d {
                opacity: 0.75,
                blend: CompositeMode2d::Normal,
                depth: i as f32,
            })
            .expect("push");
            h.draw_shape(rect(
                Vec2::new(i as f32 * 8.0, 0.0),
                12.0,
                [1.0, 1.0, 1.0, 1.0],
                0.0,
            ));
            h.pop_layer().expect("pop");
        }
        h.render_frame();
    }

    render_three(&mut h);
    let allocated = h.layer_textures_allocated();
    assert_eq!(allocated, 3, "frame 1 creates one texture per layer");
    for _ in 0..99 {
        render_three(&mut h);
    }
    assert_eq!(
        h.layer_textures_allocated(),
        allocated,
        "steady state must not create layer textures"
    );
}

/// Same layered frame twice gives identical bytes.
#[test]
fn layered_frame_is_deterministic() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(black()) else {
        return;
    };

    let mut render = || {
        h.begin_frame();
        h.draw_shape(rect(Vec2::ZERO, 50.0, [0.0, 0.25, 0.5, 1.0], 0.0));
        h.push_layer(Layer2d {
            opacity: 0.6,
            blend: CompositeMode2d::Screen,
            depth: 1.0,
        })
        .expect("push");
        h.draw_shape(rect(Vec2::new(4.0, 0.0), 30.0, [1.0, 0.0, 0.0, 0.5], 0.0));
        h.push_layer(Layer2d {
            opacity: 0.5,
            blend: CompositeMode2d::Add,
            depth: 2.0,
        })
        .expect("push inner");
        h.draw_shape(rect(Vec2::ZERO, 16.0, [0.0, 1.0, 0.0, 1.0], 0.0));
        h.pop_layer().expect("pop inner");
        h.pop_layer().expect("pop outer");
        h.render_frame();
        h.read_pixels()
    };

    assert_eq!(render(), render(), "layered frame must be deterministic");
}
