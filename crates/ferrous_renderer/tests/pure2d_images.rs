//! Image texture tests for the Pure2D path: mipmaps, premultiplied upload,
//! `uv_rect` cropping, sprites inside layers/masks/effects, budget and limits.
//!
//! Every test skips cleanly (with a printed reason) when no GPU adapter is
//! available, mirroring the other headless tests.

use std::sync::Mutex;

use ferrous_2d::render::{
    build_mip_chain, Effect2d, Layer2d, Mask2d, MaskMode2d, SpriteInstance, TextureError,
    TextureFilter2d, TextureOptions2d,
};
use ferrous_renderer::headless::{Pure2dHarness, DEFAULT_FORMAT};
use ferrous_renderer::TextureUpdateError;
use glam::{Mat4, Quat, Vec2, Vec3};

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

fn sprite(center: Vec2, size: Vec2, uv_rect: [f32; 4]) -> SpriteInstance {
    let model = Mat4::from_scale_rotation_translation(
        Vec3::new(size.x, size.y, 1.0),
        Quat::IDENTITY,
        Vec3::new(center.x, center.y, 0.0),
    );
    SpriteInstance {
        transform_c0: model.x_axis.into(),
        transform_c1: model.y_axis.into(),
        transform_c2: model.z_axis.into(),
        transform_c3: model.w_axis.into(),
        color: [1.0, 1.0, 1.0, 1.0],
        uv_rect,
        properties: [0.0, 0.0, 0.0, 0.0],
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

/// A 4x4 black/white checker downsampled to one pixel samples the last mip
/// level, which is linear 0.5.  On the sRGB target that is ~188; filtering the
/// sRGB bytes would have produced ~55 after the sRGB decode.
#[test]
fn a_checker_downsampled_is_linear_half() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(8, 8, wgpu::Color::BLACK) else {
        return;
    };
    let mut data = Vec::new();
    for y in 0..4 {
        for x in 0..4 {
            let value = if (x + y) % 2 == 0 { 255 } else { 0 };
            data.extend_from_slice(&[value, value, value, 255]);
        }
    }
    let texture = h
        .register_texture_2d(4, 4, &data, TextureOptions2d::photo())
        .unwrap();

    h.begin_frame();
    // Two output pixels from four source texels: LOD 1 (the 2x2 level).
    h.draw_sprite(
        texture,
        sprite(Vec2::ZERO, Vec2::splat(2.0), [0.0, 0.0, 1.0, 1.0]),
    );
    h.render_frame_blocking();
    let pixels = h.read_pixels();
    near(pixel(&pixels, 8, 4, 4), [188, 188, 188], 2, "checker mip");
}

/// A same-size frame rewrites the texels in place: the same id, bind group
/// and sampler show the new colour, and bad sizes are rejected with typed
/// errors instead of panicking or corrupting the upload.
#[test]
fn a_sprite_texture_updates_in_place() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(8, 8, wgpu::Color::BLACK) else {
        return;
    };
    let red = vec![255, 0, 0, 255].repeat(4);
    let texture = h.register_texture_rgba8(2, 2, &red);

    h.begin_frame();
    h.draw_sprite(
        texture,
        sprite(Vec2::ZERO, Vec2::splat(8.0), [0.0, 0.0, 1.0, 1.0]),
    );
    h.render_frame_blocking();
    near(
        pixel(&h.read_pixels(), 8, 4, 4),
        [255, 0, 0],
        2,
        "registered red",
    );

    let green = vec![0, 255, 0, 255].repeat(4);
    h.renderer_mut()
        .update_sprite_texture(texture, 2, 2, &green)
        .unwrap();
    h.begin_frame();
    h.draw_sprite(
        texture,
        sprite(Vec2::ZERO, Vec2::splat(8.0), [0.0, 0.0, 1.0, 1.0]),
    );
    h.render_frame_blocking();
    near(
        pixel(&h.read_pixels(), 8, 4, 4),
        [0, 255, 0],
        2,
        "in-place green",
    );

    assert!(matches!(
        h.renderer_mut()
            .update_sprite_texture(texture, 2, 2, &[0; 4]),
        Err(TextureUpdateError::SizeMismatch { .. })
    ));
    let bigger = vec![0; 4 * 4 * 4];
    assert!(matches!(
        h.renderer_mut().update_sprite_texture(texture, 4, 4, &bigger),
        Err(TextureUpdateError::DimensionsMismatch { .. })
    ));
}

/// The CPU chain is exact for the 2x2 -> 1x1 case and for odd sizes.
#[test]
fn the_cpu_chain_is_linear_and_covers_odd_sizes() {
    let checker = [
        0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 255,
    ];
    let chain = build_mip_chain(2, 2, &checker).unwrap();
    let last = chain.level(1).unwrap();
    assert!((i16::from(last.rgba8_srgb_premultiplied[0]) - 188).abs() <= 1);

    let odd = build_mip_chain(3, 3, &[128; 36]).unwrap();
    assert_eq!(odd.len(), 2);
    assert_eq!(odd.level(1).unwrap().width, 1);
}

/// A half-transparent white texel is premultiplied once: over black it is
/// linear 0.5 (~188), not 0.25 (~137).
#[test]
fn a_half_transparent_texel_is_premultiplied_once() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(8, 8, wgpu::Color::BLACK) else {
        return;
    };
    let texture = h
        .register_texture_2d(
            1,
            1,
            &[255, 255, 255, 128],
            TextureOptions2d {
                mipmaps: false,
                filter: TextureFilter2d::Nearest,
            },
        )
        .unwrap();

    h.begin_frame();
    h.draw_sprite(
        texture,
        sprite(Vec2::ZERO, Vec2::splat(8.0), [0.0, 0.0, 1.0, 1.0]),
    );
    h.render_frame_blocking();
    let pixels = h.read_pixels();
    near(
        pixel(&pixels, 8, 4, 4),
        [188, 188, 188],
        2,
        "premultiplied texel",
    );
}

/// `uv_rect` crops exact texels: a 2x1 red/blue texture sampled at the right
/// half is blue and at the left half is red.
#[test]
fn uv_rect_crops_exact_texels() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(8, 8, wgpu::Color::BLACK) else {
        return;
    };
    let texture = h
        .register_texture_2d(
            2,
            1,
            &[255, 0, 0, 255, 0, 0, 255, 255],
            TextureOptions2d::pixel_art(),
        )
        .unwrap();

    h.begin_frame();
    h.draw_sprite(
        texture,
        sprite(Vec2::ZERO, Vec2::new(8.0, 8.0), [0.0, 0.0, 0.5, 1.0]),
    );
    h.render_frame_blocking();
    let left = h.read_pixels();
    near(pixel(&left, 8, 4, 4), [255, 0, 0], 2, "left crop");

    h.begin_frame();
    h.draw_sprite(
        texture,
        sprite(Vec2::ZERO, Vec2::new(8.0, 8.0), [0.5, 0.0, 0.5, 1.0]),
    );
    h.render_frame_blocking();
    let right = h.read_pixels();
    near(pixel(&right, 8, 4, 4), [0, 0, 255], 2, "right crop");
}

/// A sprite inside an offscreen layer obeys the layer's group opacity.
#[test]
fn a_sprite_inside_a_layer_gets_the_group_opacity() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(8, 8, wgpu::Color::BLACK) else {
        return;
    };
    let texture = h
        .register_texture_2d(1, 1, &[255, 255, 255, 255], TextureOptions2d::pixel_art())
        .unwrap();

    h.begin_frame();
    h.push_layer(Layer2d::new(
        0.5,
        ferrous_2d::render::CompositeMode2d::Normal,
        0.0,
    ))
    .unwrap();
    h.draw_sprite(
        texture,
        sprite(Vec2::ZERO, Vec2::splat(8.0), [0.0, 0.0, 1.0, 1.0]),
    );
    h.pop_layer().unwrap();
    h.render_frame_blocking();
    let pixels = h.read_pixels();
    near(pixel(&pixels, 8, 4, 4), [188, 188, 188], 2, "layer opacity");
}

/// A sprite inside a masked layer is clipped by the mask source.
#[test]
fn a_sprite_inside_a_masked_layer_is_clipped() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(16, 16, wgpu::Color::BLACK) else {
        return;
    };
    let texture = h
        .register_texture_2d(1, 1, &[255, 255, 255, 255], TextureOptions2d::pixel_art())
        .unwrap();

    h.begin_frame();
    h.push_masked_layer(
        Layer2d::new(1.0, ferrous_2d::render::CompositeMode2d::Normal, 0.0),
        &[],
        Mask2d {
            mode: MaskMode2d::Alpha,
            invert: false,
        },
    )
    .unwrap();
    h.draw_sprite(
        texture,
        sprite(Vec2::ZERO, Vec2::splat(16.0), [0.0, 0.0, 1.0, 1.0]),
    );
    h.begin_mask().unwrap();
    // Left half only.
    h.draw_shape(ferrous_2d::render::ShapeInstance::rect(
        Vec2::new(-4.0, 0.0),
        Vec2::new(8.0, 16.0),
        [1.0, 1.0, 1.0, 1.0],
    ));
    h.end_mask().unwrap();
    h.pop_layer().unwrap();
    h.render_frame_blocking();
    let pixels = h.read_pixels();
    near(
        pixel(&pixels, 16, 4, 8),
        [255, 255, 255],
        2,
        "inside the mask",
    );
    near(pixel(&pixels, 16, 12, 8), [0, 0, 0], 2, "outside the mask");
}

/// A drop shadow follows the texture's alpha: the transparent half of the
/// sprite casts no shadow, so the shadow only appears where the opaque half
/// moves under the offset.
#[test]
fn a_drop_shadow_follows_the_texture_alpha() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(32, 32, wgpu::Color::BLACK) else {
        return;
    };
    // 2x2: left column opaque white, right column transparent.
    let data = [
        255, 255, 255, 255, 0, 0, 0, 0, 255, 255, 255, 255, 0, 0, 0, 0,
    ];
    let texture = h
        .register_texture_2d(2, 2, &data, TextureOptions2d::pixel_art())
        .unwrap();

    h.begin_frame();
    h.push_layer_with_effects(
        Layer2d::new(1.0, ferrous_2d::render::CompositeMode2d::Normal, 0.0),
        &[Effect2d::DropShadow {
            color: [1.0, 0.0, 0.0, 1.0],
            offset: [10.0, 0.0],
            blur: 0.0,
            opacity: 1.0,
        }],
    )
    .unwrap();
    h.draw_sprite(
        texture,
        sprite(Vec2::ZERO, Vec2::splat(20.0), [0.0, 0.0, 1.0, 1.0]),
    );
    h.pop_layer().unwrap();
    h.render_frame_blocking();
    let pixels = h.read_pixels();
    // The opaque left half is white at the centre-left.
    near(
        pixel(&pixels, 32, 10, 16),
        [255, 255, 255],
        2,
        "opaque half",
    );
    // The shadow of that half lands in the transparent right half.
    near(
        pixel(&pixels, 32, 21, 16),
        [255, 0, 0],
        8,
        "shadow of the alpha",
    );
    // Beyond the offset silhouette there is only the backdrop.
    near(pixel(&pixels, 32, 30, 16), [0, 0, 0], 2, "past the shadow");
}

/// The budget counts every mip level and the free path subtracts it.
#[test]
fn the_texture_budget_counts_mips_and_frees_them() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(8, 8, wgpu::Color::BLACK) else {
        return;
    };
    assert_eq!(h.texture_bytes_2d(), 0);
    let data = vec![255u8; 4 * 4 * 4];
    let texture = h
        .register_texture_2d(4, 4, &data, TextureOptions2d::photo())
        .unwrap();
    // 4x4 + 2x2 + 1x1 = 21 texels * 4 bytes.
    assert_eq!(h.texture_bytes_2d(), 21 * 4);
    h.renderer_mut().free_sprite_texture(texture);
    assert_eq!(h.texture_bytes_2d(), 0);

    // Without mipmaps only level 0 is resident (16 texels * 4 bytes).
    let flat = h
        .register_texture_2d(4, 4, &data, TextureOptions2d::pixel_art())
        .unwrap();
    assert_eq!(h.texture_bytes_2d(), 16 * 4);
    h.renderer_mut().free_sprite_texture(flat);
    assert_eq!(h.texture_bytes_2d(), 0);

    // The legacy registration is counted too.
    let legacy = h.register_texture_rgba8(2, 2, &[255; 16]);
    assert_eq!(h.texture_bytes_2d(), 16);
    h.renderer_mut().free_sprite_texture(legacy);
    assert_eq!(h.texture_bytes_2d(), 0);
}

/// `mipmaps: false` builds level 0 only, so resident bytes are exactly
/// `width * height * 4` (no discarded chain).
#[test]
fn no_mipmaps_resident_bytes_are_level_zero_only() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(8, 8, wgpu::Color::BLACK) else {
        return;
    };
    // An odd size would otherwise produce 7x5 + 3x2 + 1x1 levels.
    let data = vec![128u8; 7 * 5 * 4];
    let texture = h
        .register_texture_2d(7, 5, &data, TextureOptions2d::pixel_art())
        .unwrap();
    assert_eq!(h.texture_bytes_2d(), 7 * 5 * 4);
    h.renderer_mut().free_sprite_texture(texture);
    assert_eq!(h.texture_bytes_2d(), 0);
}

/// A texture above the device limit is a typed error, not a panic.
#[test]
fn an_oversized_texture_is_a_typed_error() {
    let _guard = GPU_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let Some(mut h) = harness(8, 8, wgpu::Color::BLACK) else {
        return;
    };
    let limit = h.device().limits().max_texture_dimension_2d;
    let error = h
        .register_texture_2d(limit + 1, 1, &[255; 4], TextureOptions2d::pixel_art())
        .unwrap_err();
    assert_eq!(
        error,
        TextureError::TooLarge {
            width: limit + 1,
            height: 1,
            limit
        }
    );
    // A wrong byte count is typed too.
    assert!(matches!(
        h.register_texture_2d(2, 2, &[255; 3], TextureOptions2d::pixel_art()),
        Err(TextureError::SizeMismatch { .. })
    ));
}
