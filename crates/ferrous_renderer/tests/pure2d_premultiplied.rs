//! Premultiplied-alpha regression tests for the Pure2D compositing contract.
//!
//! Each test skips cleanly (with a printed reason) when no GPU adapter is
//! available, mirroring the other headless tests.

use std::sync::Mutex;

use ferrous_2d::render::ShapeInstance;
use ferrous_renderer::headless::Pure2dHarness;
use glam::Vec2;
use wgpu::TextureFormat;

static GPU_LOCK: Mutex<()> = Mutex::new(());

fn harness(width: u32, height: u32, clear: wgpu::Color) -> Option<Pure2dHarness> {
    match Pure2dHarness::with_clear_color(width, height, TextureFormat::Rgba8Unorm, clear) {
        Ok(h) => Some(h),
        Err(err) => {
            println!("[skip] headless GPU unavailable: {err}");
            None
        }
    }
}

fn at(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * width + x) * 4) as usize;
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

/// Two 50 % translucent rects over a **transparent** clear: the blend must
/// produce the premultiplied result, which straight-alpha `ALPHA_BLENDING`
/// gets wrong (it would write red 64 instead of 128).
#[test]
fn translucent_rects_over_transparent_clear_are_premultiplied() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(
        64,
        64,
        wgpu::Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        },
    ) else {
        return;
    };

    h.begin_frame();
    // Blue 50 % first, then red 50 % on top.
    h.draw_shape(ShapeInstance::rect(
        Vec2::ZERO,
        Vec2::splat(56.0),
        [0.0, 0.0, 1.0, 0.5],
    ));
    h.draw_shape(ShapeInstance::rect(
        Vec2::ZERO,
        Vec2::splat(56.0),
        [1.0, 0.0, 0.0, 0.5],
    ));
    h.render_frame();
    let pixels = h.read_pixels();

    // Premultiplied: src_red (0.5, 0, 0, 0.5) + dst_blue (0, 0, 0.5, 0.5) * 0.5
    // = (0.5, 0, 0.25, 0.75) -> 128, 0, 64, 191.
    assert_near(
        at(&pixels, 64, 32, 32),
        [128, 0, 64, 191],
        2,
        "premultiplied red over translucent blue",
    );
}

/// The opaque scenes that existed before the premultiplied change must be
/// byte-for-byte familiar: an opaque red rect over an opaque black clear is
/// still pure red with full alpha.
#[test]
fn opaque_scene_is_unchanged() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(
        32,
        32,
        wgpu::Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        },
    ) else {
        return;
    };

    h.begin_frame();
    h.draw_shape(ShapeInstance::rect(
        Vec2::ZERO,
        Vec2::splat(24.0),
        [1.0, 0.0, 0.0, 1.0],
    ));
    h.render_frame();
    let pixels = h.read_pixels();
    assert_near(
        at(&pixels, 32, 16, 16),
        [255, 0, 0, 255],
        1,
        "opaque red rect",
    );
}

/// The opt-in premultiplied texture path must render the same centre texel as
/// the straight-alpha path for an already-premultiplied 50 % red sprite.
#[test]
fn premultiplied_sprite_texture_matches_straight_path() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(
        32,
        32,
        wgpu::Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        },
    ) else {
        return;
    };

    // 50 % red: straight storage is (255, 0, 0, 128); premultiplied storage is
    // sRGB-encoded linear (0.5, 0, 0, 0.5) = (188, 0, 0, 128).
    let straight = [255u8, 0, 0, 128];
    let straight_px: Vec<u8> = std::iter::repeat_n(straight, 4).flatten().collect();
    let premul = [188u8, 0, 0, 128];
    let premul_px: Vec<u8> = std::iter::repeat_n(premul, 4).flatten().collect();

    let straight_id = h.register_texture_rgba8(2, 2, &straight_px);
    let premul_id = h.register_texture_rgba8_premultiplied(2, 2, &premul_px);

    let model = glam::Mat4::from_scale(glam::Vec3::new(16.0, 16.0, 1.0));
    let instance = ferrous_2d::render::SpriteInstance {
        transform_c0: model.x_axis.into(),
        transform_c1: model.y_axis.into(),
        transform_c2: model.z_axis.into(),
        transform_c3: model.w_axis.into(),
        color: [1.0, 1.0, 1.0, 1.0],
        uv_rect: [0.0, 0.0, 1.0, 1.0],
        properties: [0.0, 0.0, 0.0, 0.0],
    };

    h.begin_frame();
    h.draw_sprite(straight_id, instance);
    h.render_frame();
    let straight_readback = h.read_pixels();

    h.begin_frame();
    h.draw_sprite(premul_id, instance);
    h.render_frame();
    let premultiplied_readback = h.read_pixels();

    assert_near(
        at(&straight_readback, 32, 16, 16),
        at(&premultiplied_readback, 32, 16, 16),
        2,
        "straight vs premultiplied sprite centre",
    );
}
