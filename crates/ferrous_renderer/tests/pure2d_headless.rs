//! Off-screen (headless) integration tests for the Pure2D render path.
//!
//! These tests instantiate a real `wgpu::Device` from a fallback software
//! adapter (lavapipe / llvmpipe), render 2D geometry into an off-screen
//! `wgpu::Texture` **without a Surface or window**, read the pixels back, and
//! assert both the rendered output and the hardware-independent counters.
//!
//! Run with:
//! ```text
//! export CARGO_TARGET_DIR=/opt/softwaremaker/workspaces/96a0b3c0-be13-4354-8674-72220ab22596/target
//! export VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json
//! export VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json
//! export WGPU_BACKEND=vulkan
//! cargo test -p ferrous_renderer --test pure2d_headless -- --nocapture
//! ```

use std::sync::Mutex;

use ferrous_2d::render::{ShapeInstance, SpriteInstance};
use ferrous_renderer::headless::{Pure2dHarness, DEFAULT_FORMAT};
use glam::{Mat4, Quat, Vec2, Vec3};
use wgpu::TextureFormat;

/// Serialises the tests: each creates its own software device, and running
/// several lavapipe devices concurrently on a 4-vCPU box mostly measures
/// scheduler contention.
static GPU_LOCK: Mutex<()> = Mutex::new(());

const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

fn sprite(center: Vec2, size: Vec2, z: f32) -> SpriteInstance {
    let model = Mat4::from_scale_rotation_translation(
        Vec3::new(size.x, size.y, 1.0),
        Quat::IDENTITY,
        Vec3::new(center.x, center.y, z),
    );
    SpriteInstance {
        transform_c0: model.x_axis.into(),
        transform_c1: model.y_axis.into(),
        transform_c2: model.z_axis.into(),
        transform_c3: model.w_axis.into(),
        color: [1.0, 1.0, 1.0, 1.0],
        uv_rect: [0.0, 0.0, 1.0, 1.0],
        properties: [0.0, 0.0, 0.0, 0.0],
    }
}

fn assert_rgb_near(actual: [u8; 4], expected: [u8; 3], tol: u8, what: &str) {
    for (i, (&a, &e)) in actual[..3].iter().zip(expected.iter()).enumerate() {
        assert!(
            a.abs_diff(e) <= tol,
            "{what}: channel {i} = {a}, expected {e} +/- {tol} (full pixel {actual:?})"
        );
    }
}

/// Renders a single red rectangle into a 256x256 off-screen target and checks
/// the actual pixels.  `Rgba8Unorm` (non-sRGB) is used so the read-back bytes
/// are the shader's raw output and can be compared exactly.
#[test]
fn renders_shape_offscreen_and_matches_pixels() {
    let _guard = GPU_LOCK.lock().unwrap();
    let mut h = Pure2dHarness::new(256, 256, TextureFormat::Rgba8Unorm).expect("headless gpu");

    h.begin_frame();
    h.draw_shape(ShapeInstance::rect(
        Vec2::ZERO,
        Vec2::new(100.0, 100.0),
        RED,
    ));
    let report = h.render_frame();

    // Hardware-independent accounting.
    assert_eq!(report.draw_calls, 1, "a single shape is one draw call");
    assert_eq!(report.shape_draw_calls, 1);
    assert_eq!(report.sprite_draw_calls, 0);
    assert_eq!(report.instances_uploaded, 1);
    assert_eq!(
        report.bytes_uploaded,
        std::mem::size_of::<ShapeInstance>() as u64,
        "exactly one ShapeInstance must be uploaded"
    );
    assert_eq!(
        report.buffer_reallocations, 0,
        "must fit in the initial 1024-instance capacity"
    );

    // Pixel checks. Center is inside the 100x100 rect (world units == pixels,
    // origin at texture centre); (2,2) is clear-colour background.
    let center = h.read_pixel(128, 128);
    let corner = h.read_pixel(2, 2);
    assert_rgb_near(center, [255, 0, 0], 4, "rect centre");
    assert_rgb_near(corner, [0, 0, 0], 4, "background corner");
    assert_eq!(corner[3], 255, "opaque clear colour");
}

/// Verifies the draw-call accounting for a mixed shape+sprite frame.
///
/// The three shapes share `z = 0.0` and are pushed consecutively, so the
/// engine's contiguous-run merge collapses them into one shape draw call.  The
/// two sprites share one texture and also share `z = 0.0`, collapsing into one
/// sprite draw call.  (This relies on `sort_unstable` preserving equal-key
/// order for small slices; if the engine's sort strategy changes, the merged
/// run breaks and this test becomes a useful early warning.)
#[test]
fn draw_calls_match_batched_scene() {
    let _guard = GPU_LOCK.lock().unwrap();
    let mut h = Pure2dHarness::new(128, 128, DEFAULT_FORMAT).expect("headless gpu");
    let tex = h.register_texture_rgba8(2, 2, &[255u8; 16]);

    h.begin_frame();
    h.draw_shape(ShapeInstance::rect_z(Vec2::new(-24.0, 0.0), 0.0, Vec2::splat(12.0), RED));
    h.draw_shape(ShapeInstance::rect_z(Vec2::ZERO, 0.0, Vec2::splat(12.0), GREEN));
    h.draw_shape(ShapeInstance::rect_z(Vec2::new(24.0, 0.0), 0.0, Vec2::splat(12.0), BLUE));
    h.draw_sprite(tex, sprite(Vec2::new(-8.0, -24.0), Vec2::splat(8.0), 0.0));
    h.draw_sprite(tex, sprite(Vec2::new(8.0, -24.0), Vec2::splat(8.0), 0.0));

    let report = h.render_frame();

    assert_eq!(report.instances_uploaded, 5);
    assert_eq!(
        report.shape_draw_calls, 1,
        "3 equal-z contiguous shapes must merge into 1 draw"
    );
    assert_eq!(
        report.sprite_draw_calls, 1,
        "2 equal-z sprites sharing one texture must merge into 1 draw"
    );
    assert_eq!(report.draw_calls, 2);
    assert_eq!(
        report.bytes_uploaded,
        (3 * std::mem::size_of::<ShapeInstance>()
            + 2 * std::mem::size_of::<SpriteInstance>()) as u64
    );
}

/// M5 regression: the unified shape+sprite command stream must have a
/// **deterministic total order**. `z` is the primary sort key and equal-`z`
/// commands are tie-broken by the command itself (`DrawCommand2d` derived
/// `Ord`), which reproduces insertion order. This is what makes the painter's
/// order and the contiguous-run coalescing contractual instead of an accident
/// of an unstable sort.
#[test]
fn z_order_is_deterministic_for_equal_and_interleaved_z() {
    let _guard = GPU_LOCK.lock().unwrap();
    let mut h = Pure2dHarness::new(128, 256, TextureFormat::Rgba8Unorm).expect("headless gpu");

    // Equal z: both shapes share z = 0, so the later-inserted one wins the
    // painter's order. Repeat identical frames to assert determinism.
    for iteration in 0..3 {
        h.begin_frame();
        h.draw_shape(ShapeInstance::rect_z(Vec2::ZERO, 0.0, Vec2::splat(64.0), RED));
        h.draw_shape(ShapeInstance::rect_z(Vec2::ZERO, 0.0, Vec2::splat(64.0), GREEN));
        let report = h.render_frame();

        assert_eq!(
            report.shape_draw_calls, 1,
            "iteration {iteration}: equal-z contiguous shapes must coalesce"
        );
        assert_rgb_near(
            h.read_pixel(64, 128),
            [0, 255, 0],
            4,
            "equal-z tie-break must paint the later command on top",
        );
    }

    // Interleaved/differing z: z is the primary key, so a shape pushed *first*
    // with a higher z must still be painted last. Two distinct z values cannot
    // coalesce.
    h.begin_frame();
    h.draw_shape(ShapeInstance::rect_z(Vec2::ZERO, 0.5, Vec2::splat(64.0), GREEN));
    h.draw_shape(ShapeInstance::rect_z(Vec2::ZERO, 0.0, Vec2::splat(64.0), RED));
    let report = h.render_frame();

    assert_eq!(
        report.shape_draw_calls, 2,
        "different z values must never share a coalesced draw"
    );
    assert_rgb_near(
        h.read_pixel(64, 128),
        [0, 255, 0],
        4,
        "higher z must be painted last regardless of insertion order",
    );
}

/// H10 regression: `bind_groups_total` must report the number of **live** sprite
/// bind groups. After the texture-id free-list landed (id recycling),
/// `next_sprite_texture_id` is only a high-water mark and over-reports once a
/// texture is freed.
#[test]
fn bind_groups_total_tracks_live_groups_after_free() {
    let _guard = GPU_LOCK.lock().unwrap();
    let mut h = Pure2dHarness::new(64, 64, DEFAULT_FORMAT).expect("headless gpu");
    let t0 = h.register_texture_rgba8(2, 2, &[255, 0, 0, 255].repeat(4));
    let t1 = h.register_texture_rgba8(2, 2, &[0, 255, 0, 255].repeat(4));

    h.begin_frame();
    h.draw_sprite(t0, sprite(Vec2::new(-16.0, 0.0), Vec2::splat(8.0), 0.0));
    h.draw_sprite(t1, sprite(Vec2::new(16.0, 0.0), Vec2::splat(8.0), 0.0));
    let first = h.render_frame();
    assert_eq!(first.bind_groups_total, 2, "two live textures, two bind groups");

    h.renderer_mut().free_sprite_texture(t0);
    let after_free = h.render_frame();
    assert_eq!(
        after_free.bind_groups_total, 1,
        "the freed texture's bind group must no longer be counted"
    );
}

/// Each distinct sprite texture adds a draw call and a bind group; the bind
/// group must be created **once**, not per frame.
#[test]
fn sprite_textures_create_bind_groups_once() {
    let _guard = GPU_LOCK.lock().unwrap();
    let mut h = Pure2dHarness::new(64, 64, DEFAULT_FORMAT).expect("headless gpu");
    let t0 = h.register_texture_rgba8(2, 2, &[255, 0, 0, 255].repeat(4));
    let t1 = h.register_texture_rgba8(2, 2, &[0, 255, 0, 255].repeat(4));
    assert_eq!(t0, 0);
    assert_eq!(t1, 1);

    h.begin_frame();
    h.draw_sprite(t0, sprite(Vec2::new(-16.0, 0.0), Vec2::splat(8.0), 0.0));
    h.draw_sprite(t1, sprite(Vec2::new(16.0, 0.0), Vec2::splat(8.0), 1.0));
    let first = h.render_frame();

    assert_eq!(first.bind_groups_total, 2, "one bind group per texture");
    assert_eq!(first.bind_groups_created, 2, "both created at setup");
    assert_eq!(
        first.sprite_draw_calls, 2,
        "different textures can never share a draw"
    );

    // Second frame with the same textures: no new bind groups, no reallocs.
    h.begin_frame();
    h.draw_sprite(t0, sprite(Vec2::new(-16.0, 0.0), Vec2::splat(8.0), 0.0));
    h.draw_sprite(t1, sprite(Vec2::new(16.0, 0.0), Vec2::splat(8.0), 1.0));
    let second = h.render_frame();
    assert_eq!(second.bind_groups_created, 0, "steady state creates no bind groups");
    assert_eq!(second.buffer_reallocations, 0);
}

/// Pins the per-frame-clear contract that the engine runner now enforces at the
/// start of every frame (`crates/ferrous_engine/src/runner/frame.rs`). The
/// runner path itself needs a window/event loop and is not testable headlessly,
/// but the harness exposes the same `begin_frame()` contract:
///
/// - Without the clear the shape batcher is append-only, so each frame re-uploads
///   the previous frame's shapes too ("ghosting").
/// - With the clear every frame uploads only its own geometry.
#[test]
fn per_frame_clear_prevents_shape_accumulation() {
    let _guard = GPU_LOCK.lock().unwrap();
    let mut h = Pure2dHarness::new(64, 64, TextureFormat::Rgba8Unorm).expect("headless gpu");

    // No `clear_2d()`: the shape batcher is not reset between renders. The
    // per-frame counters are cumulative, so reset only those between renders to
    // read each frame's upload count independently.
    h.draw_shape(ShapeInstance::rect(Vec2::ZERO, Vec2::splat(8.0), RED));
    h.renderer_mut().renderer_2d_pure.reset_frame_counters();
    let first = h.render_frame();
    assert_eq!(first.instances_uploaded, 1);

    h.draw_shape(ShapeInstance::rect(Vec2::ZERO, Vec2::splat(8.0), GREEN));
    h.renderer_mut().renderer_2d_pure.reset_frame_counters();
    let second = h.render_frame();
    assert_eq!(
        second.instances_uploaded, 2,
        "without a per-frame clear the previous frame's shapes are still batched"
    );

    // With the contract the runner applies, each frame is independent.
    h.begin_frame();
    h.draw_shape(ShapeInstance::rect(Vec2::ZERO, Vec2::splat(8.0), RED));
    let third = h.render_frame();
    assert_eq!(
        third.instances_uploaded, 1,
        "begin_frame()/clear_2d() resets the 2D batchers"
    );
}
