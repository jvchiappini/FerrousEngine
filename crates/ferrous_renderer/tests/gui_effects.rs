//! Headless pixel tests for the real GUI effects: the rounded-clip SDF mask
//! and the two-pass backdrop blur.
//!
//! These tests need a GPU adapter; they skip with a printed note when the
//! headless context cannot be created (the same convention as
//! `tests/scene_target.rs`).

use std::sync::Mutex;
use std::time::Instant;

use ferrous_core::context::EngineContext;
use ferrous_renderer::headless::DEFAULT_FORMAT;
use ferrous_renderer::SceneTarget2d;
use ferrous_ui_render::{BackdropBlurSpec, GuiBatch, GuiRenderer, Rect};
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

fn render_once(
    gui: &mut GuiRenderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    target: &wgpu::TextureView,
    batch: &GuiBatch,
    clear: wgpu::Color,
) {
    gui.prepare(queue, batch);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("GUI effect test encoder"),
    });
    gui.render(
        &mut encoder,
        target,
        None,
        batch,
        wgpu::LoadOp::Clear(clear),
    );
    queue.submit(Some(encoder.finish()));
}

#[test]
fn rounded_clip_cuts_the_corner_and_keeps_the_centre() {
    let _guard = GPU_LOCK.lock().unwrap();
    let Some(context) = context() else {
        return;
    };
    let device = context.device.clone();
    let queue = context.queue.clone();
    let width = 96;
    let height = 96;

    let mut gui = GuiRenderer::new(device.clone(), DEFAULT_FORMAT, 256, width, height, 1, None);
    let target = SceneTarget2d::new(&device, width, height, TextureFormat::Rgba8UnormSrgb);

    let mut batch = GuiBatch::new();
    batch.push_clip_rounded(
        Rect {
            x: 8.0,
            y: 8.0,
            width: 80.0,
            height: 80.0,
        },
        [24.0; 4],
    );
    batch.rect(8.0, 8.0, 80.0, 80.0, [1.0, 0.0, 0.0, 1.0]);
    batch.pop_clip();
    batch.damage_union = None;

    render_once(
        &mut gui,
        &device,
        &queue,
        target.view(),
        &batch,
        wgpu::Color::TRANSPARENT,
    );

    // (10, 10) is inside the clip rectangle but outside the 24 px top-left
    // corner arc; (48, 48) is the centre.
    let corner = target
        .read_pixel_into(&device, &queue, 10, 10)
        .expect("corner readback");
    let centre = target
        .read_pixel_into(&device, &queue, 48, 48)
        .expect("centre readback");
    let outside = target
        .read_pixel_into(&device, &queue, 2, 2)
        .expect("outside readback");

    assert!(
        corner[3] < 8,
        "the rounded-corner cut-out must be transparent, got {corner:?}"
    );
    assert!(
        centre[0] > 220 && centre[3] > 220,
        "the centre of the clip must be painted, got {centre:?}"
    );
    assert!(
        outside[3] < 8,
        "outside the scissor must stay transparent, got {outside:?}"
    );
}

#[test]
fn backdrop_blur_samples_a_blurred_scene_not_a_tint() {
    let _guard = GPU_LOCK.lock().unwrap();
    let Some(context) = context() else {
        return;
    };
    let device = context.device.clone();
    let queue = context.queue.clone();
    let width = 128;
    let height = 128;

    // Scene: red on the left half, blue on the right half.
    let scene = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("backdrop test scene"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEFAULT_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let index = ((y * width + x) * 4) as usize;
            if x < width / 2 {
                pixels[index..index + 4].copy_from_slice(&[255, 0, 0, 255]);
            } else {
                pixels[index..index + 4].copy_from_slice(&[0, 0, 255, 255]);
            }
        }
    }
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &scene,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &pixels,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    let scene_view = scene.create_view(&wgpu::TextureViewDescriptor::default());

    let mut gui = GuiRenderer::new(device.clone(), DEFAULT_FORMAT, 256, width, height, 1, None);
    // A real caller passes the scene view (for example a `SceneTarget2d`).
    gui.set_backdrop_source(&scene_view, width, height);

    let target = SceneTarget2d::new(&device, width, height, TextureFormat::Rgba8UnormSrgb);
    let mut batch = GuiBatch::new();
    // Fully transparent tint: any colour the quad shows can only come from
    // the blurred backdrop, never from the tint fallback.
    batch.backdrop_blur(BackdropBlurSpec {
        rect: Rect {
            x: 32.0,
            y: 32.0,
            width: 64.0,
            height: 64.0,
        },
        radii: [0.0; 4],
        tint: [0.0, 0.0, 0.0, 0.0],
        blur_radius: 20.0,
    });
    batch.damage_union = None;

    render_once(
        &mut gui,
        &device,
        &queue,
        target.view(),
        &batch,
        wgpu::Color {
            r: 0.0,
            g: 1.0,
            b: 0.0,
            a: 1.0,
        },
    );

    // The scene boundary is at x = 64; a blur of radius 20 px mixes both
    // halves there, while a plain sample or the tint fallback would show one
    // half (or the green clear colour).
    let boundary = target
        .read_pixel_into(&device, &queue, 64, 64)
        .expect("boundary readback");
    let left = target
        .read_pixel_into(&device, &queue, 44, 64)
        .expect("left readback");
    let right = target
        .read_pixel_into(&device, &queue, 84, 64)
        .expect("right readback");

    assert!(
        boundary[0] > 30 && boundary[2] > 30,
        "the blur must mix red and blue at the boundary, got {boundary:?}"
    );
    assert!(
        boundary[1] < 100,
        "the green clear colour must be replaced by the blurred scene, got {boundary:?}"
    );
    assert!(
        left[0] > right[0] && right[2] > left[2],
        "the blur must preserve the left/right scene split, got left={left:?} right={right:?}"
    );

    // Measured frame cost for the quarter-resolution two-pass blur.
    let iterations = 8;
    let start = Instant::now();
    for _ in 0..iterations {
        render_once(
            &mut gui,
            &device,
            &queue,
            target.view(),
            &batch,
            wgpu::Color {
                r: 0.0,
                g: 1.0,
                b: 0.0,
                a: 1.0,
            },
        );
        device.poll(wgpu::Maintain::Wait);
    }
    let millis = start.elapsed().as_secs_f64() * 1000.0 / f64::from(iterations);
    println!(
        "backdrop blur: {width}x{height} scene -> {}x{} blur, {millis:.3} ms/frame",
        (width / 4).max(1),
        (height / 4).max(1)
    );
    assert!(
        millis < 50.0,
        "the bounded blur must stay well under a frame, measured {millis:.3} ms"
    );
}

#[test]
fn a_parsed_icon_path_is_filled_as_a_real_shape() {
    let _guard = GPU_LOCK.lock().unwrap();
    let Some(context) = context() else {
        return;
    };
    let device = context.device.clone();
    let queue = context.queue.clone();
    let width = 96;
    let height = 96;

    // A diamond. The old `ferrous_svg` placeholder was a triangle covering
    // x in [30, 70] at y = 50, so the pixel at (15, 50) proves the parsed path
    // data is what reached the GPU.
    let mesh =
        ferrous_svg::SvgMesh::from_str("M 10 50 L 50 10 L 90 50 L 50 90 Z").expect("diamond path");
    assert!(!mesh.indices.is_empty());

    let mut gui = GuiRenderer::new(device.clone(), DEFAULT_FORMAT, 256, width, height, 1, None);
    let target = SceneTarget2d::new(&device, width, height, TextureFormat::Rgba8UnormSrgb);
    let mut batch = GuiBatch::new();
    batch.push_svg(ferrous_ui_render::SvgCommand {
        mesh,
        pos: [0.0, 0.0],
        color: [0.0, 1.0, 0.0, 1.0],
        z: 0.0,
    });
    batch.damage_union = None;

    render_once(
        &mut gui,
        &device,
        &queue,
        target.view(),
        &batch,
        wgpu::Color::TRANSPARENT,
    );

    let left_tip = target
        .read_pixel_into(&device, &queue, 15, 50)
        .expect("left tip readback");
    let centre = target
        .read_pixel_into(&device, &queue, 48, 52)
        .expect("centre readback");
    let outside = target
        .read_pixel_into(&device, &queue, 5, 5)
        .expect("outside readback");

    assert!(
        left_tip[1] > 200 && left_tip[3] > 200,
        "the diamond tip must be filled (the placeholder triangle missed it), got {left_tip:?}"
    );
    assert!(
        centre[1] > 200 && centre[3] > 200,
        "the diamond centre must be filled, got {centre:?}"
    );
    assert!(
        outside[3] < 8,
        "outside the icon must stay transparent, got {outside:?}"
    );
}
