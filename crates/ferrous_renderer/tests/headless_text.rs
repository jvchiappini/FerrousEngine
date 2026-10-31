//! Headless Pure2D text tests: baseline, alignment, z-interleaving,
//! transforms, sharpness, determinism and linear colour.
//!
//! Every test skips cleanly (with a printed reason) when no GPU adapter is
//! available, so the suite is honest on CPU-only CI.

use std::sync::Mutex;

use ferrous_2d::render::{FontId2d, ShapeInstance};
use ferrous_font::{charset, TextAlign};
use ferrous_renderer::headless::{Pure2dHarness, DEFAULT_FORMAT};
use glam::{Mat4, Vec2, Vec3};
use wgpu::TextureFormat;

static GPU_LOCK: Mutex<()> = Mutex::new(());

const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];

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

fn harness(width: u32, height: u32, format: TextureFormat) -> Option<Pure2dHarness> {
    match Pure2dHarness::new(width, height, format) {
        Ok(h) => Some(h),
        Err(err) => {
            println!("[skip] headless GPU unavailable: {err}");
            None
        }
    }
}

fn register_font(h: &mut Pure2dHarness, bytes: &[u8]) -> Option<FontId2d> {
    match h.register_font(bytes, charset::ascii()) {
        Ok(font) => Some(font),
        Err(err) => {
            println!("[skip] font atlas unavailable: {err}");
            None
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct InkBox {
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
}

impl InkBox {
    fn width(self) -> f32 {
        self.max_x - self.min_x
    }
    fn height(self) -> f32 {
        self.max_y - self.min_y
    }
}

/// Bounding box of pixels whose brightest channel exceeds `threshold`, in
/// Pure2D world coordinates (1 unit = 1 pixel, origin at the target centre, Y
/// up).  Texel `(i, j)` samples world `(i + 0.5 - W/2, H/2 - j - 0.5)`.
fn ink_box(pixels: &[u8], width: u32, height: u32, threshold: u8) -> Option<InkBox> {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut count = 0u32;
    for y in 0..height {
        for x in 0..width {
            let i = ((y * width + x) * 4) as usize;
            let bright = pixels[i].max(pixels[i + 1]).max(pixels[i + 2]);
            if bright <= threshold {
                continue;
            }
            let wx = x as f32 + 0.5 - width as f32 * 0.5;
            let wy = height as f32 * 0.5 - (y as f32 + 0.5);
            min_x = min_x.min(wx);
            min_y = min_y.min(wy);
            max_x = max_x.max(wx);
            max_y = max_y.max(wy);
            count += 1;
        }
    }
    if count == 0 {
        None
    } else {
        Some(InkBox {
            min_x,
            min_y,
            max_x,
            max_y,
        })
    }
}

fn assert_close(actual: f32, expected: f32, tolerance: f32, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{what}: {actual} vs expected {expected} +/- {tolerance}"
    );
}

#[test]
fn text_renders_at_the_baseline() {
    let _guard = GPU_LOCK.lock().unwrap();
    let Some(bytes) = roboto_bytes() else { return };
    let Some(mut h) = harness(256, 256, TextureFormat::Rgba8Unorm) else {
        return;
    };
    let Some(font) = register_font(&mut h, &bytes) else {
        return;
    };
    let metrics = h
        .measure_text(font, "H", 100.0, TextAlign::Left)
        .expect("layout metrics");

    h.begin_frame();
    h.draw_text(font, "H", Mat4::IDENTITY, 100.0, TextAlign::Left, WHITE);
    let report = h.render_frame();
    assert_eq!(report.text_draw_calls, 1, "all glyphs of one call coalesce");
    assert!(report.instances_uploaded >= 1);

    let ink = ink_box(&h.read_pixels(), 256, 256, 10).expect("'H' must produce ink");
    assert!(
        ink.min_y >= -1.0,
        "no glyph ink may fall below the baseline by more than 1 px (min y = {})",
        ink.min_y
    );
    assert!(
        ink.max_y <= metrics.ascender + 1.0,
        "'H' must stay below the ascender (max y = {}, ascender = {})",
        ink.max_y,
        metrics.ascender
    );
    assert!(
        ink.max_y > metrics.ascender * 0.4,
        "'H' must actually be drawn (max y = {})",
        ink.max_y
    );
    assert!(
        ink.min_y < 2.0,
        "'H' must sit on the baseline (min y = {})",
        ink.min_y
    );
}

#[test]
fn text_alignment_shifts_the_ink_box() {
    let _guard = GPU_LOCK.lock().unwrap();
    let Some(bytes) = roboto_bytes() else { return };
    let Some(mut h) = harness(256, 256, TextureFormat::Rgba8Unorm) else {
        return;
    };
    let Some(font) = register_font(&mut h, &bytes) else {
        return;
    };
    let advance = h
        .measure_text(font, "HH", 64.0, TextAlign::Left)
        .expect("layout metrics")
        .advance;

    let mut boxes = Vec::new();
    for align in [TextAlign::Left, TextAlign::Center, TextAlign::Right] {
        h.begin_frame();
        h.draw_text(font, "HH", Mat4::IDENTITY, 64.0, align, WHITE);
        h.render_frame();
        boxes.push(ink_box(&h.read_pixels(), 256, 256, 10).expect("ink"));
    }

    let left = boxes[0];
    assert_close(
        boxes[1].min_x,
        left.min_x - advance * 0.5,
        1.0,
        "centre min x",
    );
    assert_close(
        boxes[1].max_x,
        left.max_x - advance * 0.5,
        1.0,
        "centre max x",
    );
    assert_close(boxes[2].min_x, left.min_x - advance, 1.0, "right min x");
    assert_close(boxes[2].max_x, left.max_x - advance, 1.0, "right max x");
}

#[test]
fn text_interleaves_by_z() {
    let _guard = GPU_LOCK.lock().unwrap();
    let Some(bytes) = roboto_bytes() else { return };
    let Some(mut h) = harness(256, 256, TextureFormat::Rgba8Unorm) else {
        return;
    };
    let Some(font) = register_font(&mut h, &bytes) else {
        return;
    };

    h.begin_frame();
    h.draw_shape(ShapeInstance::rect_z(
        Vec2::ZERO,
        1.0,
        Vec2::splat(200.0),
        RED,
    ));
    // Text depth is the transform's translation z, shared by every glyph.
    h.draw_text(
        font,
        "H",
        Mat4::from_translation(Vec3::new(0.0, 0.0, 2.0)),
        120.0,
        TextAlign::Left,
        WHITE,
    );
    h.draw_shape(ShapeInstance::rect_z(
        Vec2::ZERO,
        3.0,
        Vec2::splat(40.0),
        GREEN,
    ));
    let report = h.render_frame();
    let pixels = h.read_pixels();

    let at = |x: u32, y: u32| -> [u8; 4] {
        let i = ((y * 256 + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
    };

    // The z = 3 rect must cover both the z = 1 rect and the z = 2 text.
    assert_eq!(
        at(128, 128),
        [0, 255, 0, 255],
        "z = 3 rect wins at the centre"
    );

    // Somewhere outside the small green rect the white text must be visible,
    // proving it was painted above the red rect but below the green one.
    let mut found_white = false;
    for y in 0..256u32 {
        for x in 0..256u32 {
            let p = at(x, y);
            let far_from_centre = (x as i32 - 128).abs() > 24 || (y as i32 - 128).abs() > 24;
            if far_from_centre && p[0] > 235 && p[1] > 235 && p[2] > 235 {
                found_white = true;
                break;
            }
        }
        if found_white {
            break;
        }
    }
    assert!(found_white, "white text must be visible over the red rect");
    assert!(report.text_draw_calls >= 1);
}

#[test]
fn text_transform_rotates_and_scales_the_ink_box() {
    let _guard = GPU_LOCK.lock().unwrap();
    let Some(bytes) = roboto_bytes() else { return };
    let Some(mut h) = harness(256, 256, TextureFormat::Rgba8Unorm) else {
        return;
    };
    let Some(font) = register_font(&mut h, &bytes) else {
        return;
    };

    h.begin_frame();
    h.draw_text(font, "H", Mat4::IDENTITY, 50.0, TextAlign::Left, WHITE);
    h.render_frame();
    let base = ink_box(&h.read_pixels(), 256, 256, 10).expect("base ink");

    h.begin_frame();
    let rotate = Mat4::from_rotation_z(std::f32::consts::FRAC_PI_2);
    h.draw_text(font, "H", rotate, 50.0, TextAlign::Left, WHITE);
    h.render_frame();
    let rotated = ink_box(&h.read_pixels(), 256, 256, 10).expect("rotated ink");

    // 90° CCW: (x, y) -> (-y, x).  Rotating the AABB corners is exact for an
    // axis-aligned 90° rotation.
    assert_close(rotated.min_x, -base.max_y, 1.5, "rotated min x");
    assert_close(rotated.max_x, -base.min_y, 1.5, "rotated max x");
    assert_close(rotated.min_y, base.min_x, 1.5, "rotated min y");
    assert_close(rotated.max_y, base.max_x, 1.5, "rotated max y");

    h.begin_frame();
    let scale = Mat4::from_scale(Vec3::new(2.0, 2.0, 1.0));
    h.draw_text(font, "H", scale, 50.0, TextAlign::Left, WHITE);
    h.render_frame();
    let scaled = ink_box(&h.read_pixels(), 256, 256, 10).expect("scaled ink");

    assert_close(scaled.min_x, base.min_x * 2.0, 1.5, "scaled min x");
    assert_close(scaled.max_x, base.max_x * 2.0, 1.5, "scaled max x");
    assert_close(scaled.min_y, base.min_y * 2.0, 1.5, "scaled min y");
    assert_close(scaled.max_y, base.max_y * 2.0, 1.5, "scaled max y");
}

#[test]
fn text_is_sharp_when_scaled() {
    let _guard = GPU_LOCK.lock().unwrap();
    let Some(bytes) = roboto_bytes() else { return };
    let Some(mut h) = harness(512, 512, TextureFormat::Rgba8Unorm) else {
        return;
    };
    let Some(font) = register_font(&mut h, &bytes) else {
        return;
    };
    let metrics = h
        .measure_text(font, "H", 32.0, TextAlign::Left)
        .expect("metrics");

    h.begin_frame();
    let transform = Mat4::from_scale(Vec3::splat(8.0));
    h.draw_text(font, "H", transform, 32.0, TextAlign::Left, WHITE);
    h.render_frame();
    let pixels = h.read_pixels();
    let ink = ink_box(&pixels, 512, 512, 10).expect("ink");
    assert!(
        ink.width() > 80.0 && ink.height() > 80.0,
        "glyph must be large"
    );

    // A horizontal scanline above the H crossbar crosses both vertical stems
    // with no diagonal/hinted geometry.  Count intermediate (partially
    // covered) pixels on every dark -> light transition.
    let row_world = metrics.ascender * 0.6;
    let row = (256.0 - row_world) as u32;
    let at = |x: u32| -> u8 {
        let i = ((row * 512 + x) * 4) as usize;
        pixels[i].max(pixels[i + 1]).max(pixels[i + 2])
    };

    let mut worst_transition = 0u32;
    let mut transitions = 0u32;
    let mut in_ink = false;
    let mut soft = 0u32;
    for x in 0..512u32 {
        let v = at(x);
        if !in_ink {
            if v > 235 {
                in_ink = true;
                transitions += 1;
                worst_transition = worst_transition.max(soft);
                soft = 0;
            } else if v > 20 {
                soft += 1;
            } else {
                soft = 0;
            }
        } else if v < 20 {
            in_ink = false;
            soft = 0;
        }
    }

    assert!(
        transitions >= 1,
        "the scanline must cross at least one stem edge"
    );
    assert!(
        worst_transition <= 2,
        "MSDF edge transition must be at most 2 px wide, got {worst_transition}"
    );
}

#[test]
fn text_is_deterministic() {
    let _guard = GPU_LOCK.lock().unwrap();
    let Some(bytes) = roboto_bytes() else { return };
    let Some(mut h) = harness(192, 192, TextureFormat::Rgba8Unorm) else {
        return;
    };
    let Some(font) = register_font(&mut h, &bytes) else {
        return;
    };

    let mut render = || {
        h.begin_frame();
        h.draw_shape(ShapeInstance::rect_z(
            Vec2::ZERO,
            0.0,
            Vec2::splat(80.0),
            RED,
        ));
        h.draw_text(
            font,
            "Ferrous",
            Mat4::from_translation(Vec3::new(-70.0, 20.0, 1.0)),
            24.0,
            TextAlign::Left,
            WHITE,
        );
        h.render_frame();
        h.read_pixels()
    };

    let first = render();
    let second = render();
    assert_eq!(
        first, second,
        "the same frame twice must give identical bytes"
    );
}

#[test]
fn text_colour_is_linear() {
    let _guard = GPU_LOCK.lock().unwrap();
    let Some(bytes) = roboto_bytes() else { return };
    // sRGB target: the shader writes linear 0.5, wgpu encodes it on store.
    let Some(mut h) = harness(256, 256, DEFAULT_FORMAT) else {
        return;
    };
    let Some(font) = register_font(&mut h, &bytes) else {
        return;
    };

    h.begin_frame();
    h.draw_text(
        font,
        "H",
        Mat4::IDENTITY,
        120.0,
        TextAlign::Left,
        [1.0, 1.0, 1.0, 0.5],
    );
    h.render_frame();
    let pixels = h.read_pixels();
    let max_green = pixels.chunks_exact(4).map(|p| p[1]).max().unwrap_or(0);

    // 50 % alpha white over black in linear light => linear 0.5 => sRGB 188.
    assert!(
        (186..=190).contains(&max_green),
        "50 % white over black must encode to sRGB ~188, got {max_green}"
    );
}

#[test]
fn fonts_can_be_freed_and_recycled() {
    let _guard = GPU_LOCK.lock().unwrap();
    let Some(bytes) = roboto_bytes() else { return };
    let Some(mut h) = harness(32, 32, TextureFormat::Rgba8Unorm) else {
        return;
    };
    let font = h.register_font(&bytes, charset::ascii()).expect("font");
    assert_eq!(h.renderer().font_count_2d(), 1);
    h.free_font(font);
    assert_eq!(h.renderer().font_count_2d(), 0);

    // A stale handle must not draw and must not panic.
    h.begin_frame();
    h.draw_text(font, "H", Mat4::IDENTITY, 16.0, TextAlign::Left, WHITE);
    let report = h.render_frame();
    assert_eq!(report.text_draw_calls, 0);

    let recycled = h.register_font(&bytes, charset::ascii()).expect("font");
    assert_eq!(recycled, font, "the freed id must be recycled");
}
