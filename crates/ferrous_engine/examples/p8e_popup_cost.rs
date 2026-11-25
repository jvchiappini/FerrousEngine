//! Phase 8F Lane B: popup present backends, step by step, in release.
//!
//! Run:
//! ```text
//! cargo run --release -p ferrous_engine --example p8e_popup_cost -- --mode=sharing --present=both
//! WGPU_BACKEND=dx12 cargo run --release -p ferrous_engine --example p8e_popup_cost -- --present=both
//! WGPU_BACKEND=vulkan cargo run --release -p ferrous_engine --example p8e_popup_cost -- --present=both
//! ```
//!
//! Steps (each on a fresh popup, median of three):
//!   a. `create_window` alone (same attributes as `WinitPopups::create`);
//!   b. plus the present setup: `create_surface` + `configure` for a swapchain,
//!      the offscreen texture + DIB for a layered window;
//!   c. plus `GuiRenderer::new` (or `new_sharing`);
//!   d. plus the first `prepare`/`render`/`submit`/`present`.
//!
//! Then input-to-present for a brand-new popup and for a pooled one (5 runs,
//! median and range), the per-repaint cost after a hover change, and optionally
//! the pre-warm decision (`--prewarm`).
//!
//! RSS is `PROCESS_MEMORY_COUNTERS::WorkingSetSize` (`GetProcessMemoryInfo`).
//! Keep the host idle; every timing is wall clock and includes the window
//! system and the driver. `WGPU_BACKEND` picks the instance backend explicitly
//! (`dx12`, `vulkan`, `gl`; absent means every backend).

use std::sync::Arc;
use std::time::Instant;

use ferrous_engine::popup_present::present_frame;
#[cfg(target_os = "windows")]
use ferrous_engine::popup_present::present_offscreen;
use ferrous_engine::wgpu;
use ferrous_engine::windows::{
    drain, PopupContext, PopupPresent, PopupQueue, PopupSpec, WindowMap, WinitPopups,
};
#[cfg(target_os = "windows")]
use ferrous_engine::windows::{LayeredSurface, OffscreenTarget};
use ferrous_engine::winit;
use ferrous_engine::winit::application::ApplicationHandler;
use ferrous_engine::winit::event_loop::{ActiveEventLoop, EventLoop};
use ferrous_engine::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use ferrous_engine::winit::window::Window;
use ferrous_ui_render::{GuiBatch, GuiRenderer};

const POPUP_WIDTH: u32 = 320;
const POPUP_HEIGHT: u32 = 200;
const POPUP_INSTANCES: u32 = 1024;
const POPUP_SVG_VERTICES: u32 = 4096;
const MAIN_WIDTH: u32 = 800;
const MAIN_HEIGHT: u32 = 600;
const STEP_RUNS: usize = 3;
const LATENCY_RUNS: usize = 5;
const REPAINT_RUNS: usize = 50;
const DEMO_COLOR: [f32; 4] = [0.15, 0.35, 0.85, 1.0];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// The pre-8E path: a full `GuiRenderer::new` + `set_ui_depth(false)`.
    New,
    /// The Phase 8E path: reuse the main renderer's GPU objects.
    Sharing,
    /// Like Sharing, but the main renderer targets the engine's real UI target
    /// (Rgba16Float, MSAA 4) while the popup presents sRGB/1 - the target
    /// mismatch the engine actually has.
    Engine,
}

impl Mode {
    fn label(self) -> &'static str {
        match self {
            Mode::New => "new",
            Mode::Sharing => "sharing",
            Mode::Engine => "engine",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PresentChoice {
    Swapchain,
    Layered,
    Both,
}

impl PresentChoice {
    fn backends(self) -> &'static [PopupPresent] {
        match self {
            Self::Swapchain => &[PopupPresent::Swapchain],
            Self::Layered => &[PopupPresent::Layered],
            Self::Both => &[PopupPresent::Swapchain, PopupPresent::Layered],
        }
    }
}

struct Options {
    mode: Mode,
    prewarm: bool,
    present: PresentChoice,
}

struct CostApp {
    options: Option<Options>,
}

fn main() -> Result<(), winit::error::EventLoopError> {
    let mut options = Options {
        mode: Mode::New,
        prewarm: false,
        present: PresentChoice::Both,
    };
    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "--mode=sharing" | "--sharing" => options.mode = Mode::Sharing,
            "--mode=engine" | "--engine" => options.mode = Mode::Engine,
            "--mode=new" | "--new" => options.mode = Mode::New,
            "--present=swapchain" => options.present = PresentChoice::Swapchain,
            "--present=layered" => options.present = PresentChoice::Layered,
            "--present=both" => options.present = PresentChoice::Both,
            "--prewarm" => options.prewarm = true,
            "--help" | "-h" => {
                println!(
                    "usage: p8e_popup_cost [--mode=new|sharing|engine] \
                     [--present=swapchain|layered|both] [--prewarm]"
                );
                return Ok(());
            }
            other => eprintln!("p8e_popup_cost: ignoring unknown argument {other:?}"),
        }
    }

    let event_loop = EventLoop::new()?;
    let mut app = CostApp {
        options: Some(options),
    };
    event_loop.run_app(&mut app)
}

impl ApplicationHandler for CostApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let Some(options) = self.options.take() else {
            event_loop.exit();
            return;
        };
        if let Err(error) = run(event_loop, &options) {
            eprintln!("p8e_popup_cost: {error}");
        }
        event_loop.exit();
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        _event: winit::event::WindowEvent,
    ) {
    }
}

// -- The measurement -------------------------------------------------------

struct Gpu {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    pipeline_cache: Option<Arc<wgpu::PipelineCache>>,
}

fn run(event_loop: &ActiveEventLoop, options: &Options) -> anyhow::Result<()> {
    println!(
        "P8E\tmode={}\tpopup={}x{}\tmain={}x{}\trss_unit=bytes",
        options.mode.label(),
        POPUP_WIDTH,
        POPUP_HEIGHT,
        MAIN_WIDTH,
        MAIN_HEIGHT
    );

    let main_window = Arc::new(
        event_loop
            .create_window(
                Window::default_attributes()
                    .with_title("p8e popup cost (main)")
                    .with_inner_size(winit::dpi::PhysicalSize::new(MAIN_WIDTH, MAIN_HEIGHT)),
            )
            .map_err(|error| anyhow::anyhow!("main window: {error}"))?,
    );
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: backends_from_env(),
        ..Default::default()
    });
    let main_surface = instance
        .create_surface(SurfaceTarget(main_window.clone()))
        .map_err(|error| anyhow::anyhow!("main surface: {error}"))?;
    let context = pollster::block_on(ferrous_gpu::EngineContext::new_with_instance(
        instance,
        Some(&main_surface),
    ))
    .map_err(|error| anyhow::anyhow!("gpu context: {error}"))?;
    let gpu = Gpu {
        instance: context.instance,
        adapter: context.adapter,
        device: context.device.clone(),
        queue: context.queue.clone(),
        pipeline_cache: context.pipeline_cache.clone(),
    };
    let format = choose_format(
        &main_surface,
        &gpu.adapter,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    );
    main_surface.configure(
        &gpu.device,
        &wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            format,
            width: MAIN_WIDTH,
            height: MAIN_HEIGHT,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        },
    );

    // The engine passes this renderer into each popup through `PopupContext`.
    // Keep its depth contract (the engine's 3-D default: UI pipelines declare a
    // depth target). `--mode=engine` uses the engine's real UI target:
    // `Rgba16Float` + MSAA 4, so a popup that presents sRGB/1 builds that
    // pipeline variant once and every later popup on the same renderer caches
    // it (Phase 8F finding 2).
    let (main_format, main_samples) = if options.mode == Mode::Engine {
        (wgpu::TextureFormat::Rgba16Float, 4)
    } else {
        (format, 1)
    };
    let main_gui = GuiRenderer::new(
        gpu.device.clone(),
        main_format,
        POPUP_INSTANCES,
        MAIN_WIDTH,
        MAIN_HEIGHT,
        main_samples,
        gpu.pipeline_cache.clone(),
    );
    let _ = gpu.device.poll(wgpu::Maintain::Wait);
    let adapter = gpu.adapter.get_info();
    println!(
        "P8E\tadapter_backend={:?}\tadapter={}\tmain_renderer=pipeline_set:{}",
        adapter.backend,
        adapter.name,
        main_gui.pipeline_set_id()
    );

    for &present in options.present.backends() {
        if present == PopupPresent::Layered && !cfg!(target_os = "windows") {
            println!("P8E\tpresent=layered\tskipped=Windows-only");
            continue;
        }
        println!("P8E\tpresent={}", present.label());
        step_table(
            event_loop,
            &gpu,
            &main_gui,
            &main_window,
            format,
            options.mode,
            present,
        )?;
        latency_table(
            event_loop,
            &gpu,
            &main_gui,
            &main_window,
            format,
            options.mode,
            present,
        )?;
        if options.prewarm {
            prewarm_table(
                event_loop,
                &gpu,
                &main_gui,
                &main_window,
                format,
                options.mode,
                present,
            )?;
        }
    }

    // Keep the main renderer alive until here so the sharing registry entry
    // survives every measurement.
    drop(main_gui);
    Ok(())
}

/// `(step, elapsed ms, RSS after the step)`.
struct StepSample {
    step: &'static str,
    ms: f64,
    rss: u64,
    /// RSS after this step minus RSS after the previous one (the step's own
    /// allocation), so driver caching between runs cannot distort it.
    delta: i64,
}

#[allow(clippy::too_many_arguments)] // reason: evidence harness
fn step_table(
    event_loop: &ActiveEventLoop,
    gpu: &Gpu,
    main_gui: &GuiRenderer,
    main_window: &Arc<Window>,
    main_format: wgpu::TextureFormat,
    mode: Mode,
    present: PopupPresent,
) -> anyhow::Result<()> {
    if present == PopupPresent::Layered {
        #[cfg(target_os = "windows")]
        {
            return layered_step_table(event_loop, gpu, main_gui, main_window, mode);
        }
        #[cfg(not(target_os = "windows"))]
        {
            return Ok(());
        }
    }
    let mut samples: Vec<StepSample> = Vec::new();
    let mut baselines: Vec<u64> = Vec::new();
    let mut first_variant: Option<u64> = None;
    for run_index in 0..STEP_RUNS {
        let baseline = working_set_bytes();
        baselines.push(baseline);

        // (a) window alone.
        let start = Instant::now();
        let window = create_popup_window(event_loop, main_window)?;
        let after_window = start.elapsed().as_secs_f64() * 1000.0;
        let rss_window = working_set_bytes();
        samples.push(StepSample {
            step: "a:create_window",
            ms: after_window,
            rss: rss_window,
            delta: signed_delta(rss_window, baseline),
        });

        // (b) surface + configure.
        let start = Instant::now();
        let surface = gpu
            .instance
            .create_surface(SurfaceTarget(window.clone()))
            .map_err(|error| anyhow::anyhow!("popup surface: {error}"))?;
        let caps = surface.get_capabilities(&gpu.adapter);
        let format = choose_format(&surface, &gpu.adapter, main_format);
        let config = popup_config(&caps, format);
        surface.configure(&gpu.device, &config);
        let after_surface = start.elapsed().as_secs_f64() * 1000.0;
        let rss_surface = working_set_bytes();
        samples.push(StepSample {
            step: "b:surface+configure",
            ms: after_surface,
            rss: rss_surface,
            delta: signed_delta(rss_surface, rss_window),
        });

        // (c) renderer.
        let start = Instant::now();
        let mut gui = make_renderer(mode, gpu, main_gui, format, POPUP_WIDTH, POPUP_HEIGHT);
        let after_renderer = start.elapsed().as_secs_f64() * 1000.0;
        let shares = gui.shares_pipelines_with(main_gui);
        let depth_bytes = gui.popup_depth_bytes();
        let variant_shared = first_variant.is_some_and(|id| id == gui.pipeline_set_id());
        if first_variant.is_none() {
            first_variant = Some(gui.pipeline_set_id());
        }
        let rss_renderer = working_set_bytes();
        samples.push(StepSample {
            step: "c:GuiRenderer",
            ms: after_renderer,
            rss: rss_renderer,
            delta: signed_delta(rss_renderer, rss_surface),
        });

        // (d) first frame up to `present` returning.
        let start = Instant::now();
        present_one(&gpu.device, &gpu.queue, &mut gui, &surface, &config);
        let after_present = start.elapsed().as_secs_f64() * 1000.0;
        let _ = gpu.device.poll(wgpu::Maintain::Wait);
        let rss_present = working_set_bytes();
        samples.push(StepSample {
            step: "d:first_frame",
            ms: after_present,
            rss: rss_present,
            delta: signed_delta(rss_present, rss_renderer),
        });

        let total = after_window + after_surface + after_renderer + after_present;
        println!(
            "P8E\tpresent=swapchain\tstep_run={run_index}\ttotal_ms={total:.3}\tshares_pipelines={shares}\tvariant_reused={variant_shared}\tpopup_depth_bytes={depth_bytes}\tsteps={:.3},{:.3},{:.3},{:.3}\trss={},{},{},{}",
            after_window, after_surface, after_renderer, after_present, rss_window, rss_surface, rss_renderer, rss_present
        );

        // Destroy this popup and let the driver release what it can.
        drop(gui);
        drop(surface);
        drop(window);
        let _ = gpu.device.poll(wgpu::Maintain::Wait);
        std::thread::sleep(std::time::Duration::from_millis(40));
    }

    println!(
        "P8E\tpresent=swapchain\tstep_baseline_rss={}\trun_baselines={:?}",
        median_u64(&baselines),
        baselines
    );
    for step in [
        "a:create_window",
        "b:surface+configure",
        "c:GuiRenderer",
        "d:first_frame",
    ] {
        let ms: Vec<f64> = samples
            .iter()
            .filter(|sample| sample.step == step)
            .map(|sample| sample.ms)
            .collect();
        let rss: Vec<u64> = samples
            .iter()
            .filter(|sample| sample.step == step)
            .map(|sample| sample.rss)
            .collect();
        let deltas: Vec<i64> = samples
            .iter()
            .filter(|sample| sample.step == step)
            .map(|sample| sample.delta)
            .collect();
        let first = samples.iter().find(|s| s.step == step);
        println!(
            "P8E\tpresent=swapchain\tmedian\tstep={step}\tms={:.3}\trss={}\tfirst_ms={:.3}\trun_delta_mb={:?}\tfirst_delta_mb={:.3}\tmedian_delta_mb={:.3}",
            median_f64(&ms),
            median_u64(&rss),
            first.map_or(0.0, |s| s.ms),
            deltas.iter().map(|&d| mb_signed(d)).collect::<Vec<_>>(),
            mb_signed(first.map_or(0, |s| s.delta)),
            mb_signed(median_i64(&deltas)),
        );
    }
    let first_total: f64 = [
        "a:create_window",
        "b:surface+configure",
        "c:GuiRenderer",
        "d:first_frame",
    ]
    .iter()
    .filter_map(|step| samples.iter().find(|s| s.step == *step).map(|s| s.ms))
    .sum();
    let first_delta: i64 = samples.iter().take(4).map(|sample| sample.delta).sum();
    println!(
        "P8E\tpresent=swapchain\tfirst_popup_total_ms={first_total:.3}\tfirst_popup_delta_mb={:.3}",
        mb_signed(first_delta)
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)] // reason: evidence harness
fn latency_table(
    event_loop: &ActiveEventLoop,
    gpu: &Gpu,
    main_gui: &GuiRenderer,
    main_window: &Arc<Window>,
    main_format: wgpu::TextureFormat,
    _mode: Mode,
    present: PopupPresent,
) -> anyhow::Result<()> {
    let mut map = WindowMap::new(
        main_window.id(),
        WinitPopups::with_present(main_window, present),
    );
    let mut queue = PopupQueue::new();
    let spec = PopupSpec {
        position: (160, 160),
        size: (POPUP_WIDTH, POPUP_HEIGHT),
        transparent: true,
        resizable: false,
    };
    let popup_context = |format: wgpu::TextureFormat| PopupContext {
        event_loop,
        instance: &gpu.instance,
        adapter: &gpu.adapter,
        device: &gpu.device,
        queue: &gpu.queue,
        pipeline_cache: gpu.pipeline_cache.clone(),
        format,
        main: main_gui,
    };
    let tag = present.label();

    // Brand-new popup every run: open + show + window/surface-or-offscreen/
    // renderer + first present, exactly what `Runner::render_popup` does for a
    // fresh window.
    let mut fresh: Vec<f64> = Vec::new();
    let mut fresh_rss: Vec<u64> = Vec::new();
    for run_index in 0..LATENCY_RUNS {
        let before = working_set_bytes();
        let start = Instant::now();
        let id = queue.open(spec);
        queue.show(id, spec.position);
        drain(&mut map, &mut queue, popup_context(main_format));
        let handle = map.handle(id).expect("popup window");
        {
            let target = map
                .backend_mut()
                .window_mut(handle)
                .ok_or_else(|| anyhow::anyhow!("popup target"))?;
            let (width, height) = target.size();
            let mut batch = std::mem::take(&mut target.batch);
            fill_demo(&mut batch, width, height, DEMO_COLOR);
            let outcome = present_frame(target, &batch, spec.position, false);
            if !outcome.presented {
                eprintln!("P8E\tlatency_new\tpresent={tag}\trun={run_index}\tnot_presented");
            }
            target.batch = batch;
        }
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        let _ = gpu.device.poll(wgpu::Maintain::Wait);
        let after = working_set_bytes();
        fresh.push(elapsed);
        fresh_rss.push(after.saturating_sub(before));
        println!(
            "P8E\tlatency_new\tpresent={tag}\trun={run_index}\tms={elapsed:.3}\tdelta_mb={:.3}",
            mb(after.saturating_sub(before)),
        );
        queue.close(id);
        drain(&mut map, &mut queue, popup_context(main_format));
        let _ = gpu.device.poll(wgpu::Maintain::Wait);
        std::thread::sleep(std::time::Duration::from_millis(40));
    }

    // Pooled popup: open once, hide (pool), reopen 5 times.
    let id = queue.open(spec);
    queue.show(id, spec.position);
    drain(&mut map, &mut queue, popup_context(main_format));
    let handle = map.handle(id).expect("popup window");
    {
        let target = map
            .backend_mut()
            .window_mut(handle)
            .ok_or_else(|| anyhow::anyhow!("popup target"))?;
        let (width, height) = target.size();
        let mut batch = std::mem::take(&mut target.batch);
        fill_demo(&mut batch, width, height, DEMO_COLOR);
        let _ = present_frame(target, &batch, spec.position, false);
        target.batch = batch;
    }
    let _ = gpu.device.poll(wgpu::Maintain::Wait);
    queue.hide(id);
    drain(&mut map, &mut queue, popup_context(main_format));

    let mut pooled: Vec<f64> = Vec::new();
    for run_index in 0..LATENCY_RUNS {
        let start = Instant::now();
        queue.show(id, spec.position);
        drain(&mut map, &mut queue, popup_context(main_format));
        {
            let target = map
                .backend_mut()
                .window_mut(handle)
                .ok_or_else(|| anyhow::anyhow!("popup target"))?;
            let (width, height) = target.size();
            let mut batch = std::mem::take(&mut target.batch);
            fill_demo(&mut batch, width, height, DEMO_COLOR);
            let _ = present_frame(target, &batch, spec.position, false);
            target.batch = batch;
        }
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        pooled.push(elapsed);
        println!("P8E\tlatency_pooled\tpresent={tag}\trun={run_index}\tms={elapsed:.3}");
        queue.hide(id);
        drain(&mut map, &mut queue, popup_context(main_format));
        let _ = gpu.device.poll(wgpu::Maintain::Wait);
    }

    // Repaint cost: the window is already open and warm; each iteration is one
    // hover change (the fill colour moves) plus one present.
    queue.show(id, spec.position);
    drain(&mut map, &mut queue, popup_context(main_format));
    let mut repaints: Vec<f64> = Vec::new();
    {
        let target = map
            .backend_mut()
            .window_mut(handle)
            .ok_or_else(|| anyhow::anyhow!("repaint target"))?;
        let (width, height) = target.size();
        let mut batch = std::mem::take(&mut target.batch);
        for run_index in 0..REPAINT_RUNS {
            let hover = 0.2 + 0.6 * (run_index as f32 / REPAINT_RUNS as f32);
            fill_demo(&mut batch, width, height, [0.15, hover, 0.85, 1.0]);
            let start = Instant::now();
            let outcome = present_frame(target, &batch, spec.position, false);
            let elapsed = start.elapsed().as_secs_f64() * 1_000.0;
            if outcome.presented {
                repaints.push(elapsed);
            }
        }
        target.batch = batch;
    }
    let _ = gpu.device.poll(wgpu::Maintain::Wait);

    println!(
        "P8E\tlatency_summary\tpresent={tag}\tnew_median_ms={:.3}\tnew_range_ms={:.3}-{:.3}\tnew_delta_mb_median={:.3}\tpooled_median_ms={:.3}\tpooled_range_ms={:.3}-{:.3}\trepaint_median_us={:.1}\trepaint_range_us={:.1}-{:.1}",
        median_f64(&fresh),
        fresh.iter().copied().fold(f64::INFINITY, f64::min),
        fresh.iter().copied().fold(0.0, f64::max),
        mb(median_u64(&fresh_rss)),
        median_f64(&pooled),
        pooled.iter().copied().fold(f64::INFINITY, f64::min),
        pooled.iter().copied().fold(0.0, f64::max),
        median_f64(&repaints) * 1000.0,
        repaints.iter().copied().fold(f64::INFINITY, f64::min) * 1000.0,
        repaints.iter().copied().fold(0.0, f64::max) * 1000.0,
    );
    queue.close(id);
    drain(&mut map, &mut queue, popup_context(main_format));
    Ok(())
}

#[allow(clippy::too_many_arguments)] // reason: evidence harness
fn prewarm_table(
    event_loop: &ActiveEventLoop,
    gpu: &Gpu,
    main_gui: &GuiRenderer,
    main_window: &Arc<Window>,
    main_format: wgpu::TextureFormat,
    mode: Mode,
    present: PopupPresent,
) -> anyhow::Result<()> {
    let _ = mode;
    let mut map = WindowMap::new(
        main_window.id(),
        WinitPopups::with_present(main_window, present),
    );
    let mut queue = PopupQueue::new();
    let spec = PopupSpec {
        position: (160, 160),
        size: (POPUP_WIDTH, POPUP_HEIGHT),
        transparent: true,
        resizable: false,
    };
    let popup_context = |format: wgpu::TextureFormat| PopupContext {
        event_loop,
        instance: &gpu.instance,
        adapter: &gpu.adapter,
        device: &gpu.device,
        queue: &gpu.queue,
        pipeline_cache: gpu.pipeline_cache.clone(),
        format,
        main: main_gui,
    };

    // Pre-warm during the "idle" period: create the hidden window, surface and
    // renderer, but do not show or present.
    let before = working_set_bytes();
    let start = Instant::now();
    let id = queue.open(spec);
    drain(&mut map, &mut queue, popup_context(main_format));
    let handle = map.handle(id).expect("prewarm window");
    let shares = map
        .backend()
        .window(handle)
        .and_then(|window| window.gui.as_ref())
        .is_some_and(|gui| gui.shares_pipelines_with(main_gui));
    let prewarm_ms = start.elapsed().as_secs_f64() * 1000.0;
    let after = working_set_bytes();
    // Evidence that `memory-report` exposes the `popups` subsystem: the hidden
    // popup's present target and GUI buffers are registered while it waits.
    println!(
        "P8E\tmemory_report\t{}",
        ferrous_core::memory::report_json(64)
    );

    // First open with the pre-warmed popup: show + first frame only.
    let start = Instant::now();
    queue.show(id, spec.position);
    drain(&mut map, &mut queue, popup_context(main_format));
    {
        let target = map
            .backend_mut()
            .window_mut(handle)
            .ok_or_else(|| anyhow::anyhow!("prewarm target"))?;
        let (width, height) = target.size();
        let mut batch = std::mem::take(&mut target.batch);
        fill_demo(&mut batch, width, height, DEMO_COLOR);
        let _ = present_frame(target, &batch, spec.position, false);
        target.batch = batch;
    }
    let first_open_ms = start.elapsed().as_secs_f64() * 1000.0;
    let _ = gpu.device.poll(wgpu::Maintain::Wait);

    println!(
        "P8E\tprewarm\tpresent={}\tprewarm_ms={prewarm_ms:.3}\thidden_delta_mb={:.3}\tfirst_open_ms={first_open_ms:.3}\tshares={shares}",
        present.label(),
        mb(after.saturating_sub(before)),
    );
    queue.close(id);
    drain(&mut map, &mut queue, popup_context(main_format));
    Ok(())
}

/// The batch the latency/repaint runs present: one bar over the popup size.
fn fill_demo(batch: &mut GuiBatch, width: u32, height: u32, color: [f32; 4]) {
    batch.clear();
    batch.rect(8.0, 8.0, (width.saturating_sub(16)) as f32, 40.0, color);
    batch.rect(
        8.0,
        56.0,
        (width.saturating_sub(16)) as f32,
        (height.saturating_sub(64)) as f32,
        [0.08, 0.09, 0.12, 0.92],
    );
}

#[cfg(target_os = "windows")]
#[allow(clippy::too_many_arguments)] // reason: evidence harness
fn layered_step_table(
    event_loop: &ActiveEventLoop,
    gpu: &Gpu,
    main_gui: &GuiRenderer,
    main_window: &Arc<Window>,
    mode: Mode,
) -> anyhow::Result<()> {
    let mut samples: Vec<StepSample> = Vec::new();
    let mut baselines: Vec<u64> = Vec::new();
    for run_index in 0..STEP_RUNS {
        let baseline = working_set_bytes();
        baselines.push(baseline);

        // (a) window alone (borderless, owned, no taskbar, WS_EX_LAYERED).
        let start = Instant::now();
        let window = create_popup_window(event_loop, main_window)?;
        let after_window = start.elapsed().as_secs_f64() * 1000.0;
        let rss_window = working_set_bytes();
        samples.push(StepSample {
            step: "a:create_window",
            ms: after_window,
            rss: rss_window,
            delta: signed_delta(rss_window, baseline),
        });

        // (b) offscreen texture + staging + DIB (no surface, no swapchain).
        let start = Instant::now();
        let format = wgpu::TextureFormat::Bgra8UnormSrgb;
        let offscreen = OffscreenTarget::new(&gpu.device, POPUP_WIDTH, POPUP_HEIGHT, format);
        let mut layered =
            LayeredSurface::new(hwnd_of(&window), POPUP_WIDTH, POPUP_HEIGHT, (160, 160))
                .ok_or_else(|| anyhow::anyhow!("layered DIB"))?;
        let after_offscreen = start.elapsed().as_secs_f64() * 1000.0;
        let rss_offscreen = working_set_bytes();
        samples.push(StepSample {
            step: "b:offscreen+dib",
            ms: after_offscreen,
            rss: rss_offscreen,
            delta: signed_delta(rss_offscreen, rss_window),
        });

        // (c) renderer.
        let start = Instant::now();
        let gui = make_renderer(mode, gpu, main_gui, format, POPUP_WIDTH, POPUP_HEIGHT);
        let after_renderer = start.elapsed().as_secs_f64() * 1000.0;
        let shares = gui.shares_pipelines_with(main_gui);
        let rss_renderer = working_set_bytes();
        samples.push(StepSample {
            step: "c:GuiRenderer",
            ms: after_renderer,
            rss: rss_renderer,
            delta: signed_delta(rss_renderer, rss_offscreen),
        });

        // (d) render + readback + UpdateLayeredWindow.
        let start = Instant::now();
        let mut batch = GuiBatch::new();
        fill_demo(&mut batch, POPUP_WIDTH, POPUP_HEIGHT, DEMO_COLOR);
        let presented = present_offscreen(
            &gpu.device,
            &gpu.queue,
            &gui,
            &batch,
            &offscreen,
            &mut layered,
            (160, 160),
            wgpu::Color::TRANSPARENT,
        )
        .is_some();
        let after_present = start.elapsed().as_secs_f64() * 1000.0;
        let _ = gpu.device.poll(wgpu::Maintain::Wait);
        let rss_present = working_set_bytes();
        samples.push(StepSample {
            step: "d:first_frame",
            ms: after_present,
            rss: rss_present,
            delta: signed_delta(rss_present, rss_renderer),
        });
        let total = after_window + after_offscreen + after_renderer + after_present;
        println!(
            "P8E\tpresent=layered\tstep_run={run_index}\ttotal_ms={total:.3}\tshares_pipelines={shares}\tpresented={presented}\tsteps={:.3},{:.3},{:.3},{:.3}\trss={},{},{},{}",
            after_window, after_offscreen, after_renderer, after_present, rss_window, rss_offscreen, rss_renderer, rss_present
        );
        drop(layered);
        drop(gui);
        drop(window);
        let _ = gpu.device.poll(wgpu::Maintain::Wait);
        std::thread::sleep(std::time::Duration::from_millis(40));
    }

    println!(
        "P8E\tpresent=layered\tstep_baseline_rss={}\trun_baselines={:?}",
        median_u64(&baselines),
        baselines
    );
    for step in [
        "a:create_window",
        "b:offscreen+dib",
        "c:GuiRenderer",
        "d:first_frame",
    ] {
        let ms: Vec<f64> = samples
            .iter()
            .filter(|sample| sample.step == step)
            .map(|sample| sample.ms)
            .collect();
        let rss: Vec<u64> = samples
            .iter()
            .filter(|sample| sample.step == step)
            .map(|sample| sample.rss)
            .collect();
        let deltas: Vec<i64> = samples
            .iter()
            .filter(|sample| sample.step == step)
            .map(|sample| sample.delta)
            .collect();
        let first = samples.iter().find(|s| s.step == step);
        println!(
            "P8E\tpresent=layered\tmedian\tstep={step}\tms={:.3}\trss={}\tfirst_ms={:.3}\trun_delta_mb={:?}\tfirst_delta_mb={:.3}\tmedian_delta_mb={:.3}",
            median_f64(&ms),
            median_u64(&rss),
            first.map_or(0.0, |s| s.ms),
            deltas.iter().map(|&d| mb_signed(d)).collect::<Vec<_>>(),
            mb_signed(first.map_or(0, |s| s.delta)),
            mb_signed(median_i64(&deltas)),
        );
    }
    let first_total: f64 = [
        "a:create_window",
        "b:offscreen+dib",
        "c:GuiRenderer",
        "d:first_frame",
    ]
    .iter()
    .filter_map(|step| samples.iter().find(|s| s.step == *step).map(|s| s.ms))
    .sum();
    let first_delta: i64 = samples.iter().take(4).map(|sample| sample.delta).sum();
    println!(
        "P8E\tpresent=layered\tfirst_popup_total_ms={first_total:.3}\tfirst_popup_delta_mb={:.3}",
        mb_signed(first_delta)
    );
    Ok(())
}

fn make_renderer(
    mode: Mode,
    gpu: &Gpu,
    main_gui: &GuiRenderer,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> GuiRenderer {
    match mode {
        Mode::New => {
            let mut renderer = GuiRenderer::new(
                gpu.device.clone(),
                format,
                POPUP_INSTANCES,
                width,
                height,
                1,
                gpu.pipeline_cache.clone(),
            );
            renderer.set_ui_depth(false);
            renderer.with_svg_capacity(POPUP_SVG_VERTICES)
        }
        Mode::Sharing | Mode::Engine => GuiRenderer::new_sharing(
            main_gui,
            gpu.device.clone(),
            format,
            POPUP_INSTANCES,
            width,
            height,
            1,
        )
        .with_svg_capacity(POPUP_SVG_VERTICES),
    }
}

/// Prepares, renders, submits and presents one popup frame.
fn present_one(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    gui: &mut GuiRenderer,
    surface: &wgpu::Surface<'_>,
    config: &wgpu::SurfaceConfiguration,
) {
    let mut batch = GuiBatch::new();
    batch.rect(
        8.0,
        8.0,
        (config.width.saturating_sub(16)) as f32,
        40.0,
        [0.15, 0.35, 0.85, 1.0],
    );
    let frame = acquire(surface, device, config);
    let view = frame
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());
    gui.prepare(queue, &batch);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("p8e popup frame"),
    });
    gui.render(
        &mut encoder,
        &view,
        None,
        &batch,
        wgpu::LoadOp::Clear(wgpu::Color::BLACK),
    );
    queue.submit(Some(encoder.finish()));
    frame.present();
}

fn acquire(
    surface: &wgpu::Surface<'_>,
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
) -> wgpu::SurfaceTexture {
    match surface.get_current_texture() {
        Ok(frame) => frame,
        Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
            surface.configure(device, config);
            surface
                .get_current_texture()
                .expect("swapchain after reconfigure")
        }
        Err(error) => panic!("surface error: {error}"),
    }
}

// -- Window/surface setup (mirrors `WinitPopups::create`) ------------------

fn create_popup_window(
    event_loop: &ActiveEventLoop,
    main: &Arc<Window>,
) -> anyhow::Result<Arc<Window>> {
    let mut attributes = Window::default_attributes()
        .with_decorations(false)
        .with_visible(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_active(false)
        .with_inner_size(winit::dpi::PhysicalSize::new(POPUP_WIDTH, POPUP_HEIGHT))
        .with_position(winit::dpi::PhysicalPosition::new(160, 160));
    #[cfg(target_os = "windows")]
    {
        use winit::platform::windows::WindowAttributesExtWindows;
        attributes = attributes
            .with_owner_window(hwnd_of(main))
            .with_skip_taskbar(true);
    }
    #[cfg(not(target_os = "windows"))]
    let _ = main;
    let window = Arc::new(
        event_loop
            .create_window(attributes)
            .map_err(|error| anyhow::anyhow!("popup window: {error}"))?,
    );
    #[cfg(target_os = "windows")]
    set_no_activate(hwnd_of(&window));
    Ok(window)
}

/// The instance backends the experiment asks for.
///
/// wgpu 23's `Instance::new` takes the descriptor's backends; this reads the
/// same `WGPU_BACKEND` spelling the Phase 8F prompt uses, so a run can force
/// DX12 or Vulkan without a custom flag. Unknown values keep every backend.
fn backends_from_env() -> wgpu::Backends {
    match std::env::var("WGPU_BACKEND")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "dx12" | "d3d12" => wgpu::Backends::DX12,
        "vulkan" | "vk" => wgpu::Backends::VULKAN,
        "gl" | "gles" | "opengl" => wgpu::Backends::GL,
        "" => wgpu::Backends::all(),
        other => {
            eprintln!("p8e_popup_cost: unknown WGPU_BACKEND={other:?}, using all");
            wgpu::Backends::all()
        }
    }
}

fn choose_format(
    surface: &wgpu::Surface<'_>,
    adapter: &wgpu::Adapter,
    fallback: wgpu::TextureFormat,
) -> wgpu::TextureFormat {
    let caps = surface.get_capabilities(adapter);
    caps.formats
        .iter()
        .copied()
        .find(wgpu::TextureFormat::is_srgb)
        .or_else(|| caps.formats.first().copied())
        .unwrap_or(fallback)
}

fn popup_config(
    caps: &wgpu::SurfaceCapabilities,
    format: wgpu::TextureFormat,
) -> wgpu::SurfaceConfiguration {
    let alpha_mode = if caps
        .alpha_modes
        .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
    {
        wgpu::CompositeAlphaMode::PreMultiplied
    } else {
        wgpu::CompositeAlphaMode::Opaque
    };
    let mut usage = wgpu::TextureUsages::RENDER_ATTACHMENT;
    if caps.usages.contains(wgpu::TextureUsages::COPY_SRC) {
        usage |= wgpu::TextureUsages::COPY_SRC;
    }
    wgpu::SurfaceConfiguration {
        usage,
        format,
        width: POPUP_WIDTH,
        height: POPUP_HEIGHT,
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode,
        view_formats: vec![],
        desired_maximum_frame_latency: 1,
    }
}

/// Owns its window so the surface target is `'static`.
struct SurfaceTarget(Arc<Window>);

impl HasWindowHandle for SurfaceTarget {
    fn window_handle(
        &self,
    ) -> Result<winit::raw_window_handle::WindowHandle<'_>, winit::raw_window_handle::HandleError>
    {
        self.0.window_handle()
    }
}

impl winit::raw_window_handle::HasDisplayHandle for SurfaceTarget {
    fn display_handle(
        &self,
    ) -> Result<winit::raw_window_handle::DisplayHandle<'_>, winit::raw_window_handle::HandleError>
    {
        self.0.display_handle()
    }
}

#[cfg(target_os = "windows")]
fn hwnd_of(window: &Window) -> isize {
    match window.window_handle().map(|handle| handle.as_raw()) {
        Ok(RawWindowHandle::Win32(handle)) => handle.hwnd.get(),
        _ => 0,
    }
}

#[cfg(target_os = "windows")]
fn set_no_activate(hwnd: isize) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE,
    };
    if hwnd == 0 {
        return;
    }
    let window = hwnd as *mut core::ffi::c_void;
    // SAFETY: live window handle created above.
    let style = unsafe { GetWindowLongPtrW(window, GWL_EXSTYLE) };
    // SAFETY: same live window handle.
    unsafe {
        SetWindowLongPtrW(window, GWL_EXSTYLE, style | WS_EX_NOACTIVATE as isize);
    }
}

// -- RSS -----------------------------------------------------------------

#[cfg(target_os = "windows")]
#[repr(C)]
#[derive(Default)]
struct ProcessMemoryCounters {
    cb: u32,
    page_fault_count: u32,
    peak_working_set_size: usize,
    working_set_size: usize,
    quota_peak_paged_pool_usage: usize,
    quota_paged_pool_usage: usize,
    quota_peak_non_paged_pool_usage: usize,
    quota_non_paged_pool_usage: usize,
    pagefile_usage: usize,
    peak_pagefile_usage: usize,
}

#[cfg(target_os = "windows")]
#[link(name = "psapi")]
extern "system" {
    fn GetProcessMemoryInfo(
        process: *mut core::ffi::c_void,
        counters: *mut ProcessMemoryCounters,
        cb: u32,
    ) -> i32;
}

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcess() -> *mut core::ffi::c_void;
}

#[cfg(target_os = "windows")]
fn working_set_bytes() -> u64 {
    let mut counters = ProcessMemoryCounters {
        cb: u32::try_from(std::mem::size_of::<ProcessMemoryCounters>()).unwrap_or(0),
        ..Default::default()
    };
    // SAFETY: `counters` is a valid, correctly-sized PROCESS_MEMORY_COUNTERS.
    let ok = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters,
            std::mem::size_of::<ProcessMemoryCounters>() as u32,
        )
    };
    if ok == 0 {
        0
    } else {
        counters.working_set_size as u64
    }
}

#[cfg(not(target_os = "windows"))]
fn working_set_bytes() -> u64 {
    0
}

fn mb(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn mb_signed(bytes: i64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn signed_delta(after: u64, before: u64) -> i64 {
    after as i64 - before as i64
}

fn median_i64(values: &[i64]) -> i64 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}

fn median_f64(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[sorted.len() / 2]
}

fn median_u64(values: &[u64]) -> u64 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted[sorted.len() / 2]
}
