use ferrous_assets::Font;
use ferrous_core::{context::EngineContext, InputState};
use ferrous_gui::TextInput;
use ferrous_gui::{Canvas, GuiBatch, GuiQuad, InteractiveButton, Slider, ViewportWidget};
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
    viewport: Viewport,
    // new UI container that handles focus for us
    ui: Canvas,
    // keep shared references to widgets so we can inspect their state
    ui_button: std::rc::Rc<std::cell::RefCell<InteractiveButton>>,
    ui_slider: std::rc::Rc<std::cell::RefCell<Slider>>,
    ui_text_input: std::rc::Rc<std::cell::RefCell<TextInput>>,
    ui_viewport: std::rc::Rc<std::cell::RefCell<ViewportWidget>>,
    window_size: (u32, u32),
    last_update: std::time::Instant,
    // font used for text rendering; built once on resume
    font: Option<Font>,
    // receiver for a background font loader thread. we spawn once on resume
    // and then poll the channel during the update loop; this allows the
    // heavy work (file I/O + atlas building) to happen off the winit event
    // thread so the UI doesn't freeze while the font is being prepared.
    font_rx: Option<std::sync::mpsc::Receiver<Font>>,
}

impl EditorApp {
    fn new() -> Self {
        // create the widgets wrapped in Rc<RefCell> so they can be shared
        let ui_button = std::rc::Rc::new(std::cell::RefCell::new(InteractiveButton::new(
            50.0, 50.0, 100.0, 100.0,
        )));
        let ui_slider = std::rc::Rc::new(std::cell::RefCell::new(Slider::new(
            50.0, 200.0, 200.0, 20.0, 0.5,
        )));
        let ui_text_input = std::rc::Rc::new(std::cell::RefCell::new(TextInput::new(
            50.0, 240.0, 200.0, 24.0,
        )));
        let ui_viewport = std::rc::Rc::new(std::cell::RefCell::new(ViewportWidget::new(
            0.0, 0.0, 0.0, 0.0,
        )));

        let mut app = Self {
            renderer: None,
            window: None,
            surface: None,
            config: None,
            input: InputState::new(),
            viewport: Viewport {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            },
            // new UI container that handles focus for us
            ui: Canvas::new(),
            ui_button: ui_button.clone(),
            ui_slider: ui_slider.clone(),
            ui_text_input: ui_text_input.clone(),
            ui_viewport: ui_viewport.clone(),
            window_size: (0, 0),
            last_update: std::time::Instant::now(),
            font: None,
            font_rx: None,
        };

        // register widgets with canvas so the library handles hit/focus
        app.ui.add(ui_button);
        app.ui.add(ui_slider);
        app.ui.add(ui_text_input);
        app.ui.add(ui_viewport);

        app
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

        // load a real font file using the new high-level loader.
        // spawn a thread to do the actual loading. the device/queue are
        // `Arc`'d inside `EngineContext` so we can safely clone them and
        // move them across threads. Font::load does some GPU work; in
        // practice wgpu devices are `Send`/`Sync` and allow this, but even
        // if they didn’t we could split the work (parse+pixel gen on the
        // thread, texture creation back on the main thread).
        let device = renderer.context.device.clone();
        let queue = renderer.context.queue.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let font = Font::load("assets/fonts/Roboto-Regular.ttf", &device, &queue, ' '..'~');
            // ignore send error; receiver can be dropped if app exits early
            let _ = tx.send(font);
        });
        self.font_rx = Some(rx);

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
                    // keep the viewport widget in sync so it can receive focus
                    self.ui_viewport.borrow_mut().rect =
                        [vp.x as f32, vp.y as f32, vp.width as f32, vp.height as f32];
                    self.window_size = (w, h);
                    renderer.resize(w, h);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.input.set_mouse_position(position.x, position.y);
                // hand movement to GUI; this takes care of hover/drag updates
                self.ui.mouse_move(position.x, position.y);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = state == winit::event::ElementState::Pressed;
                self.input.update_mouse_button(button, pressed);
                let (mx, my) = self.input.mouse_position();
                // let the canvas manage focus/pressed/drag states for all
                // registered widgets
                self.ui.mouse_input(mx, my, pressed);
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
                // forward keyboard event to whichever widget currently has focus
                self.ui.keyboard_input(
                    text.as_deref(),
                    if let winit::keyboard::PhysicalKey::Code(k) = physical_key {
                        Some(k)
                    } else {
                        None
                    },
                    state == winit::event::ElementState::Pressed,
                );
            }
            _ => (),
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // draw a frame each time the loop is about to sleep
        if let (Some(surface), Some(renderer), Some(config)) =
            (&mut self.surface, &mut self.renderer, &mut self.config)
        {
            // poll the background loader; if a font has arrived we
            // install it and hand the atlas off to the renderer.
            if self.font.is_none() {
                if let Some(rx) = &self.font_rx {
                    if let Ok(font) = rx.try_recv() {
                        renderer.set_font_atlas(&font.atlas.view, &font.atlas.sampler);
                        self.font = Some(font);
                        self.font_rx = None; // not needed anymore
                    }
                }
            }
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
            if inside {
                // move camera only when the viewport itself has focus
                if self.ui_viewport.borrow().focused {
                    renderer.handle_input(&mut self.input, dt);
                }
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
            // collect UI render commands (including widgets) and convert
            // them to batches. TextInput.collect already handles placeholder.
            let mut cmds = Vec::new();
            self.ui.collect(&mut cmds);
            let mut text_batch = ferrous_gui::TextBatch::new();
            for cmd in &cmds {
                cmd.to_batches(&mut batch, &mut text_batch, self.font.as_ref());
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
                // show slider value so we actually reference ui_slider field
                let val = self.ui_slider.borrow().value;
                text_batch.draw_text(
                    font,
                    &format!("slider = {:.2}", val),
                    [10.0, 90.0],
                    18.0,
                    [1.0, 1.0, 0.2, 1.0],
                );
                // show button/ text input state to keep references live
                let btn_pressed = self.ui_button.borrow().pressed;
                text_batch.draw_text(
                    font,
                    &format!("button pressed = {}", btn_pressed),
                    [10.0, 110.0],
                    18.0,
                    [0.9, 0.5, 0.2, 1.0],
                );
                let txt_content = self.ui_text_input.borrow().text.clone();
                text_batch.draw_text(
                    font,
                    &format!("text = '{}'", txt_content),
                    [10.0, 130.0],
                    18.0,
                    [0.5, 0.9, 0.2, 1.0],
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
