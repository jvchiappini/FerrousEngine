//! Raw OpenGL (glow) floor: create a GL 3.3 context, clear the 1280x800
//! logical window once with FIFO swap interval 1, then idle. No UI, no layout,
//! no text — the GL tier's raw-API floor.

use std::ffi::CString;
use std::io::Write;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use glow::HasContext;
use glutin::config::{ConfigTemplateBuilder, GlConfig};
use glutin::context::{
    ContextApi, ContextAttributesBuilder, NotCurrentGlContext, PossiblyCurrentContext, Version,
};
use glutin::display::{GetGlDisplay, GlDisplay};
use glutin::surface::{GlSurface, SwapInterval, WindowSurface};
use glutin_winit::{DisplayBuilder, GlWindow};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::raw_window_handle::HasWindowHandle;
use winit::window::{Window, WindowId};

struct Gfx {
    gl: glow::Context,
    surface: glutin::surface::Surface<WindowSurface>,
    context: PossiblyCurrentContext,
    window: Arc<Window>,
}

struct App {
    gfx: Option<Gfx>,
    frames: Arc<AtomicU64>,
    started: Instant,
}

fn parse_seconds() -> f64 {
    std::env::args()
        .skip_while(|arg| arg != "--seconds")
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(8.0)
}

fn create(event_loop: &ActiveEventLoop) -> Gfx {
    let attributes = Window::default_attributes()
        .with_title("floor-glow")
        .with_inner_size(LogicalSize::new(1280.0, 800.0))
        .with_resizable(false);

    let template = ConfigTemplateBuilder::new().with_alpha_size(8);
    let (window, gl_config) = DisplayBuilder::new()
        .with_window_attributes(Some(attributes))
        .build(event_loop, template, |configs| {
            configs
                .reduce(|acc, config| {
                    if config.num_samples() > acc.num_samples() {
                        config
                    } else {
                        acc
                    }
                })
                .expect("no OpenGL config")
        })
        .expect("no OpenGL display");
    let window = Arc::new(window.expect("no window"));

    let gl_display = gl_config.display();
    let raw_window_handle = window.window_handle().unwrap().as_raw();
    let context_attributes = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
        .build(Some(raw_window_handle));
    let not_current = unsafe {
        gl_display
            .create_context(&gl_config, &context_attributes)
            .expect("create GL context")
    };
    let surface_attributes = window.build_surface_attributes(Default::default()).unwrap();
    let surface = unsafe {
        gl_display
            .create_window_surface(&gl_config, &surface_attributes)
            .unwrap()
    };
    let context = not_current.make_current(&surface).unwrap();
    surface
        .set_swap_interval(&context, SwapInterval::Wait(NonZeroU32::new(1).unwrap()))
        .unwrap();
    let gl = unsafe {
        glow::Context::from_loader_function(|name| {
            let name = CString::new(name).unwrap();
            gl_display.get_proc_address(&name)
        })
    };

    Gfx {
        gl,
        surface,
        context,
        window,
    }
}

fn render(gfx: &mut Gfx) {
    unsafe {
        gfx.gl
            .clear_color(6.0 / 255.0, 8.0 / 255.0, 13.0 / 255.0, 1.0);
        gfx.gl.clear(glow::COLOR_BUFFER_BIT);
    }
    let _ = gfx.surface.swap_buffers(&gfx.context);
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        let gfx = create(event_loop);
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
                    if let (Some(width), Some(height)) =
                        (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                    {
                        gfx.surface.resize(&gfx.context, width, height);
                    }
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

fn main() {
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

    println!("backend gl");
    let _ = std::io::stdout().flush();

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App {
        gfx: None,
        frames,
        started,
    };
    event_loop.run_app(&mut app).unwrap();
}
