//! Off-screen 2D scene target: render, resize and read back.

use std::sync::Mutex;

use ferrous_2d::render::ShapeInstance;
use ferrous_core::context::EngineContext;
use ferrous_renderer::headless::DEFAULT_FORMAT;
use ferrous_renderer::{Renderer, RendererMode, SceneTarget2d};
use glam::Vec2;
use wgpu::TextureFormat;

static GPU_LOCK: Mutex<()> = Mutex::new(());

fn context() -> Option<EngineContext> {
    match pollster::block_on(EngineContext::new()) {
        Ok(context) => Some(context),
        Err(error) => {
            println!("[skip] headless GPU unavailable: {error}");
            None
        }
    }
}

#[test]
fn scene_target_renders_and_reads_back() {
    let _guard = GPU_LOCK.lock().unwrap();
    let Some(context) = context() else {
        return;
    };
    let device = context.device.clone();
    let queue = context.queue.clone();
    let mut renderer = Renderer::new_with_mode(
        context,
        64,
        64,
        TextureFormat::Rgba8Unorm,
        1,
        None,
        RendererMode::Pure2D,
    );
    renderer.enable_pure_2d(wgpu::Color::BLACK);

    let mut target = SceneTarget2d::new(&device, 64, 64, TextureFormat::Rgba8Unorm);
    target.render_with(&mut renderer, &device, &queue, |renderer| {
        renderer.draw_2d_shape(ShapeInstance::rect(
            Vec2::ZERO,
            Vec2::splat(32.0),
            [1.0, 0.0, 0.0, 1.0],
        ));
    });

    let center = target
        .read_pixel_into(&device, &queue, 32, 32)
        .expect("center readback");
    let corner = target
        .read_pixel_into(&device, &queue, 1, 1)
        .expect("corner readback");
    assert!(
        center[0] > 240 && center[1] < 10 && center[2] < 10,
        "center {center:?}"
    );
    assert!(
        corner[0] < 10 && corner[1] < 10 && corner[2] < 10,
        "corner {corner:?}"
    );

    let mut pixels = Vec::new();
    target
        .read_rgba8_into(&device, &queue, &mut pixels)
        .expect("frame readback");
    assert_eq!(pixels.len(), 64 * 64 * 4);
    assert_eq!(target.width(), 64);
    assert_eq!(target.height(), 64);
    assert_eq!(target.format(), TextureFormat::Rgba8Unorm);
}

#[test]
fn scene_target_resize_and_default_format() {
    let _guard = GPU_LOCK.lock().unwrap();
    let Some(context) = context() else {
        return;
    };
    let device = context.device.clone();
    let queue = context.queue.clone();
    let mut renderer = Renderer::new_with_mode(
        context,
        64,
        64,
        DEFAULT_FORMAT,
        1,
        None,
        RendererMode::Pure2D,
    );
    renderer.enable_pure_2d(wgpu::Color::BLACK);

    let mut target = SceneTarget2d::new(&device, 64, 48, SceneTarget2d::DEFAULT_FORMAT);
    assert_eq!(target.width(), 64);
    assert_eq!(target.height(), 48);
    target.resize(&device, 96, 64);
    assert_eq!(target.width(), 96);
    assert_eq!(target.height(), 64);

    target.render_with(&mut renderer, &device, &queue, |renderer| {
        renderer.draw_2d_shape(ShapeInstance::rect(
            Vec2::ZERO,
            Vec2::splat(20.0),
            [0.0, 1.0, 0.0, 1.0],
        ));
    });
    let center = target
        .read_pixel_into(&device, &queue, 48, 32)
        .expect("center readback");
    assert!(center[1] > 200, "center {center:?}");
}
