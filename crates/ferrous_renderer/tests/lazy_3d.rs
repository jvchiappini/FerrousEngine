//! Prompt 17 S4: 3-D resources are lazy in `Flat2D`.
//!
//! A renderer built in `RendererMode::Flat2D` must not allocate the world pass
//! (instancing pipelines, shadow map, HDR target), the material registry, the
//! SSAO + prepass stack, the skinning/gizmo pipelines or the million-particle
//! system. They are built on the first 3-D use (a mode switch to `Full3D`
//! included), and switching back keeps rendering.
//!
//! Every test skips cleanly when no GPU adapter is available.

use std::sync::Mutex;

use ferrous_core::context::EngineContext;
use ferrous_core::scene::World;
use ferrous_renderer::{Renderer, RendererMode};

/// wgpu devices are cheap but tests in one binary share a process; serialise
/// the GPU-heavy ones like the other headless suites do.
static GPU_LOCK: Mutex<()> = Mutex::new(());

fn context() -> Option<EngineContext> {
    match pollster::block_on(EngineContext::new()) {
        Ok(context) => Some(context),
        Err(error) => {
            println!("[skip] headless GPU unavailable: {error:?}");
            None
        }
    }
}

fn flat_2d_renderer() -> Option<Renderer> {
    let context = context()?;
    Some(Renderer::new_with_mode(
        context,
        320,
        200,
        wgpu::TextureFormat::Bgra8UnormSrgb,
        1,
        None,
        RendererMode::Flat2D,
    ))
}

/// Submits one frame through whatever path the renderer's current mode uses.
fn render_one_frame(renderer: &mut Renderer) {
    let mut encoder = renderer.begin_frame();
    renderer.render_to_internal_target(&mut encoder, None);
    renderer.context.queue.submit(Some(encoder.finish()));
}

#[test]
fn flat_2d_allocates_no_3d_resources() {
    let _guard = GPU_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(renderer) = flat_2d_renderer() else {
        return;
    };

    assert_eq!(renderer.mode, RendererMode::Flat2D);
    assert!(
        !renderer.three_d_ready(),
        "the world pass, shadow map, SSAO/prepass, skinning, gizmos, post-process and AA \
         must not exist in Flat2D"
    );
    assert!(
        !renderer.material_registry_ready(),
        "the material registry GPU parts must not exist in Flat2D"
    );
    assert!(
        renderer.particle_system.is_none(),
        "the 1 000 000-particle buffer must not exist in Flat2D"
    );
}

#[test]
fn the_first_3d_use_builds_everything_and_the_modes_cross_over() {
    let _guard = GPU_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(mut renderer) = flat_2d_renderer() else {
        return;
    };
    assert!(!renderer.three_d_ready());
    assert!(renderer.particle_system.is_none());

    // Runtime switch to 3-D: this is the first 3-D use.
    renderer.enable_full_3d();
    assert_eq!(renderer.mode, RendererMode::Full3D);
    assert!(renderer.three_d_ready());
    assert!(renderer.material_registry_ready());
    assert!(renderer.particle_system.is_some());

    // The first real 3-D draw runs on the freshly built group.
    renderer.sync_world(&World::default());
    render_one_frame(&mut renderer);

    // And the renderer goes back to the 2-D fast path without panicking.
    renderer.enable_flat_2d(wgpu::Color::BLACK);
    assert_eq!(renderer.mode, RendererMode::Flat2D);
    render_one_frame(&mut renderer);
}

#[test]
fn a_full_3d_renderer_still_builds_its_group_at_construction() {
    let _guard = GPU_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(context) = context() else {
        return;
    };
    let renderer = Renderer::new_with_mode(
        context,
        320,
        200,
        wgpu::TextureFormat::Bgra8UnormSrgb,
        1,
        None,
        RendererMode::Full3D,
    );
    assert_eq!(renderer.mode, RendererMode::Full3D);
    assert!(renderer.three_d_ready());
    assert!(renderer.material_registry_ready());
    assert!(renderer.particle_system.is_some());
}

/// The 1 MiB stack guard for the Motioner pin.
///
/// `motioner preview` runs on the Windows main thread, whose debug stack is
/// 1 MiB. Building the `Full3D` renderer and running its first frame used to
/// overflow it after the Phase 9D state grew (the `WorldBuilder`, the lazy
/// slots and the enlarged `ensure_three_d` frame). This test creates the GPU
/// context on the test thread, then performs the first `Full3D` frame on a
/// thread with exactly 1 MiB; raising the stack is not an acceptable fix, so
/// the engine must keep the frame small.
#[test]
fn a_full_3d_first_frame_fits_in_a_one_megabyte_stack() {
    let _guard = GPU_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(context) = context() else {
        return;
    };
    let worker = std::thread::Builder::new()
        .name("1mib-full3d".to_owned())
        .stack_size(1024 * 1024)
        .spawn(move || {
            let mut renderer = Renderer::new_with_mode(
                context,
                320,
                200,
                wgpu::TextureFormat::Bgra8UnormSrgb,
                1,
                None,
                RendererMode::Full3D,
            );
            renderer.sync_world(&World::default());
            render_one_frame(&mut renderer);
        })
        .expect("spawn the 1 MiB worker");
    worker
        .join()
        .expect("the Full3D first frame must fit in a 1 MiB stack");
}
