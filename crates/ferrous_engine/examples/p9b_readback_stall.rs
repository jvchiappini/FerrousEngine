//! Phase 9B: the popup readback stall behind a heavy main-window queue.
//!
//! A layered popup must read its offscreen frame back on the CPU. The old path
//! called `device.poll(Maintain::Wait)` on the event-loop thread, so a menu
//! repaint waited for whatever the main window had queued. This example queues a
//! heavy GUI batch first, then measures:
//!
//! * `sync_block_ms`: `map_async` + `Maintain::Wait` on this thread (the old
//!   path);
//! * `async_submit_ms`: `map_async` + `Maintain::Poll` (the new path returns at
//!   once; the worker thread does the wait);
//! * `async_total_ms`: when the worker-thread copy is readable.
//!
//! Run with `cargo run --release -p ferrous_engine --example p9b_readback_stall`.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Instant;

use ferrous_renderer::headless::{Pure2dHarness, DEFAULT_FORMAT};
use ferrous_ui_render::{GuiBatch, GuiRenderer};
use wgpu::{
    BufferUsages, CommandEncoderDescriptor, ImageCopyBuffer, ImageCopyTexture, ImageDataLayout,
};

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;
const HEAVY_QUADS: usize = 600_000;

fn padded_bytes_per_row(width: u32) -> u32 {
    (width.max(1) * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT
}

fn heavy_batch() -> GuiBatch {
    let mut batch = GuiBatch::new();
    for index in 0..HEAVY_QUADS {
        let x = (index % WIDTH as usize) as f32;
        let y = ((index / WIDTH as usize) % HEIGHT as usize) as f32;
        batch.rect(x, y, 6.0, 6.0, [0.1, 0.2, 0.3, 0.35]);
    }
    batch
}

fn popup_batch() -> GuiBatch {
    let mut batch = GuiBatch::new();
    batch.rect(16.0, 16.0, 240.0, 140.0, [0.15, 0.16, 0.2, 0.96]);
    batch.rect(28.0, 28.0, 180.0, 18.0, [0.8, 0.8, 0.8, 1.0]);
    batch
}

fn staging(device: &wgpu::Device) -> wgpu::Buffer {
    let size = u64::from(padded_bytes_per_row(WIDTH)) * u64::from(HEIGHT);
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("p9b staging"),
        size,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    })
}

fn submit_frame(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    gui: &mut GuiRenderer,
    harness: &Pure2dHarness,
    heavy: &GuiBatch,
    popup: &GuiBatch,
    buffer: &wgpu::Buffer,
) {
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("p9b heavy + popup"),
    });
    gui.prepare(queue, heavy);
    gui.render(
        &mut encoder,
        &harness.target().view,
        None,
        heavy,
        wgpu::LoadOp::Clear(wgpu::Color::BLACK),
    );
    gui.prepare(queue, popup);
    gui.render(
        &mut encoder,
        &harness.target().view,
        None,
        popup,
        wgpu::LoadOp::Load,
    );
    encoder.copy_texture_to_buffer(
        ImageCopyTexture {
            texture: &harness.target().texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        ImageCopyBuffer {
            buffer,
            layout: ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row(WIDTH)),
                rows_per_image: Some(HEIGHT),
            },
        },
        wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));
}

fn main() {
    let harness = Pure2dHarness::new(WIDTH, HEIGHT, DEFAULT_FORMAT).expect("headless GPU");
    let device = harness.renderer().context.device.clone();
    let queue = harness.renderer().context.queue.clone();
    let mut gui = GuiRenderer::new(
        device.clone(),
        DEFAULT_FORMAT,
        700_000,
        WIDTH,
        HEIGHT,
        1,
        None,
    );
    gui.set_ui_depth(false);
    let heavy = heavy_batch();
    let popup = popup_batch();

    // Warm the pipelines and the buffers.
    {
        let warm = staging(&device);
        submit_frame(&device, &queue, &mut gui, &harness, &heavy, &popup, &warm);
        let slice = warm.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::Maintain::Wait);
        warm.unmap();
    }

    // Sync: the old path, on this thread.
    let mut sync_block_ms = Vec::new();
    for _ in 0..5 {
        let buffer = staging(&device);
        submit_frame(&device, &queue, &mut gui, &harness, &heavy, &popup, &buffer);
        let started = Instant::now();
        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        device.poll(wgpu::Maintain::Wait);
        sync_block_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        buffer.unmap();
    }

    // Async: the new path. The wait happens on a worker thread.
    let mut async_submit_ms = Vec::new();
    let mut async_total_ms = Vec::new();
    for _ in 0..5 {
        let buffer = Arc::new(staging(&device));
        submit_frame(&device, &queue, &mut gui, &harness, &heavy, &popup, &buffer);
        let ready = Arc::new(AtomicBool::new(false));
        let signal = Arc::clone(&ready);
        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, move |_| {
            signal.store(true, Ordering::Release);
        });
        let started = Instant::now();
        device.poll(wgpu::Maintain::Poll);
        async_submit_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        while !ready.load(Ordering::Acquire) {
            device.poll(wgpu::Maintain::Poll);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        async_total_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        buffer.unmap();
    }

    let median = |values: &mut Vec<f64>| {
        values.sort_by(f64::total_cmp);
        values[values.len() / 2]
    };
    let sync = median(&mut sync_block_ms);
    let submit = median(&mut async_submit_ms);
    let total = median(&mut async_total_ms);
    println!(
        "{{\"heavy_quads\":{HEAVY_QUADS},\"sync_block_ms\":{sync:.3},\
         \"async_submit_ms\":{submit:.3},\"async_total_ms\":{total:.3},\
         \"sync_runs\":{sync_block_ms:?},\"async_submit_runs\":{async_submit_ms:?},\
         \"async_total_runs\":{async_total_ms:?}}}"
    );
}
