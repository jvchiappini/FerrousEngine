use ferrous_assets::Font;
use ferrous_core::{context::EngineContext, InputState};
use ferrous_gui::{GuiBatch, GuiQuad, InteractiveButton, Slider};
use ferrous_gui::TextInput;
use ferrous_renderer::{Renderer, Viewport};
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

// interactive button widget lives in its own module, so we can delete the
// ad-hoc `TestButton` that previously duplicated the logic.

/// Application state managed by winit's `ApplicationHandler` API.
struct EditorApp {
    renderer: Option<Renderer>,
    window: Option<Arc<Window>>,
    surface: Option<wgpu::Surface<'static>>,
    config: Option<wgpu::SurfaceConfiguration>,
    input: InputState,
    button: InteractiveButton,
    slider: Slider,
    text_input: TextInput,
    viewport: Viewport,
    window_size: (u32, u32),
    last_update: std::time::Instant,
    // font used for text rendering; built once on resume
    font: Option<Font>,
}

impl EditorApp {
    fn new() -> Self {
        Self {
            renderer: None,
            window: None,
            surface: None,
            config: None,
            input: InputState::new(),
            button: InteractiveButton::new(50.0, 50.0, 100.0, 100.0),
            slider: Slider::new(50.0, 200.0, 200.0, 20.0, 0.5),
            text_input: TextInput::new(50.0, 240.0, 200.0, 24.0),
            viewport: Viewport {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            },
            window_size: (0, 0),
            last_update: std::time::Instant::now(),
            font: None,
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

        let mut renderer = Renderer::new(context, config.width, config.height, config.format);

        // load a real font file using the new high-level loader.
        let font = Font::load(
            "assets/fonts/Roboto-Regular.ttf",
            &renderer.context.device,
            &renderer.context.queue,
            ' '..'~',
        );
        // hand atlas to renderer which will forward to its GUI component
        renderer.set_font_atlas(&font.atlas.view, &font.atlas.sampler);

        self.font = Some(font);

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
                    // compute viewport: leave 300px on left, 200px bottom
                    let vp = Viewport {
                        x: 300,
                        y: 0,
                        width: w.saturating_sub(300),
                        height: h.saturating_sub(200),
                    };
                    renderer.set_viewport(vp);
                    self.viewport = vp;
                    self.window_size = (w, h);
                    renderer.resize(w, h);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.input.set_mouse_position(position.x, position.y);
                self.button.hovered = self.button.hit(position.x, position.y);
                if self.slider.dragging {
                    // update slider value while dragging
                    self.slider.update_value(position.x);
                }
                // change focus based on cursor if not dragging slider
                if !self.slider.dragging {
                    if self.text_input.hit(position.x, position.y) {
                        self.text_input.focused = true;
                    } else {
                        self.text_input.focused = false;
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = state == winit::event::ElementState::Pressed;
                self.input.update_mouse_button(button, pressed);
                if pressed {
                    let (mx, my) = self.input.mouse_position();
                    if self.button.hit(mx, my) {
                        self.button.pressed = true;
                    } else {
                        self.button.pressed = false;
                    }
                    // slider thumb press
                    if self.slider.thumb_hit(mx, my) {
                        self.slider.dragging = true;
                    }
                    // clicking text input will focus it
                    if self.text_input.hit(mx, my) {
                        self.text_input.focused = true;
                    }
                } else {
                    self.button.pressed = false;
                    // stop slider dragging when mouse released
                    self.slider.dragging = false;
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                // existing key handling plus text insertion/backspace
                let winit::event::KeyEvent {
                    physical_key,
                    state,
                    text,
                    ..
                } = event;
                if let winit::keyboard::PhysicalKey::Code(code) = physical_key {
                    self.input
                        .update_key(code, state == winit::event::ElementState::Pressed);
                }
                if self.text_input.focused {
                    // insert any text from the event
                    if let Some(txt) = text {
                        for c in txt.chars() {
                            if !c.is_control() {
                                self.text_input.insert_char(c);
                            }
                        }
                    }
                    // handle backspace key via KeyCode
                    if state == winit::event::ElementState::Pressed {
                        use winit::keyboard::KeyCode;
                        if physical_key
                            == winit::keyboard::PhysicalKey::Code(KeyCode::Backspace)
                        {
                            self.text_input.backspace();
                        }
                    }
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
            // compute delta time
            let now = std::time::Instant::now();
            let dt = (now - self.last_update).as_secs_f32();
            self.last_update = now;

            // update camera based on WASD if the UI isn't grabbing the mouse
            // camera movement only when cursor inside viewport
            let (mx, my) = self.input.mouse_position();
            let inside = mx >= self.viewport.x as f64
                && mx < (self.viewport.x + self.viewport.width) as f64
                && my >= self.viewport.y as f64
                && my < (self.viewport.y + self.viewport.height) as f64;
            if inside && !self.button.hovered {
                renderer.handle_input(&mut self.input, dt);
            }

            let mut encoder = renderer.begin_frame();

            let mut batch = GuiBatch::new();
            // draw grey side panel
            let (w, h) = self.window_size;
            batch.push(GuiQuad {
                pos: [0.0, 0.0],
                size: [300.0, h as f32],
                color: [0.145, 0.145, 0.145, 1.0],
            });
            // draw grey bottom panel
            batch.push(GuiQuad {
                pos: [0.0, (h.saturating_sub(200)) as f32],
                size: [w as f32, 200.0],
                color: [0.145, 0.145, 0.145, 1.0],
            });
            self.button.draw(&mut batch);
            // draw slider
            self.slider.draw(&mut batch);
            // prepare text batch before drawing input
            let mut text_batch = ferrous_gui::TextBatch::new();
            // draw text input using font batch
            if let Some(font) = &self.font {
                if self.text_input.placeholder.is_empty() {
                    self.text_input.placeholder = "Type here...".to_string();
                }
                self.text_input.draw(&mut batch, &mut text_batch, Some(font));
            } else {
                self.text_input.draw(&mut batch, &mut text_batch, None);
            }

            // build additional text examples if we have a font
            if let Some(font) = &self.font {
                // Render text to verify the MSDF pipeline is working correctly.
                text_batch.draw_text(
                    font,
                    "Hello FerrousEngine!",
                    [10.0, 10.0],
                    24.0,
                    [1.0, 1.0, 1.0, 1.0],
                );
                text_batch.draw_text(
                    font,
                    "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
                    [10.0, 40.0],
                    18.0,
                    [1.0, 0.9, 0.3, 1.0],
                );
                text_batch.draw_text(
                    font,
                    "abcdefghijklmnopqrstuvwxyz 0123456789",
                    [10.0, 64.0],
                    18.0,
                    [0.7, 0.85, 1.0, 1.0],
                );
            }

            // acquire the swapchain frame before drawing; we will render
            // directly into its texture view
            let frame = match surface.get_current_texture() {
                Ok(f) => f,
                Err(_) => {
                    surface.configure(&renderer.context.device, &config);
                    surface
                        .get_current_texture()
                        .expect("failed to acquire swap chain texture")
                }
            };
            let view = frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());

            renderer.render_to_view(&mut encoder, &view, Some(&batch), Some(&text_batch));

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
