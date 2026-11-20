//! Reports the GUI pass state-bind counters for a synthetic N-panel UI.
//!
//! Prompt 18 P5 hoisted the invariant GUI render state (uniform bind group,
//! shared quad vertex/index buffer, text/icon pipeline reuse) and added
//! `UiFrameCounters`.  This example builds `N` scissored panels, each with one
//! quad, one text run and one icon run, renders the batch through the real
//! [`GuiRenderer`], and prints the counters of that pass.
//!
//! Before P5 every primitive range rebound the pipeline, both bind groups and
//! its vertex/index buffers, so `pipeline_binds == draw_calls`.  With the
//! cache the draw and segment counts are unchanged but the bind counts drop.
//!
//! ```text
//! cargo run -p ferrous_renderer --example draw_state
//! ```
//!
//! The per-range before/after arithmetic for one full segment
//! (`quad -> text -> icon`, quad state first) is:
//!
//! ```text
//!             pipeline  bind groups  buffers   total binds
//! before            3            6        9            18
//! after             2            3        5            10   (+1 group 0 per pass)
//! ```
//!
//! Text reuses the quad vertex/index buffer, icons reuse the text pipeline and
//! the quad buffers, and bind group 0 is bound once for the whole pass.
//!
//! The example uses a 1x1 dummy atlas for text and icons: it reports state
//! binds, not glyph coverage, and the real font is not needed to exercise the
//! pipelines.

#[cfg(feature = "gui")]
use ferrous_core::context::EngineContext;
#[cfg(feature = "gui")]
use ferrous_ui_core::Rect;
#[cfg(feature = "gui")]
use ferrous_ui_render::{GuiBatch, GuiRenderer, TextQuad};

#[cfg(feature = "gui")]
const WIDTH: u32 = 1280;
#[cfg(feature = "gui")]
const HEIGHT: u32 = 800;
#[cfg(feature = "gui")]
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;
#[cfg(feature = "gui")]
const PANELS: u32 = 12;

#[cfg(feature = "gui")]
fn context() -> Option<EngineContext> {
    let backends = wgpu::util::backend_bits_from_env().unwrap_or(wgpu::Backends::all());
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends,
        ..Default::default()
    });
    pollster::block_on(EngineContext::new_with_instance(instance, None)).ok()
}

#[cfg(feature = "gui")]
fn glyph(pos: [f32; 2], size: [f32; 2], node_id: u32) -> TextQuad {
    TextQuad {
        pos,
        size,
        uv0: [0.0, 0.0],
        uv1: [1.0, 1.0],
        color: [0.9, 0.9, 0.95, 1.0],
        z_order: 0.5,
        node_id,
    }
}

/// One scissored panel with a quad, a text run and an icon run.
#[cfg(feature = "gui")]
fn synthetic_ui(panels: u32) -> GuiBatch {
    let mut batch = GuiBatch::new();
    for i in 0..panels {
        let column = i % 4;
        let row = i / 4;
        let x = 24.0 + column as f32 * 300.0;
        let y = 24.0 + row as f32 * 220.0;
        batch.push_clip(Rect {
            x,
            y,
            width: 280.0,
            height: 200.0,
        });
        batch.rect(x, y, 280.0, 200.0, [0.12, 0.12, 0.14, 1.0]);
        batch.push_text(glyph([x + 16.0, y + 16.0], [8.0, 8.0], i));
        batch.push_icon(glyph([x + 240.0, y + 16.0], [16.0, 16.0], i));
        batch.pop_clip();
    }
    batch
}

#[cfg(feature = "gui")]
fn dummy_atlas(device: &wgpu::Device) -> (wgpu::TextureView, wgpu::Sampler) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("draw_state dummy atlas"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("draw_state dummy atlas sampler"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    (view, sampler)
}

#[cfg(feature = "gui")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Some(context) = context() else {
        eprintln!("no GPU adapter available");
        return Ok(());
    };

    let batch = synthetic_ui(PANELS);
    let mut renderer = GuiRenderer::new(
        context.device.clone(),
        FORMAT,
        PANELS * 4,
        WIDTH,
        HEIGHT,
        1,
        context.pipeline_cache.clone(),
    );

    let (atlas_view, atlas_sampler) = dummy_atlas(&context.device);
    renderer.set_font_atlas(&atlas_view, &atlas_sampler);
    renderer.set_icon_atlas(&atlas_view, &atlas_sampler);
    renderer.prepare(&context.queue, &batch);

    let target = context.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("draw_state target"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder = context
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("draw_state encoder"),
        });
    renderer.render(
        &mut encoder,
        &view,
        None,
        &batch,
        wgpu::LoadOp::Clear(wgpu::Color::BLACK),
    );
    context.queue.submit(Some(encoder.finish()));

    let counters = renderer.frame_counters();
    println!("adapter          : {}", context.adapter.get_info().name);
    println!("panels           : {PANELS}");
    println!(
        "primitives       : {} quads, {} text, {} icons",
        PANELS, PANELS, PANELS
    );
    println!("segments         : {}", counters.segments);
    println!("scissor changes  : {}", counters.scissor_changes);
    println!("draw calls       : {}", counters.draw_calls);
    println!("pipeline binds   : {}", counters.pipeline_binds);
    println!("bind group binds : {}", counters.bind_group_binds);
    println!("buffer binds     : {}", counters.buffer_binds);
    Ok(())
}

#[cfg(not(feature = "gui"))]
fn main() {
    eprintln!("the `draw_state` example requires the `gui` feature");
}
