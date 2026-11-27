//! The GUI builds and draws with the separate-binding image path.
//!
//! `GuiRenderer::new` selects its quad shader and image bind group from the
//! features the **device** enabled.  Asking for a device with no optional
//! features (as GL/WebGL2 does) must therefore take `gui_web.wgsl` and one
//! texture + sampler binding per slot, and still record a real draw.
//!
//! This runs on any adapter (Vulkan included), so it pins the fallback without
//! needing GL hardware; it skips cleanly when no adapter exists.

use std::sync::{Arc, Mutex};

use ferrous_ui_render::{GuiBatch, GuiRenderer, ImageBindingMode};

static GPU_LOCK: Mutex<()> = Mutex::new(());

const SIZE: u32 = 64;

fn bare_device() -> Option<(Arc<wgpu::Device>, Arc<wgpu::Queue>)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))?;
    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("No-bindless GUI test device"),
            required_features: wgpu::Features::empty(),
            required_limits: adapter.limits(),
            memory_hints: wgpu::MemoryHints::Performance,
        },
        None,
    ))
    .ok()?;
    Some((Arc::new(device), Arc::new(queue)))
}

#[test]
fn the_gui_builds_and_draws_without_texture_binding_arrays() {
    let _guard = GPU_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some((device, queue)) = bare_device() else {
        println!("[skip] headless GPU unavailable");
        return;
    };

    // The device was created without the descriptor-array features, so the
    // renderer must choose the separate-binding shader/layout.
    assert_eq!(
        ImageBindingMode::for_features(device.features()),
        ImageBindingMode::Separate
    );

    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let mut gui = GuiRenderer::new(device.clone(), format, 64, SIZE, SIZE, 1, None);
    assert_eq!(
        gui.image_binding_mode(),
        ImageBindingMode::Separate,
        "a device without TEXTURE_BINDING_ARRAY must take the separate path"
    );

    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("No-bindless GUI target"),
        size: wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());

    let mut batch = GuiBatch::new();
    batch.rect(
        8.0,
        8.0,
        (SIZE - 16) as f32,
        (SIZE - 16) as f32,
        [1.0, 0.0, 0.0, 1.0],
    );

    gui.prepare(&queue, &batch);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("No-bindless GUI encoder"),
    });
    gui.render(
        &mut encoder,
        &view,
        None,
        &batch,
        wgpu::LoadOp::Clear(wgpu::Color::BLACK),
    );
    queue.submit(Some(encoder.finish()));
    device.poll(wgpu::Maintain::Wait);

    assert_eq!(
        gui.frame_counters().draw_calls,
        1,
        "the separate path must record the quad"
    );
}
