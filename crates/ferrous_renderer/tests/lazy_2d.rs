//! The internal render target is lazy in the 2-D modes.
//!
//! A `Flat2D`/`Pure2D` renderer presents straight to the surface, so its
//! swapchain-sized colour + depth pair used to cost 11.52 MB at 1600x900 for
//! nothing.  These tests pin the new contract: no allocation at construction,
//! materialization on the first render into the target, and the same memory
//! inventory labels once it exists.
//!
//! Every test skips cleanly when no GPU adapter is available.

use std::sync::Mutex;

use ferrous_core::context::EngineContext;
use ferrous_renderer::{Renderer, RendererMode};

static GPU_LOCK: Mutex<()> = Mutex::new(());

const WIDTH: u32 = 320;
const HEIGHT: u32 = 200;

fn renderer(mode: RendererMode) -> Option<Renderer> {
    let context = match pollster::block_on(EngineContext::new()) {
        Ok(context) => context,
        Err(error) => {
            println!("[skip] headless GPU unavailable: {error:?}");
            return None;
        }
    };
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

fn has_label(_renderer: &Renderer, label: &str) -> bool {
    ferrous_core::memory::report(usize::MAX)
        .largest
        .iter()
        .any(|entry| entry.label == label)
}

fn render_internal(mut renderer: Renderer) {
    let mut encoder = renderer.begin_frame();
    renderer.render_to_internal_target(&mut encoder, None);
    renderer.context.queue.submit(Some(encoder.finish()));
}

#[test]
fn a_pure_2d_target_is_unallocated_until_a_target_render() {
    let _guard = GPU_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(mut renderer) = renderer(RendererMode::Pure2D) else {
        return;
    };
    ferrous_core::memory::clear();

    assert!(renderer.render_target.is_lazy());
    assert!(
        !renderer.render_target.is_allocated(),
        "Pure2D construction must not allocate the colour + depth pair"
    );
    for label in ["rt:color resolve", "rt:color msaa", "rt:depth"] {
        assert!(
            !has_label(&renderer, label),
            "{label} must not be in the memory inventory"
        );
    }

    let mut encoder = renderer.begin_frame();
    renderer.render_to_internal_target(&mut encoder, None);
    assert!(
        renderer.render_target.is_allocated(),
        "the first Target render must materialize the pair"
    );
    for label in ["rt:color resolve", "rt:depth"] {
        assert!(
            has_label(&renderer, label),
            "{label} must appear once the target is allocated"
        );
    }
    assert!(!has_label(&renderer, "rt:color msaa"));
    renderer.context.queue.submit(Some(encoder.finish()));
}

#[test]
fn a_flat_2d_target_is_unallocated_until_a_target_render() {
    let _guard = GPU_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(renderer) = renderer(RendererMode::Flat2D) else {
        return;
    };
    ferrous_core::memory::clear();

    assert!(!renderer.render_target.is_allocated());
    assert!(!has_label(&renderer, "rt:color resolve"));
    render_internal(renderer);
}

#[test]
fn releasing_and_resizing_the_lazy_target_still_work() {
    let _guard = GPU_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(mut renderer) = renderer(RendererMode::Pure2D) else {
        return;
    };

    let mut encoder = renderer.begin_frame();
    renderer.render_to_internal_target(&mut encoder, None);
    renderer.context.queue.submit(Some(encoder.finish()));
    assert!(renderer.render_target.is_allocated());

    renderer.render_target.release();
    assert!(!renderer.render_target.is_allocated());

    // A logical resize while unallocated only records the new size; the next
    // render materializes at that size.
    renderer.resize(WIDTH * 2, HEIGHT * 2);
    assert!(!renderer.render_target.is_allocated());
    assert_eq!(renderer.viewport_size().x, (WIDTH * 2) as f32);

    let mut encoder = renderer.begin_frame();
    renderer.render_to_internal_target(&mut encoder, None);
    renderer.context.queue.submit(Some(encoder.finish()));
    assert!(renderer.render_target.is_allocated());
}
