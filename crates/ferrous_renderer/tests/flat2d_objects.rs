//! Prompt 15 Phase 9D Lane A: a `Flat2D` renderer owns only what it draws.
//!
//! A GUI-only renderer must not build either [`Renderer2d`] pipeline set (the
//! HDR overlay one or the Pure2D swapchain one) and must not allocate any 3-D
//! object.  The proof is the memory inventory: after `ferrous_core::memory`
//! is cleared, constructing a `Flat2D` renderer registers no `2d:`, `3d:` or
//! `rt:` entry.  Pure2D builds only the swapchain set on its first frame and
//! Full3D builds only the HDR overlay set.
//!
//! Every test skips cleanly when no GPU adapter is available.
//!
//! [`Renderer2d`]: ferrous_2d::render::Renderer2d

use std::sync::Mutex;

use ferrous_2d::render::ShapeInstance;
use ferrous_core::context::EngineContext;
use ferrous_renderer::{Renderer, RendererMode};
use glam::Vec2;

/// One GPU device at a time: these tests share the process-global memory
/// inventory, so a concurrent constructor would pollute the snapshots.
static GPU_LOCK: Mutex<()> = Mutex::new(());

const WIDTH: u32 = 320;
const HEIGHT: u32 = 200;

fn renderer(mode: RendererMode) -> Option<Renderer> {
    let context = match pollster::block_on(EngineContext::new()) {
        Ok(context) => Some(context),
        Err(error) => {
            println!("[skip] headless GPU unavailable: {error:?}");
            None
        }
    }?;
    Some(Renderer::new_with_mode(
        context,
        WIDTH,
        HEIGHT,
        wgpu::TextureFormat::Bgra8UnormSrgb,
        1,
        None,
        mode,
    ))
}

/// Every label currently registered in the global memory inventory.
fn labels(renderer: &Renderer) -> Vec<String> {
    renderer
        .memory_entries()
        .into_iter()
        .map(|entry| entry.label)
        .collect()
}

fn assert_no_object_labels(renderer: &Renderer, what: &str) {
    for label in labels(renderer) {
        assert!(
            !label.starts_with("3d:"),
            "{what}: a 3-D resource is resident ({label})"
        );
        assert!(
            !label.starts_with("2d:"),
            "{what}: a lazily built 2-D object is resident ({label})"
        );
        assert!(
            !label.starts_with("rt:"),
            "{what}: the internal colour+depth target is resident ({label})"
        );
    }
}

#[test]
fn flat_2d_construction_registers_no_2d_or_3d_object() {
    let _guard = GPU_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    // Clear first so the snapshot only describes this constructor.
    ferrous_core::memory::clear();
    let Some(renderer) = renderer(RendererMode::Flat2D) else {
        return;
    };

    assert_eq!(renderer.mode, RendererMode::Flat2D);
    assert!(!renderer.three_d_ready());
    assert!(!renderer.material_registry_ready());
    assert!(renderer.particle_system.is_none());
    assert!(
        !renderer.renderer_2d_ready(),
        "Flat2D must not build the HDR overlay 2-D pipeline set"
    );
    assert!(
        !renderer.renderer_2d_pure_ready(),
        "Flat2D must not build the Pure2D swapchain 2-D pipeline set"
    );
    assert!(
        !renderer.camera_ready(),
        "a UI-only Flat2D app must not allocate the 3-D camera uniform"
    );
    assert!(
        !renderer.pipeline_layouts_ready(),
        "a UI-only Flat2D app must not build the 3-D PipelineLayouts"
    );
    // The GUI pass legitimately registers `gui:*` buffers; nothing 2-D/3-D.
    assert_no_object_labels(&renderer, "Flat2D construction");
}

#[test]
fn a_flat_2d_frame_adds_no_2d_or_3d_object() {
    let _guard = GPU_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(mut renderer) = renderer(RendererMode::Flat2D) else {
        return;
    };

    // Present to an external view (the swapchain stand-in): a Flat2D frame
    // must not need the internal colour+depth target either.
    let target = ferrous_renderer::headless::OffscreenTarget::new(
        &renderer.context.device,
        WIDTH,
        HEIGHT,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    );

    ferrous_core::memory::clear();
    let mut encoder = renderer.begin_frame();
    renderer.render_to_view(&mut encoder, &target.view, None);
    renderer.context.queue.submit(Some(encoder.finish()));

    assert!(
        !renderer.renderer_2d_ready(),
        "a Flat2D frame must not build the HDR overlay set"
    );
    assert!(
        !renderer.renderer_2d_pure_ready(),
        "a Flat2D frame must not build the Pure2D set"
    );
    assert!(!renderer.three_d_ready());
    assert!(
        !renderer.camera_ready(),
        "a Flat2D frame must not build the camera"
    );
    assert!(
        !renderer.pipeline_layouts_ready(),
        "a Flat2D frame must not build the 3-D layouts"
    );
    assert_no_object_labels(&renderer, "Flat2D frame");
}

#[test]
fn pure_2d_builds_only_its_swapchain_set() {
    let _guard = GPU_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(mut renderer) = renderer(RendererMode::Pure2D) else {
        return;
    };
    // Orthographic pixel camera: the shape below must be visible (not culled),
    // otherwise no 2-D pipeline bank would be touched by the frame.
    renderer.enable_pure_2d(wgpu::Color::BLACK);

    assert!(!renderer.renderer_2d_pure_ready());
    assert!(!renderer.renderer_2d_ready());

    // One shape: only the shape bank may compile, never sprite/text/mesh/grid.
    renderer.draw_2d_shape(ShapeInstance::rect(
        Vec2::ZERO,
        Vec2::splat(32.0),
        [1.0, 0.0, 0.0, 1.0],
    ));
    let mut encoder = renderer.begin_frame();
    renderer.render_to_internal_target(&mut encoder, None);
    renderer.context.queue.submit(Some(encoder.finish()));

    assert!(
        renderer.renderer_2d_pure_ready(),
        "the first Pure2D frame builds the swapchain renderer"
    );
    let pure = renderer
        .renderer_2d_pure
        .renderer()
        .expect("the first frame materialized the Pure2D renderer");
    assert_eq!(
        pure.built_pipeline_banks(),
        1,
        "drawing one shape must build only the shape pipeline bank"
    );
    assert!(
        !renderer.renderer_2d_ready(),
        "Pure2D must not build the HDR overlay set"
    );
    assert!(!renderer.three_d_ready());
}

#[test]
fn full_3d_builds_the_overlay_set_not_the_pure_one() {
    let _guard = GPU_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(renderer) = renderer(RendererMode::Full3D) else {
        return;
    };

    assert!(renderer.three_d_ready());
    assert!(
        renderer.renderer_2d_ready(),
        "the 3-D group owns the HDR overlay 2-D set"
    );
    assert!(
        !renderer.renderer_2d_pure_ready(),
        "the Pure2D set is built only when Pure2D is first rendered"
    );
}
