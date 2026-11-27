//! Shared body of the wgpu clear-only floor apps.
//!
//! The window is 1280x800 *logical* (winit applies the monitor DPI), the
//! swapchain is 8-bit sRGB, present mode is FIFO with a 2-deep image queue, and
//! the app presents exactly one frame, then idles until the watchdog exits.
//! That is the raw-API floor: no UI, no layout, no text.

use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

const CLEAR: wgpu::Color = wgpu::Color {
    r: 6.0 / 255.0,
    g: 8.0 / 255.0,
    b: 13.0 / 255.0,
    a: 1.0,
};

struct Gfx {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
}

struct App {
    gfx: Option<Gfx>,
    frames: Arc<AtomicU64>,
    started: Instant,
    backends: wgpu::Backends,
    title: &'static str,
}

fn parse_seconds() -> f64 {
    std::env::args()
        .skip_while(|arg| arg != "--seconds")
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(8.0)
}

pub fn run(backends: wgpu::Backends, title: &'static str, backend_name: &'static str) {
    let seconds = parse_seconds();
    let started = Instant::now();
    let frames = Arc::new(AtomicU64::new(0));
    let counter = Arc::clone(&frames);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs_f64(seconds));
        println!("frames {}", counter.load(Ordering::Relaxed));
        let _ = std::io::stdout().flush();
        std::process::exit(0);
    });

    println!("backend {}", backend_name);
    let _ = std::io::stdout().flush();

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App {
        gfx: None,
        frames,
        started,
        backends,
        title,
    };
    event_loop.run_app(&mut app).unwrap();
}

fn create(event_loop: &ActiveEventLoop, backends: wgpu::Backends, title: &'static str) -> Gfx {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends,
        ..wgpu::InstanceDescriptor::new_with_display_handle(Box::new(
            event_loop.owned_display_handle(),
        ))
    });

    let window = Arc::new(
        event_loop
            .create_window(
                Window::default_attributes()
                    .with_title(title)
                    .with_inner_size(LogicalSize::new(1280.0, 800.0))
                    .with_resizable(false),
            )
            .unwrap(),
    );

    let size = window.inner_size();
    let surface = instance.create_surface(window.clone()).unwrap();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: Some(&surface),
        force_fallback_adapter: false,
    }))
    .expect("no wgpu adapter for the requested backend");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();

    let capabilities = surface.get_capabilities(&adapter);
    let format = capabilities
        .formats
        .iter()
        .copied()
        .find(|format| format.is_srgb())
        .unwrap_or(capabilities.formats[0]);
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: size.width.max(1),
        height: size.height.max(1),
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 2,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        view_formats: vec![],
    };
    surface.configure(&device, &config);

    Gfx {
        window,
        surface,
        device,
        queue,
        config,
    }
}

fn render(gfx: &mut Gfx) {
    let frame = match gfx.surface.get_current_texture() {
        wgpu::CurrentSurfaceTexture::Success(frame) => frame,
        wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
        wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => return,
        wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
            gfx.surface.configure(&gfx.device, &gfx.config);
            return;
        }
        _ => return,
    };

    let view = frame
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = gfx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("floor-wgpu"),
        });
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("floor-clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(CLEAR),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
    gfx.queue.submit(Some(encoder.finish()));
    frame.present();
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        let gfx = create(event_loop, self.backends, self.title);
        gfx.window.request_redraw();
        self.gfx = Some(gfx);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(gfx) = self.gfx.as_mut() {
                    gfx.config.width = size.width.max(1);
                    gfx.config.height = size.height.max(1);
                    gfx.surface.configure(&gfx.device, &gfx.config);
                    gfx.window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                let frame = self.frames.fetch_add(1, Ordering::Relaxed) + 1;
                if frame == 1 {
                    println!(
                        "startup_ms {}",
                        self.started.elapsed().as_secs_f64() * 1000.0
                    );
                    println!("bench_ready");
                    let _ = std::io::stdout().flush();
                }
                if let Some(gfx) = self.gfx.as_mut() {
                    render(gfx);
                }
            }
            _ => {}
        }
    }
}
