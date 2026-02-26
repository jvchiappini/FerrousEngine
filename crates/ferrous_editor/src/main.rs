use ferrous_core::{context::EngineContext, InputState};
use ferrous_gui::{GuiBatch, GuiQuad, Widget};
use ferrous_renderer::Renderer;

use std::sync::Arc;

// trait needed to extract a raw window handle from winit::window::Window
// `HasRawWindowHandle` is re-exported by winit; importing from there keeps our
// dependency list smaller and satisfies the trait bound for
// `window.raw_window_handle()`.

// winit 0.30 uses the application API where you implement
// `ApplicationHandler` instead of manually driving the event loop.
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

/// Simple interactive widget used for testing hit‑testing and color change.
struct TestButton {
    rect: [f32; 4], // x, y, width, height
    hovered: bool,
    pressed: bool,
}

impl TestButton {
    fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            rect: [x, y, w, h],
            hovered: false,
            pressed: false,
        }
    }

    fn hit(&self, x: f64, y: f64) -> bool {
        let x = x as f32;
        let y = y as f32;
        x >= self.rect[0]
            && x <= self.rect[0] + self.rect[2]
            && y >= self.rect[1]
            && y <= self.rect[1] + self.rect[3]
    }
}

impl Widget for TestButton {
    fn draw(&self, batch: &mut GuiBatch) {
        let color = if self.pressed {
            [0.8, 0.2, 0.2, 1.0]
        } else if self.hovered {
            [0.2, 0.8, 0.2, 1.0]
        } else {
            [0.2, 0.2, 0.8, 1.0]
        };
        batch.push(GuiQuad {
            pos: [self.rect[0], self.rect[1]],
            size: [self.rect[2], self.rect[3]],
            color,
        });
    }
}

/// Application state managed by winit's `ApplicationHandler` API.
struct EditorApp {
    renderer: Option<Renderer>,
    window: Option<Arc<Window>>,
    surface: Option<wgpu::Surface<'static>>,
    config: Option<wgpu::SurfaceConfiguration>,
    input: InputState,
    test_button: TestButton,
}

impl EditorApp {
    fn new() -> Self {
        Self {
            renderer: None,
            window: None,
            surface: None,
            config: None,
            input: InputState::new(),
            test_button: TestButton::new(50.0, 50.0, 100.0, 100.0),
        }
    }
}

impl ApplicationHandler for EditorApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // create the window once the application resumes
        let window = Arc::new(
            event_loop
                .create_window(Window::default_attributes())
                .expect("failed to create window"),
        );
        let size = window.inner_size();
        self.window = Some(window.clone());

        // initialize GPU context asynchronously
        let context = pollster::block_on(async { EngineContext::new().await.unwrap() });

        // surface creation using the window directly; wgpu 0.23 can accept a
        // `&Window` as the surface target so we no longer need to pull a raw
        // handle ourselves.
        let surface = context.instance.create_surface(window.as_ref()).unwrap();
        // the borrow lifetime attached to the returned
        // `Surface` is normally tied to the reference, but because we keep the
        // `Arc<Window>` alive for the remainder of the program we can safely
        // extend it to `'static` with a transient transmute.
        let surface: wgpu::Surface<'static> = unsafe { std::mem::transmute(surface) };
        let device_for_surface = context.device.clone();
        let caps = surface.get_capabilities(&context.adapter);
        let surface_format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 0,
        };
        surface.configure(&device_for_surface, &config);

        let renderer = Renderer::new(context, config.width, config.height, config.format);

        self.renderer = Some(renderer);
        self.surface = Some(surface);
        self.config = Some(config);
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        // nothing special to do for now
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(new_size) => {
                if let (Some(surface), Some(renderer), Some(config)) =
                    (&self.surface, &mut self.renderer, &mut self.config)
                {
                    let (w, h) = (new_size.width.max(1), new_size.height.max(1));
                    config.width = w;
                    config.height = h;
                    surface.configure(&renderer.context.device, &config);
                    renderer.resize(w, h);
                }
            }
            WindowEvent::KeyboardInput { .. } => {
                // winit 0.30 exposes `physical_key` and `state` in the event
                // structure. for now we don't translate these to our
                // `KeyCode` type, so just ignore the event.
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.input.set_mouse_position(position.x, position.y);
                self.test_button.hovered = self.test_button.hit(position.x, position.y);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = state == winit::event::ElementState::Pressed;
                self.input.update_mouse_button(button, pressed);
                if pressed {
                    let (mx, my) = self.input.mouse_position();
                    if self.test_button.hit(mx, my) {
                        self.test_button.pressed = true;
                    } else {
                        self.test_button.pressed = false;
                    }
                } else {
                    self.test_button.pressed = false;
                }
            }
            _ => (),
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // draw a frame each time the loop is about to sleep
        if let (Some(surface), Some(renderer), Some(config)) =
            (&mut self.surface, &mut self.renderer, &mut self.config)
        {
            let mut encoder = renderer.begin_frame();

            let mut batch = GuiBatch::new();
            self.test_button.draw(&mut batch);

            renderer.render_to_target(&mut encoder, Some(&batch));

            let frame = match surface.get_current_texture() {
                Ok(f) => f,
                Err(_) => {
                    surface.configure(&renderer.context.device, &config);
                    surface
                        .get_current_texture()
                        .expect("failed to acquire swap chain texture")
                }
            };
            let _view = frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            renderer.context.queue.submit(Some(encoder.finish()));
            frame.present();
        }

        if let Some(win) = &self.window {
            win.request_redraw();
        }
    }
}

fn main() {
    println!("Ferrous editor starting...");

    // the application handler owns all mutable state; we hand it off to
    // winit's platform-specific runtime via `run_app` which will call the
    // trait methods with an `ActiveEventLoop` reference.
    let mut app = EditorApp::new();
    let event_loop = winit::event_loop::EventLoop::new().unwrap();
    // `run_app` takes a mutable reference to the handler and returns a
    // Result we can unwrap; it will drive our `EditorApp` through its
    // lifecycle callbacks.
    event_loop.run_app(&mut app).unwrap();
}
