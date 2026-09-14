//! Regression test for the sprite-texture churn leak (audit item H4).
//!
//! `free_sprite_texture` used to remove only the bind group, leaving the freed
//! id's bucket behind in `SpriteBatcher` (measured: 2000 orphan keys after
//! 2000 register/free cycles) and never recycling ids (monotonic counter).
//! This test drives the public `Renderer` API through the headless harness.
//!
//! Run with:
//! ```text
//! export CARGO_TARGET_DIR=<shared target dir>
//! export VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json
//! export VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json
//! export WGPU_BACKEND=vulkan
//! cargo test -p ferrous_renderer --test sprite_texture_recycle -- --test-threads=1
//! ```

use ferrous_2d::render::SpriteInstance;
use ferrous_renderer::headless::{Pure2dHarness, DEFAULT_FORMAT};
use glam::{Mat4, Quat, Vec3};

fn sprite() -> SpriteInstance {
    let model = Mat4::from_scale_rotation_translation(
        Vec3::new(8.0, 8.0, 1.0),
        Quat::IDENTITY,
        Vec3::new(0.0, 0.0, 0.0),
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

#[test]
fn freed_sprite_texture_is_recycled_and_batcher_cleaned() {
    let mut h = Pure2dHarness::new(64, 64, DEFAULT_FORMAT).expect("headless gpu");
    let t0 = h.register_texture_rgba8(2, 2, &[255u8; 16]);
    assert_eq!(t0, 0);

    h.begin_frame();
    h.draw_sprite(t0, sprite());
    let report = h.render_frame();
    assert_eq!(report.sprite_draw_calls, 1);

    // `render_frame` clears instances but intentionally keeps the live bucket.
    assert!(h.renderer().sprite_batcher_2d.batches.contains_key(&t0));
    assert_eq!(h.renderer().sprite_bind_groups.len(), 1);

    h.renderer_mut().free_sprite_texture(t0);

    assert!(
        h.renderer().sprite_bind_groups.is_empty(),
        "bind group must be freed"
    );
    assert!(
        !h.renderer().sprite_batcher_2d.batches.contains_key(&t0),
        "the freed texture's batcher key must be dropped"
    );
    assert!(
        h.renderer().freed_sprite_texture_ids.contains(&t0),
        "the freed id must be recyclable"
    );
    assert_eq!(
        h.renderer().next_sprite_texture_id,
        1,
        "freeing must not allocate a new id"
    );

    let t1 = h.register_texture_rgba8(2, 2, &[0u8; 16]);
    assert_eq!(t1, t0, "a recycled id must be handed out again");
    assert_eq!(
        h.renderer().next_sprite_texture_id,
        1,
        "the high-water mark must not grow on reuse"
    );
    assert_eq!(h.renderer().sprite_bind_groups.len(), 1);
    assert!(
        h.renderer().freed_sprite_texture_ids.is_empty(),
        "the free-list must be drained by the next registration"
    );
}

/// Mirror of the Phase-1 measurement: 2000 register/free cycles used to leave
/// 2000 keys in `SpriteBatcher.batches` and grow `next_sprite_texture_id` to
/// 2000. Both must now stay bounded.
#[test]
fn many_register_free_cycles_leave_no_orphans() {
    let mut h = Pure2dHarness::new(16, 16, DEFAULT_FORMAT).expect("headless gpu");

    const CYCLES: u32 = 2000;
    for _ in 0..CYCLES {
        let id = h.register_texture_rgba8(2, 2, &[10u8; 16]);
        h.renderer_mut().free_sprite_texture(id);
    }

    assert_eq!(
        h.renderer().next_sprite_texture_id,
        1,
        "ids must be recycled instead of monotonic (was {CYCLES})"
    );
    assert!(h.renderer().sprite_bind_groups.is_empty());
    assert!(
        h.renderer().sprite_batcher_2d.batches.is_empty(),
        "no orphan batcher keys may survive (was {CYCLES})"
    );
    assert!(h.renderer().freed_sprite_texture_ids.len() <= 1);
}
