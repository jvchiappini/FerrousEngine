//! Measures the cost of building a `Renderer` and its deferred 3-D group.
//!
//! Prompt 17 S4: a `Flat2D` renderer must not build the 3-D resources at
//! startup. This example times, in one process and with a fresh device per
//! iteration:
//!
//! - `Renderer::new_with_mode(Flat2D)` (what a 2-D app pays at startup),
//! - `ensure_three_d` right after it (the first 3-D use),
//! - `Renderer::new_with_mode(Full3D)` (the pre-S4 path, all at startup).
//!
//! The first iteration of each group pays the driver's cold shader compiler;
//! the median of the following iterations is the number to compare with the
//! app's warm `renderer built` mark.
//!
//! When the backend exposes `Device::generate_allocator_report` (DX12 on
//! Windows), the device bytes before/after the 3-D group are printed too:
//! run it with `WGPU_BACKEND=dx12`. Vulkan has no allocator report in wgpu 23
//! and prints `n/a`.
//!
//! ```text
//! cargo run --release -p ferrous_renderer --example renderer_startup_cost
//! ```

use std::time::Instant;

use ferrous_core::context::EngineContext;
use ferrous_renderer::{Renderer, RendererMode};

const WIDTH: u32 = 1600;
const HEIGHT: u32 = 900;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;
const RUNS: usize = 7;

fn context() -> Option<EngineContext> {
    // Honour `WGPU_BACKEND` like the engine runner does, so the same example
    // measures the allocator report on DX12 (`WGPU_BACKEND=dx12`).
    let backends = wgpu::util::backend_bits_from_env().unwrap_or(wgpu::Backends::all());
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends,
        ..Default::default()
    });
    pollster::block_on(EngineContext::new_with_instance(instance, None)).ok()
}

fn build_renderer(context: EngineContext, mode: RendererMode) -> Renderer {
    Renderer::new_with_mode(context, WIDTH, HEIGHT, FORMAT, 1, None, mode)
}

fn median(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    samples[samples.len() / 2]
}

fn allocator_bytes(device: &wgpu::Device) -> Option<u64> {
    device
        .generate_allocator_report()
        .map(|report| report.total_allocated_bytes)
}

fn main() {
    let Some(probe) = context() else {
        eprintln!("no GPU adapter available");
        return;
    };
    let adapter = probe.adapter.get_info();
    println!(
        "adapter: {} ({:?}, {:?})",
        adapter.name, adapter.device_type, adapter.backend
    );
    drop(probe);

    let mut flat = Vec::with_capacity(RUNS);
    let mut three_d = Vec::with_capacity(RUNS);
    let mut full = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let Some(ctx) = context() else {
            return;
        };
        let start = Instant::now();
        let mut renderer = build_renderer(ctx, RendererMode::Flat2D);
        flat.push(start.elapsed().as_secs_f64() * 1000.0);
        let start = Instant::now();
        renderer.ensure_three_d();
        three_d.push(start.elapsed().as_secs_f64() * 1000.0);
        drop(renderer);

        let Some(ctx) = context() else {
            return;
        };
        let start = Instant::now();
        let renderer = build_renderer(ctx, RendererMode::Full3D);
        full.push(start.elapsed().as_secs_f64() * 1000.0);
        drop(renderer);
    }
    println!(
        "renderer Flat2D        : {:8.2} ms median (all: {:?})",
        median(flat.clone()),
        formatted(&flat)
    );
    println!(
        "first 3-D use          : {:8.2} ms median",
        median(three_d.clone())
    );
    println!(
        "renderer Full3D        : {:8.2} ms median (all: {:?})",
        median(full.clone()),
        formatted(&full)
    );

    // VRAM: one Flat2D renderer, then the same one after ensure_three_d.
    let Some(ctx) = context() else {
        return;
    };
    let before = allocator_bytes(&ctx.device);
    let mut renderer = build_renderer(ctx, RendererMode::Flat2D);
    let after_flat = allocator_bytes(&renderer.context.device);
    renderer.ensure_three_d();
    let after_three_d = allocator_bytes(&renderer.context.device);
    match (before, after_flat, after_three_d) {
        (Some(before), Some(flat), Some(three_d)) => {
            println!(
                "device bytes           : {before} -> {flat} (Flat2D) -> {three_d} (3-D), \
                 +{} for the 3-D group",
                three_d.saturating_sub(flat)
            );
        }
        _ => println!("device bytes           : n/a (no allocator report on this backend)"),
    }
}

fn formatted(samples: &[f64]) -> String {
    samples
        .iter()
        .map(|value| format!("{value:.1}"))
        .collect::<Vec<_>>()
        .join(", ")
}
