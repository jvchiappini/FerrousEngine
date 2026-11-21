//! Prints a step marker between the phases of a headless renderer build.
//!
//! Prompt 18 P3 needs to know which phase of the GPU start-up commits the
//! process's private bytes: instance, adapter, device, the 2-D renderer or the
//! first 3-D use.  This example builds each phase in order and prints a
//! `STEP <name>` line before a short pause, so an external sampler can pair
//! the process counters (`PrivateUsage`, working set) with the phase.
//!
//! ```text
//! python scripts/mem_steps.py -- cargo run --release -p ferrous_renderer \
//!     --example renderer_memory_steps
//! ```
//!
//! `WGPU_BACKEND` and `WGPU_POWER_PREF` are honoured, like the runner.

use std::io::Write;
use std::time::Duration;

use ferrous_core::context::EngineContext;
use ferrous_renderer::{Renderer, RendererMode};

const WIDTH: u32 = 1600;
const HEIGHT: u32 = 900;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8UnormSrgb;
const PAUSE: Duration = Duration::from_millis(700);

fn step(name: &str) {
    println!("STEP {name}");
    let _ = std::io::stdout().flush();
    std::thread::sleep(PAUSE);
}

fn main() {
    step("start");
    let backends = wgpu::util::backend_bits_from_env().unwrap_or(wgpu::Backends::all());
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends,
        ..Default::default()
    });
    step("instance");
    let adapter = match pollster::block_on(EngineContext::request_adapter(&instance, None)) {
        Ok(adapter) => adapter,
        Err(error) => {
            eprintln!("no GPU adapter available: {error}");
            return;
        }
    };
    step("adapter");
    let context = match pollster::block_on(EngineContext::from_adapter_with(
        instance,
        adapter,
        None,
        wgpu::MemoryHints::MemoryUsage,
    )) {
        Ok(context) => context,
        Err(error) => {
            eprintln!("no GPU device available: {error}");
            return;
        }
    };
    step("device");
    let mut renderer = Renderer::new_with_mode(
        context,
        WIDTH,
        HEIGHT,
        FORMAT,
        4,
        None,
        RendererMode::Flat2D,
    );
    step("renderer_flat2d");
    renderer.ensure_three_d();
    step("three_d");
    drop(renderer);
    step("dropped");
}
