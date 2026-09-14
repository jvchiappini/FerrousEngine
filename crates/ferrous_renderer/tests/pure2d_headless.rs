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
use ferrous_renderer::Render2dConfig;
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
    // with a higher z must still be painted last. Since the H3 coalescing pass
    // groups by (pipeline, texture) and reorders the upload into painter order,
    // distinct-z shapes now share **one** draw call while preserving order.
    h.begin_frame();
    h.draw_shape(ShapeInstance::rect_z(Vec2::ZERO, 0.5, Vec2::splat(64.0), GREEN));
    h.draw_shape(ShapeInstance::rect_z(Vec2::ZERO, 0.0, Vec2::splat(64.0), RED));
    let report = h.render_frame();

    assert_eq!(
        report.shape_draw_calls, 1,
        "distinct-z shapes must coalesce into a single draw (H3)"
    );
    assert_rgb_near(
        h.read_pixel(64, 128),
        [0, 255, 0],
        4,
        "higher z must be painted last regardless of insertion order",
    );
}

/// H3 for sprites: a run of consecutive sprites that share a texture but have
/// distinct z must coalesce into **one** draw call, while the interleaving with
/// a second texture is preserved (one draw per contiguous same-texture span).
#[test]
fn distinct_z_same_texture_sprites_coalesce() {
    let _guard = GPU_LOCK.lock().unwrap();
    let mut h = Pure2dHarness::new(128, 128, DEFAULT_FORMAT).expect("headless gpu");
    let t0 = h.register_texture_rgba8(2, 2, &[255, 0, 0, 255].repeat(4));
    let t1 = h.register_texture_rgba8(2, 2, &[0, 255, 0, 255].repeat(4));

    // Three t0 sprites with distinct z, then one t1 sprite, then two more t0.
    h.begin_frame();
    h.draw_sprite(t0, sprite(Vec2::new(-24.0, 0.0), Vec2::splat(8.0), 0.0));
    h.draw_sprite(t0, sprite(Vec2::new(-8.0, 0.0), Vec2::splat(8.0), 1.0));
    h.draw_sprite(t0, sprite(Vec2::new(8.0, 0.0), Vec2::splat(8.0), 2.0));
    h.draw_sprite(t1, sprite(Vec2::new(0.0, 16.0), Vec2::splat(8.0), 3.0));
    h.draw_sprite(t0, sprite(Vec2::new(24.0, 0.0), Vec2::splat(8.0), 4.0));
    h.draw_sprite(t0, sprite(Vec2::new(40.0, 0.0), Vec2::splat(8.0), 5.0));
    let report = h.render_frame();

    assert_eq!(
        report.sprite_draw_calls, 3,
        "runs: [t0 x3], [t1], [t0 x2] => 3 draws (not 6, not 2)"
    );
    assert_eq!(report.instances_uploaded, 6);
}

/// Runtime config A/B: the `scratch_reuse` knob must be settable/gettable and
/// both branches must render identically.  This guards the non-reuse path that
/// consumers use for benchmarking.
#[test]
fn scratch_reuse_config_toggles_and_both_paths_render() {
    let _guard = GPU_LOCK.lock().unwrap();
    let mut h = Pure2dHarness::new(64, 64, TextureFormat::Rgba8Unorm).expect("headless gpu");

    assert!(h.renderer().render_2d_config().scratch_reuse, "default is reuse");

    h.renderer_mut().set_render_2d_config(Render2dConfig {
        scratch_reuse: false,
        ..Default::default()
    });
    assert!(!h.renderer().render_2d_config().scratch_reuse);

    h.begin_frame();
    h.draw_shape(ShapeInstance::rect(Vec2::ZERO, Vec2::splat(32.0), RED));
    let report = h.render_frame();
    assert_eq!(report.draw_calls, 1);
    assert_rgb_near(h.read_pixel(32, 32), [255, 0, 0], 4, "rect centre (no reuse)");

    // Switch back on and confirm the path still coalesces.
    h.renderer_mut().set_render_2d_config(Render2dConfig {
        scratch_reuse: true,
        ..Default::default()
    });
    h.begin_frame();
    h.draw_shape(ShapeInstance::rect_z(Vec2::ZERO, 0.0, Vec2::splat(32.0), GREEN));
    h.draw_shape(ShapeInstance::rect_z(Vec2::ZERO, 1.0, Vec2::splat(16.0), BLUE));
    let report = h.render_frame();
    assert_eq!(report.draw_calls, 1, "distinct-z shapes still coalesce after re-enable");
}

/// New public metrics API: `Renderer::frame_2d_stats()` must report the live
/// Pure2D counters of the last frame, and reset them at frame start.
#[test]
fn frame_2d_stats_reports_live_pure2d_counters() {
    let _guard = GPU_LOCK.lock().unwrap();
    let mut h = Pure2dHarness::new(64, 64, DEFAULT_FORMAT).expect("headless gpu");

    // Before any 2D work the counters are zero.
    assert_eq!(h.renderer().frame_2d_stats().draw_calls, 0);

    h.begin_frame();
    h.draw_shape(ShapeInstance::rect(Vec2::ZERO, Vec2::splat(16.0), RED));
    h.draw_shape(ShapeInstance::rect(
        Vec2::new(16.0, 0.0),
        Vec2::splat(16.0),
        BLUE,
    ));
    let _ = h.render_frame();

    let stats = h.renderer().frame_2d_stats();
    assert!(
        stats.draw_calls > 0,
        "frame_2d_stats must be populated on the live Pure2D path"
    );
    assert_eq!(stats.draw_calls, 1, "two equal-z shapes coalesce");
    assert_eq!(stats.shape_draw_calls, 1);
    assert_eq!(stats.instances_uploaded, 2);
    assert!(stats.bytes_uploaded >= 2 * std::mem::size_of::<ShapeInstance>() as u64);

    // Next frame with no geometry: the counters must have been reset.
    h.begin_frame();
    let _ = h.render_frame();
    assert_eq!(
        h.renderer().frame_2d_stats(),
        ferrous_2d::render::Frame2dCounters::default(),
        "counters must reset at frame start"
    );
}

/// 2D culling correctness: with culling ON the image must be byte-identical to
/// culling OFF for every visible instance, while the off-screen instances are
/// reported as culled and are neither uploaded nor billed as bytes.
#[test]
fn culling_preserves_visible_pixels_and_drops_offscreen_instances() {
    let _guard = GPU_LOCK.lock().unwrap();
    let mut h = Pure2dHarness::new(128, 128, TextureFormat::Rgba8Unorm).expect("headless gpu");
    let t_vis = h.register_texture_rgba8(2, 2, &[0, 255, 0, 255].repeat(4));

    let draw_scene = |h: &mut Pure2dHarness| {
        h.begin_frame();
        // Visible geometry (inside [-64, 64]^2 world rect).
        h.draw_shape(ShapeInstance::rect(Vec2::ZERO, Vec2::splat(40.0), RED));
        h.draw_shape(ShapeInstance::circle(Vec2::new(40.0, 40.0), 10.0, BLUE));
        h.draw_sprite(t_vis, sprite(Vec2::new(-40.0, -40.0), Vec2::splat(12.0), 0.0));
        // Off-screen geometry: 64 shapes + 64 sprites, all far outside.
        for i in 0..64 {
            let x = 1_000.0 + i as f32 * 3.0;
            h.draw_shape(ShapeInstance::rect(Vec2::new(x, 0.0), Vec2::splat(5.0), GREEN));
            h.draw_sprite(t_vis, sprite(Vec2::new(x, 500.0), Vec2::splat(5.0), 1.0));
        }
    };

    h.renderer_mut()
        .set_render_2d_config(Render2dConfig { cull_2d: false, ..Default::default() });
    draw_scene(&mut h);
    let off = h.render_frame();
    let pixels_off = h.read_pixels();
    assert_eq!(off.culled_instances, 0, "culling off reports nothing culled");

    h.renderer_mut()
        .set_render_2d_config(Render2dConfig { cull_2d: true, ..Default::default() });
    draw_scene(&mut h);
    let on = h.render_frame();
    let pixels_on = h.read_pixels();

    assert_eq!(
        pixels_on, pixels_off,
        "culling must not change any visible pixel"
    );
    assert_eq!(on.culled_instances, 128, "64 shapes + 64 sprites are off-screen");
    assert_eq!(
        on.instances_uploaded + 128,
        off.instances_uploaded,
        "culled instances must not be uploaded"
    );
    assert!(
        on.bytes_uploaded < off.bytes_uploaded,
        "culling must reduce bytes uploaded"
    );
}

/// Culling also removes whole texture runs, so it lowers draw calls (not just
/// bytes): an off-screen texture interleaved with a visible one disappears and
/// the surviving visible sprites coalesce into a single run.
#[test]
fn culling_reduces_draw_calls_for_interleaved_culled_texture_runs() {
    let _guard = GPU_LOCK.lock().unwrap();
    let mut h = Pure2dHarness::new(128, 128, TextureFormat::Rgba8Unorm).expect("headless gpu");
    let t_vis = h.register_texture_rgba8(2, 2, &[0, 255, 0, 255].repeat(4));
    let t_off = h.register_texture_rgba8(2, 2, &[255, 0, 0, 255].repeat(4));

    let draw_scene = |h: &mut Pure2dHarness| {
        h.begin_frame();
        // Alternate an off-screen sprite (t_off) before each visible one (t_vis).
        for i in 0..8 {
            h.draw_sprite(
                t_off,
                sprite(Vec2::new(5_000.0, 0.0), Vec2::splat(8.0), (i * 2) as f32),
            );
            h.draw_sprite(
                t_vis,
                sprite(Vec2::new(-28.0 + i as f32 * 8.0, 0.0), Vec2::splat(6.0), (i * 2 + 1) as f32),
            );
        }
    };

    h.renderer_mut()
        .set_render_2d_config(Render2dConfig { cull_2d: false, ..Default::default() });
    draw_scene(&mut h);
    let off = h.render_frame();

    h.renderer_mut()
        .set_render_2d_config(Render2dConfig { cull_2d: true, ..Default::default() });
    draw_scene(&mut h);
    let on = h.render_frame();

    assert_eq!(
        off.sprite_draw_calls, 16,
        "alternating textures => 16 single-sprite runs"
    );
    assert_eq!(on.culled_instances, 8);
    assert_eq!(
        on.sprite_draw_calls, 1,
        "surviving t_vis sprites become one contiguous run"
    );
    assert!(on.draw_calls < off.draw_calls, "culling must lower draw calls");
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

/// The triangle primitive must fill its interior, leave the exterior empty,
/// and anti-alias its edges (a pixel exactly on the hypotenuse is neither fully
/// covered nor fully transparent).
///
/// Pixel↔world mapping in Pure2D: texel `(i, j)` samples world
/// `(i + 0.5 - W/2, H/2 - (j + 0.5))`.  With `W = H = 256` and the triangle
/// `a=(0,0)`, `b=(80,0)`, `c=(0,80)` (region `x >= 0, y >= 0, x + y <= 80`):
/// `(154,101) -> (26.5, 26.5)` is interior, `(207,47) -> (79.5, 80.5)` is
/// exterior and `(168,88) -> (40.5, 39.5)` lies exactly on `x + y = 80`.
#[test]
fn triangle_fills_centroid_with_aa_edges() {
    let _guard = GPU_LOCK.lock().unwrap();
    let mut h = Pure2dHarness::new(256, 256, TextureFormat::Rgba8Unorm).expect("headless gpu");

    h.begin_frame();
    h.draw_shape(ShapeInstance::triangle(
        Vec2::new(0.0, 0.0),
        Vec2::new(80.0, 0.0),
        Vec2::new(0.0, 80.0),
        RED,
    ));
    let report = h.render_frame();

    assert_eq!(report.instances_uploaded, 1, "one triangle instance");
    assert_eq!(report.draw_calls, 1);

    let inside = h.read_pixel(154, 101);
    let outside = h.read_pixel(207, 47);
    let edge = h.read_pixel(168, 88);

    assert_rgb_near(inside, [255, 0, 0], 8, "triangle interior must be filled");
    assert_rgb_near(outside, [0, 0, 0], 8, "exterior must stay empty");
    assert!(
        edge[0] > 60 && edge[0] < 200,
        "pixel on the hypotenuse must be partial AA coverage, got {edge:?}"
    );

    // Two-sided: the same triangle with reversed winding must render
    // identically (the SDF sign derives from the winding, so no face culling
    // nor inside/outside flip).
    h.begin_frame();
    h.draw_shape(ShapeInstance::triangle(
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, 80.0),
        Vec2::new(80.0, 0.0),
        RED,
    ));
    h.render_frame();
    assert_rgb_near(
        h.read_pixel(154, 101),
        [255, 0, 0],
        8,
        "reversed winding must still fill the interior",
    );
}

/// `Renderer::draw_2d_shapes` must be exactly equivalent to calling
/// `draw_2d_shape` once per element: same counters, same H3 coalescing, same
/// pixels.
#[test]
fn bulk_draw_2d_shapes_matches_individual_calls() {
    let _guard = GPU_LOCK.lock().unwrap();
    let shapes = [
        ShapeInstance::rect(Vec2::new(-24.0, 0.0), Vec2::splat(12.0), RED),
        ShapeInstance::circle(Vec2::ZERO, 8.0, GREEN),
        ShapeInstance::triangle(
            Vec2::new(0.0, 20.0),
            Vec2::new(20.0, 20.0),
            Vec2::new(0.0, 40.0),
            BLUE,
        ),
    ];
    let mut h = Pure2dHarness::new(128, 128, TextureFormat::Rgba8Unorm).expect("headless gpu");

    h.begin_frame();
    for s in &shapes {
        h.draw_shape(*s);
    }
    let per_call = h.render_frame();
    let per_call_pixels = h.read_pixels();

    h.begin_frame();
    h.renderer_mut().draw_2d_shapes(&shapes);
    let bulk = h.render_frame();
    let bulk_pixels = h.read_pixels();

    assert_eq!(per_call.draw_calls, bulk.draw_calls);
    assert_eq!(per_call.shape_draw_calls, bulk.shape_draw_calls);
    assert_eq!(per_call.instances_uploaded, bulk.instances_uploaded);
    assert_eq!(per_call.bytes_uploaded, bulk.bytes_uploaded);
    assert_eq!(
        per_call_pixels, bulk_pixels,
        "bulk draw must produce pixel-identical output"
    );
}

fn regular_polygon(n: usize, radius: f32) -> Vec<Vec2> {
    (0..n)
        .map(|k| {
            let a = std::f32::consts::TAU * (k as f32) / (n as f32);
            Vec2::new(radius * a.cos(), radius * a.sin())
        })
        .collect()
}

/// Approximates a convex polygon fill with one `rect` instance per world unit
/// of height — the pre-triangle "scan-fill style" workaround.
fn scanline_rect_fill(poly: &[Vec2], color: [f32; 4]) -> Vec<ShapeInstance> {
    let min_y = poly.iter().fold(f32::INFINITY, |m, p| m.min(p.y));
    let max_y = poly.iter().fold(f32::NEG_INFINITY, |m, p| m.max(p.y));
    let mut out = Vec::new();
    let mut y = min_y.ceil();
    while y <= max_y {
        let mut xs = Vec::new();
        for i in 0..poly.len() {
            let p0 = poly[i];
            let p1 = poly[(i + 1) % poly.len()];
            let dy = p1.y - p0.y;
            if (p0.y - y) * (p1.y - y) <= 0.0 && dy.abs() > 1e-6 {
                let t = (y - p0.y) / dy;
                xs.push(p0.x + t * (p1.x - p0.x));
            }
        }
        if let Some(min) = xs.iter().copied().reduce(f32::min) {
            let max = xs.iter().copied().reduce(f32::max).unwrap();
            if max > min {
                out.push(ShapeInstance::rect(
                    Vec2::new((min + max) * 0.5, y),
                    Vec2::new(max - min, 1.0),
                    color,
                ));
            }
        }
        y += 1.0;
    }
    out
}

/// Convex polygon fill: old scan-line approximation (N rect instances) versus
/// the new triangle fan (one triangle per side).  Reports the
/// hardware-independent counters for a 64-gon; no wall-clock claim is made.
#[test]
fn polygon_fill_triangle_fan_vs_scanline_rects_64gon() {
    let _guard = GPU_LOCK.lock().unwrap();
    let n = 64usize;
    let poly = regular_polygon(n, 100.0);
    let rects = scanline_rect_fill(&poly, RED);
    let tris: Vec<ShapeInstance> = (0..n)
        .map(|i| ShapeInstance::triangle(Vec2::ZERO, poly[i], poly[(i + 1) % n], GREEN))
        .collect();

    let mut h = Pure2dHarness::new(256, 256, TextureFormat::Rgba8Unorm).expect("headless gpu");

    h.begin_frame();
    h.renderer_mut().draw_2d_shapes(&rects);
    let old = h.render_frame();

    h.begin_frame();
    h.renderer_mut().draw_2d_shapes(&tris);
    let new = h.render_frame();

    println!(
        "[polygon 64-gon] scanline_rects: instances={} draw_calls={} bytes={} | triangle_fan: instances={} draw_calls={} bytes={}",
        old.instances_uploaded, old.draw_calls, old.bytes_uploaded,
        new.instances_uploaded, new.draw_calls, new.bytes_uploaded,
    );

    assert_eq!(old.instances_uploaded, rects.len() as u32);
    assert_eq!(new.instances_uploaded, n as u32, "one triangle per 64-gon side");
    assert!(
        rects.len() > n,
        "scan-line fill needs more instances than the fan ({} vs {n})",
        rects.len()
    );
    assert_eq!(old.draw_calls, 1, "H3 coalesces same-pipeline shapes to one draw");
    assert_eq!(new.draw_calls, 1);
    assert!(
        new.bytes_uploaded < old.bytes_uploaded,
        "triangle fan must upload fewer bytes than the rect approximation"
    );
    assert_rgb_near(
        h.read_pixel(128, 128),
        [0, 255, 0],
        8,
        "triangle fan must fill the polygon centre",
    );
}

/// `polygon_triangle(..., 0b111)` must be byte-for-byte identical to
/// `triangle(...)` (which now delegates to it with the full mask).
#[test]
fn polygon_triangle_full_mask_equals_triangle() {
    let _guard = GPU_LOCK.lock().unwrap();
    let a = Vec2::new(0.0, 0.0);
    let b = Vec2::new(80.0, 0.0);
    let c = Vec2::new(0.0, 80.0);
    let mut h = Pure2dHarness::new(128, 128, TextureFormat::Rgba8Unorm).expect("headless gpu");

    h.begin_frame();
    h.draw_shape(ShapeInstance::triangle(a, b, c, RED));
    h.render_frame();
    let triangle_pixels = h.read_pixels();

    h.begin_frame();
    h.draw_shape(ShapeInstance::polygon_triangle(a, b, c, RED, 0b111));
    let report = h.render_frame();
    let polygon_pixels = h.read_pixels();

    assert_eq!(report.instances_uploaded, 1);
    assert_eq!(
        triangle_pixels, polygon_pixels,
        "full-mask polygon_triangle must match triangle()"
    );
}

/// The seam regression: a quad split along its diagonal into two triangles must
/// render as a continuous fill when the shared diagonal is marked internal
/// (hard) and only the outer edges are AA.  The same quad with the all-AA
/// `triangle()` is sampled as a witness that the diagonal seam is what the
/// mask removes.
///
/// Quad `(0,0)-(100,0)-(100,100)-(0,100)`, diagonal `p0-p2`.  Pure2D maps
/// texel `(i,j)` to world `(i + 0.5 - 128, 127.5 - j)`, so `x == y` on the
/// diagonal is exactly `i + j == 255`.
#[test]
fn triangulated_quad_has_no_internal_seam() {
    let _guard = GPU_LOCK.lock().unwrap();
    let p0 = Vec2::new(0.0, 0.0);
    let p1 = Vec2::new(100.0, 0.0);
    let p2 = Vec2::new(100.0, 100.0);
    let p3 = Vec2::new(0.0, 100.0);
    let mut h = Pure2dHarness::new(256, 256, TextureFormat::Rgba8Unorm).expect("headless gpu");

    // Tri A = p0,p1,p2: bottom (bit0) and right (bit1) external, diagonal
    // p2->p0 (bit2) internal -> 0b011.  Tri B = p0,p2,p3: diagonal p0->p2
    // (bit0) internal, top (bit1) and left (bit2) external -> 0b110.
    h.begin_frame();
    h.draw_shape(ShapeInstance::polygon_triangle(p0, p1, p2, RED, 0b011));
    h.draw_shape(ShapeInstance::polygon_triangle(p0, p2, p3, RED, 0b110));
    let masked = h.render_frame();
    assert_eq!(masked.instances_uploaded, 2);
    assert_eq!(masked.draw_calls, 1, "same-pipeline shapes coalesce");

    let pixels = h.read_pixels();
    let at = |x: u32, y: u32| -> [u8; 4] {
        let idx = ((y * 256 + x) * 4) as usize;
        [pixels[idx], pixels[idx + 1], pixels[idx + 2], pixels[idx + 3]]
    };

    // Scan the whole diagonal interior (away from the two corners p0/p2, whose
    // AA is legitimately partial): every pixel must be full red.
    for i in 140u32..=215 {
        let (x, y) = (i, 255 - i);
        let px = at(x, y);
        if px[0] < 235 {
            println!("[seam] masked diagonal ({x},{y}) = {px:?}");
        }
        assert!(
            px[0] >= 235,
            "masked diagonal pixel ({x},{y}) must be full red, got {px:?}"
        );
    }
    // Both sides of the diagonal plus an interior point must also be full red.
    for &(x, y) in &[(158u32, 108u32), (148, 98), (178, 108)] {
        let px = at(x, y);
        println!("[seam] masked interior ({x},{y}) = {px:?}");
        assert!(
            px[0] >= 235,
            "interior pixel ({x},{y}) must be full red, got {px:?}"
        );
    }

    // Outer contour must still be anti-aliased: left edge x=0 (i=127 samples
    // x=-0.5) and bottom edge y=0 (j=127 samples y=0.5) are partial.
    let left = at(127, 77);
    let bottom = at(177, 127);
    println!("[seam] masked left-edge AA = {left:?}, bottom-edge AA = {bottom:?}");
    assert!(
        left[0] > 40 && left[0] < 230,
        "left edge must be partial AA, got {left:?}"
    );
    assert!(
        bottom[0] > 40 && bottom[0] < 230,
        "bottom edge must be partial AA, got {bottom:?}"
    );

    // Witness: the all-AA quad shows the seam on the diagonal.
    h.begin_frame();
    h.draw_shape(ShapeInstance::triangle(p0, p1, p2, RED));
    h.draw_shape(ShapeInstance::triangle(p0, p2, p3, RED));
    h.render_frame();
    let seam = h.read_pixel(150, 105);
    println!("[seam] all-AA diagonal (150,105) = {seam:?}");
    assert!(
        seam[0] < 235,
        "all-AA triangles are expected to show the diagonal seam, got {seam:?}"
    );
}

/// `line_with_cap_style_aa(..., true, true)` must be pixel-identical to
/// `line_with_cap_style(...)` (which now delegates with both caps AA).
#[test]
fn line_cap_aa_full_flags_match_line_with_cap_style() {
    let _guard = GPU_LOCK.lock().unwrap();
    let mut h = Pure2dHarness::new(256, 256, TextureFormat::Rgba8Unorm).expect("headless gpu");
    let (from, to, width) = (Vec2::new(-80.0, 10.0), Vec2::new(70.0, -30.0), 12.0);

    h.begin_frame();
    h.draw_shape(ShapeInstance::line_with_cap_style(
        from,
        to,
        width,
        RED,
        ferrous_2d::CapStyle::Round,
    ));
    h.render_frame();
    let base = h.read_pixels();

    h.begin_frame();
    h.draw_shape(ShapeInstance::line_with_cap_style_aa(
        from,
        to,
        width,
        RED,
        ferrous_2d::CapStyle::Round,
        true,
        true,
    ));
    h.render_frame();
    let with_flags = h.read_pixels();

    assert_eq!(
        base, with_flags,
        "full-AA line_with_cap_style_aa must match line_with_cap_style"
    );
}

/// Wall-chain seam regression: two collinear flat-capped segments share the
/// endpoint `(0,0)`. With that shared cap marked hard (`aa_* = false`) the
/// union must be a continuous fill across `x = 0`; with both caps AA the same
/// scene shows the characteristic seam (witness).
#[test]
fn collinear_segments_with_hard_joint_have_no_seam() {
    let _guard = GPU_LOCK.lock().unwrap();
    let width = 20.0;
    let a_from = Vec2::new(-80.0, 0.0);
    let joint = Vec2::new(0.0, 0.0);
    let b_to = Vec2::new(80.0, 0.0);
    let mut h = Pure2dHarness::new(256, 256, TextureFormat::Rgba8Unorm).expect("headless gpu");

    let at = |pixels: &[u8], x: u32, y: u32| -> [u8; 4] {
        let idx = ((y * 256 + x) * 4) as usize;
        [pixels[idx], pixels[idx + 1], pixels[idx + 2], pixels[idx + 3]]
    };

    // Shared cap hard on both sides: left wall AA at its free end, right wall
    // AA at its free end.
    h.begin_frame();
    h.draw_shape(ShapeInstance::line_with_cap_style_aa(
        a_from,
        joint,
        width,
        RED,
        ferrous_2d::CapStyle::Flat,
        true,
        false,
    ));
    h.draw_shape(ShapeInstance::line_with_cap_style_aa(
        joint,
        b_to,
        width,
        RED,
        ferrous_2d::CapStyle::Flat,
        false,
        true,
    ));
    let report = h.render_frame();
    assert_eq!(report.instances_uploaded, 2);
    assert_eq!(report.draw_calls, 1, "same-pipeline shapes coalesce");
    let pixels = h.read_pixels();

    // Scan the joint column on both sides of x = 0.  Pixel (i, j) samples world
    // (i + 0.5 - 128, 127.5 - j); i = 127 -> x = -0.5, i = 128 -> x = +0.5.
    for &i in &[127u32, 128] {
        for j in 120u32..=135 {
            let px = at(&pixels, i, j);
            if px[0] < 235 {
                println!("[line-seam] hard joint ({i},{j}) = {px:?}");
            }
            assert!(
                px[0] >= 235,
                "hard joint pixel ({i},{j}) must be full red, got {px:?}"
            );
        }
    }
    // The free left cap must still be anti-aliased: x = -80 -> i = 47 samples
    // x = -80.5, just outside the segment.
    let free = at(&pixels, 47, 127);
    println!("[line-seam] free left cap (47,127) = {free:?}");
    assert!(
        free[0] > 40 && free[0] < 230,
        "free cap must be partial AA, got {free:?}"
    );

    // Witness: the same chain with both caps AA shows the seam at x = 0.
    h.begin_frame();
    h.draw_shape(ShapeInstance::line_with_cap_style(
        a_from,
        joint,
        width,
        RED,
        ferrous_2d::CapStyle::Flat,
    ));
    h.draw_shape(ShapeInstance::line_with_cap_style(
        joint,
        b_to,
        width,
        RED,
        ferrous_2d::CapStyle::Flat,
    ));
    h.render_frame();
    let seam = h.read_pixel(128, 127);
    println!("[line-seam] all-AA joint (128,127) = {seam:?}");
    assert!(
        seam[0] < 235,
        "all-AA collinear segments should show the joint seam, got {seam:?}"
    );
}

/// T-junction: a horizontal wall with a hard end butts into the middle of a
/// vertical wall.  The union must have no background gap at the overlap.
#[test]
fn hard_end_against_side_of_another_segment_has_no_gap() {
    let _guard = GPU_LOCK.lock().unwrap();
    let width = 20.0;
    let mut h = Pure2dHarness::new(256, 256, TextureFormat::Rgba8Unorm).expect("headless gpu");

    // Vertical through-wall (both caps free/AA), then horizontal wall whose
    // right end is hard and butts into the vertical wall at (0,0).
    h.begin_frame();
    h.draw_shape(ShapeInstance::line_with_cap_style_aa(
        Vec2::new(0.0, -60.0),
        Vec2::new(0.0, 60.0),
        width,
        RED,
        ferrous_2d::CapStyle::Flat,
        true,
        true,
    ));
    h.draw_shape(ShapeInstance::line_with_cap_style_aa(
        Vec2::new(-80.0, 0.0),
        Vec2::new(0.0, 0.0),
        width,
        RED,
        ferrous_2d::CapStyle::Flat,
        true,
        false,
    ));
    h.render_frame();
    let pixels = h.read_pixels();

    let at = |x: u32, y: u32| -> [u8; 4] {
        let idx = ((y * 256 + x) * 4) as usize;
        [pixels[idx], pixels[idx + 1], pixels[idx + 2], pixels[idx + 3]]
    };

    // World box around the junction x in [-6,6], y in [-6,6]; must be all red.
    for i in 122u32..=134 {
        for j in 122u32..=134 {
            let px = at(i, j);
            if px[0] < 235 {
                println!("[line-tee] gap at ({i},{j}) = {px:?}");
            }
            assert!(
                px[0] >= 235,
                "T-junction pixel ({i},{j}) must be full red, got {px:?}"
            );
        }
    }
}

// ── Retained static 2D batches ───────────────────────────────────────────────

/// Three overlapping shapes with equal `z`, so both the retained and the
/// immediate path preserve insertion order (painter order).
fn static_test_shapes() -> Vec<ShapeInstance> {
    vec![
        ShapeInstance::rect(Vec2::ZERO, Vec2::splat(40.0), RED),
        ShapeInstance::circle(Vec2::new(40.0, 40.0), 12.0, BLUE),
        ShapeInstance::rect_rotated(Vec2::new(-40.0, 20.0), Vec2::new(30.0, 10.0), 0.4, GREEN),
    ]
}

/// (a) A retained batch must render **byte-identical** pixels to drawing the
/// same shapes through the immediate path, while uploading nothing per frame.
#[test]
fn static_batch_matches_immediate_pixels_and_uploads_once() {
    let _guard = GPU_LOCK.lock().unwrap();
    let mut h = Pure2dHarness::new(128, 128, TextureFormat::Rgba8Unorm).expect("headless gpu");
    let shapes = static_test_shapes();

    let id = h.renderer_mut().create_static_2d_batch(&shapes);

    h.begin_frame();
    let static_report = h.render_frame();
    let static_pixels = h.read_pixels();

    assert_eq!(static_report.static_batches_drawn, 1, "one batch drawn");
    assert_eq!(static_report.static_instances_drawn, shapes.len() as u32);
    assert_eq!(
        static_report.shape_draw_calls, 1,
        "one instanced draw call per batch"
    );
    assert_eq!(
        static_report.bytes_uploaded, 0,
        "a retained batch must not upload per frame"
    );
    assert_eq!(static_report.instances_uploaded, 0);
    assert_eq!(static_report.buffer_reallocations, 0);

    // Same geometry through the immediate path (disable the retained draw).
    h.renderer_mut().set_render_2d_config(Render2dConfig {
        static_batches: false,
        ..Default::default()
    });
    h.begin_frame();
    for s in &shapes {
        h.draw_shape(*s);
    }
    let immediate_report = h.render_frame();
    let immediate_pixels = h.read_pixels();

    assert_eq!(
        immediate_pixels, static_pixels,
        "retained and immediate draws must be pixel-identical"
    );
    assert_eq!(immediate_report.static_batches_drawn, 0);
    assert_eq!(immediate_report.instances_uploaded, shapes.len() as u32);
    assert_eq!(
        immediate_report.bytes_uploaded,
        (shapes.len() * std::mem::size_of::<ShapeInstance>()) as u64
    );

    h.renderer_mut().destroy_static_2d_batch(id);
}

/// (b) `update` replaces the content in place without a per-frame upload, and
/// grows the buffer when the new slice exceeds the current capacity.
#[test]
fn static_batch_update_replaces_content_in_place() {
    let _guard = GPU_LOCK.lock().unwrap();
    let mut h = Pure2dHarness::new(128, 128, TextureFormat::Rgba8Unorm).expect("headless gpu");

    let id = h.renderer_mut().create_static_2d_batch(&[ShapeInstance::rect(
        Vec2::ZERO,
        Vec2::splat(40.0),
        RED,
    )]);
    h.begin_frame();
    let _ = h.render_frame();
    assert_rgb_near(h.read_pixel(64, 64), [255, 0, 0], 4, "red after create");

    h.renderer_mut().update_static_2d_batch(
        id,
        &[ShapeInstance::rect(Vec2::ZERO, Vec2::splat(40.0), GREEN)],
    );
    h.begin_frame();
    let updated = h.render_frame();
    assert_rgb_near(h.read_pixel(64, 64), [0, 255, 0], 4, "green after update");
    assert_eq!(updated.static_instances_drawn, 1);
    assert_eq!(updated.bytes_uploaded, 0, "update is not a per-frame upload");

    // Grow past the 128-instance minimum: buffer is recreated, still 0 upload.
    let many: Vec<ShapeInstance> = (0..300)
        .map(|i| {
            ShapeInstance::rect(
                Vec2::new((i % 20) as f32 * 4.0 - 38.0, (i / 20) as f32 * 4.0 - 30.0),
                Vec2::splat(3.0),
                BLUE,
            )
        })
        .collect();
    h.renderer_mut().update_static_2d_batch(id, &many);
    h.begin_frame();
    let grown = h.render_frame();
    assert_eq!(grown.static_instances_drawn, 300);
    assert_eq!(grown.bytes_uploaded, 0);

    // Shrink back: reuse the (larger) buffer, still 0 upload.
    h.renderer_mut().update_static_2d_batch(id, &many[..5]);
    h.begin_frame();
    let shrunk = h.render_frame();
    assert_eq!(shrunk.static_instances_drawn, 5);
    assert_eq!(shrunk.bytes_uploaded, 0);

    h.renderer_mut().destroy_static_2d_batch(id);
}

/// (c) `destroy` frees the batch, stops drawing it and invalidates the handle;
/// a stale handle is a safe no-op even after the slot is recycled.
#[test]
fn static_batch_destroy_stops_drawing_and_invalidates_id() {
    let _guard = GPU_LOCK.lock().unwrap();
    let mut h = Pure2dHarness::new(128, 128, TextureFormat::Rgba8Unorm).expect("headless gpu");

    let id = h.renderer_mut().create_static_2d_batch(&static_test_shapes());
    h.begin_frame();
    let first = h.render_frame();
    assert_eq!(first.static_batches_drawn, 1);

    h.renderer_mut().destroy_static_2d_batch(id);
    h.begin_frame();
    let after = h.render_frame();
    assert_eq!(after.static_batches_drawn, 0);
    assert_eq!(after.draw_calls, 0, "destroyed batch must not draw");
    assert_rgb_near(h.read_pixel(64, 64), [0, 0, 0], 4, "background after destroy");

    // Stale handle: update/destroy are no-ops and must not panic/draw.
    h.renderer_mut().update_static_2d_batch(
        id,
        &[ShapeInstance::rect(Vec2::ZERO, Vec2::splat(40.0), RED)],
    );
    h.renderer_mut().destroy_static_2d_batch(id);
    h.begin_frame();
    let still = h.render_frame();
    assert_eq!(still.draw_calls, 0, "stale id must not draw");

    // Reusing the slot issues a new generation; the old id addresses nothing.
    let new_id = h.renderer_mut().create_static_2d_batch(&[ShapeInstance::rect(
        Vec2::ZERO,
        Vec2::splat(20.0),
        BLUE,
    )]);
    assert_eq!(new_id.index(), id.index());
    assert_ne!(new_id.generation(), id.generation());
    assert_eq!(h.renderer().static_2d_batch_count(), 1);

    // A stale update must not overwrite the live batch.
    h.renderer_mut().update_static_2d_batch(
        id,
        &[ShapeInstance::rect(Vec2::ZERO, Vec2::splat(40.0), RED)],
    );
    h.begin_frame();
    let recycled = h.render_frame();
    assert_eq!(recycled.static_instances_drawn, 1);
    assert_rgb_near(h.read_pixel(64, 64), [0, 0, 255], 4, "live blue batch");

    h.renderer_mut().destroy_static_2d_batch(new_id);
}

/// (d) A retained batch must not upload anything across steady-state frames.
#[test]
fn static_batch_has_no_per_frame_upload() {
    let _guard = GPU_LOCK.lock().unwrap();
    let mut h = Pure2dHarness::new(128, 128, TextureFormat::Rgba8Unorm).expect("headless gpu");
    let id = h.renderer_mut().create_static_2d_batch(&static_test_shapes());

    for frame in 0..3 {
        h.begin_frame();
        let r = h.render_frame();
        assert_eq!(r.bytes_uploaded, 0, "frame {frame}: no upload");
        assert_eq!(r.instances_uploaded, 0, "frame {frame}: no instance upload");
        assert_eq!(r.buffer_reallocations, 0, "frame {frame}: no realloc");
        assert_eq!(r.static_batches_drawn, 1);
        assert_eq!(r.static_instances_drawn, 3);
    }

    h.renderer_mut().destroy_static_2d_batch(id);
}

/// Multiple batches each cost exactly one draw call and zero uploads, and the
/// `static_batches` config flag gates them for A/B comparison.
#[test]
fn multiple_static_batches_draw_one_call_each() {
    let _guard = GPU_LOCK.lock().unwrap();
    let mut h = Pure2dHarness::new(128, 128, TextureFormat::Rgba8Unorm).expect("headless gpu");

    let a = h.renderer_mut().create_static_2d_batch(&[ShapeInstance::rect(
        Vec2::new(-20.0, 0.0),
        Vec2::splat(20.0),
        RED,
    )]);
    let b = h.renderer_mut().create_static_2d_batch(&[ShapeInstance::rect(
        Vec2::new(20.0, 0.0),
        Vec2::splat(20.0),
        GREEN,
    )]);
    assert_eq!(h.renderer().static_2d_batch_count(), 2);

    h.begin_frame();
    let r = h.render_frame();
    assert_eq!(r.static_batches_drawn, 2);
    assert_eq!(r.shape_draw_calls, 2, "one draw per batch");
    assert_eq!(r.draw_calls, 2);
    assert_eq!(r.bytes_uploaded, 0);

    // Disabling the flag skips both without destroying them.
    h.renderer_mut().set_render_2d_config(Render2dConfig {
        static_batches: false,
        ..Default::default()
    });
    h.begin_frame();
    let gated = h.render_frame();
    assert_eq!(gated.draw_calls, 0);
    assert_eq!(h.renderer().static_2d_batch_count(), 2);

    h.renderer_mut().destroy_static_2d_batch(a);
    h.renderer_mut().destroy_static_2d_batch(b);
}

