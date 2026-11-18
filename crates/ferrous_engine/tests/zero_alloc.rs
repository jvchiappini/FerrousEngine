//! Zero-allocation frame tests (prompt 18 P2).
//!
//! Run with the counting allocator:
//!
//! ```text
//! cargo test -p ferrous_engine --features alloc-stats --test zero_alloc
//! ```
//!
//! The scope is **thread-local**: a worker that allocates while the frame is
//! open does not make the frame fail (the frame trace reports those bytes as
//! worker allocations).

#![cfg(feature = "alloc-stats")]

use ferrous_core::alloc_stats::{reset, ThreadScope};
use ferrous_ui_core::{DrawContext, Rect, RenderCommand, Widget};

struct Solid;

impl Widget<()> for Solid {
    fn draw(&self, ctx: &mut DrawContext, cmds: &mut Vec<RenderCommand>) {
        cmds.push(RenderCommand::Quad {
            rect: ctx.rect,
            color: [0.2, 0.3, 0.4, 1.0],
            radii: [0.0; 4],
            flags: 0,
        });
    }
}

#[test]
fn an_engine_ui_frame_allocates_nothing_after_warmup() {
    let mut ui: ferrous_gui::UiSystem<()> = ferrous_gui::UiSystem::new();
    ui.add(Solid);
    let viewport = Rect {
        x: 0.0,
        y: 0.0,
        width: 800.0,
        height: 600.0,
    };
    let mut batch = ferrous_ui_render::GuiBatch::new();

    for _ in 0..10 {
        ui.update(1.0 / 60.0, 800.0, 600.0);
        batch.clear();
        ui.render_into(&mut batch, viewport, None);
    }

    reset();
    let scope = ThreadScope::new();
    ui.update(1.0 / 60.0, 800.0, 600.0);
    batch.clear();
    ui.render_into(&mut batch, viewport, None);
    let delta = scope.finish();
    assert_eq!(delta.allocating_calls(), 0, "{delta:?}");
}

#[test]
fn a_pure2d_frame_allocates_nothing_after_warmup() {
    use ferrous_2d::render::ShapeInstance;
    use ferrous_core::glam::Vec2;
    use ferrous_renderer::headless::{Pure2dHarness, DEFAULT_FORMAT};

    let Ok(mut harness) = Pure2dHarness::new(64, 64, DEFAULT_FORMAT) else {
        // No adapter in this environment: the trace tests cover the CPU path.
        return;
    };
    let shape = ShapeInstance::rect(Vec2::ZERO, Vec2::splat(16.0), [0.5, 0.2, 0.8, 1.0]);

    for _ in 0..10 {
        harness.begin_frame();
        harness.draw_shape(shape);
        harness.render_frame();
    }

    reset();
    let scope = ThreadScope::new();
    harness.begin_frame();
    harness.draw_shape(shape);
    harness.render_frame();
    let delta = scope.finish();
    assert_eq!(delta.allocating_calls(), 0, "{delta:?}");
}
