use ferrous_assets::Font;
use ferrous_core::InputState;
use ferrous_gui::{GuiBatch, TextBatch, Ui};
use ferrous_renderer::Viewport;
use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

use crate::builder::AppConfig;
use crate::context::AppContext;
use crate::graphics::GraphicsState;
use crate::traits::FerrousApp;

struct Runner<A: FerrousApp> {
    app: A,
    config: AppConfig,
    window: Option<Arc<Window>>,
    graphics: Option<GraphicsState>,
    ui: Ui,
    input: InputState,
    window_size: (u32, u32),
    viewport: Viewport,
    last_update: std::time::Instant,
    font: Option<Font>,
    font_rx: Option<std::sync::mpsc::Receiver<Font>>,
}

impl<A: FerrousApp> Runner<A> {
    fn new(app: A, config: AppConfig) -> Self {
        Self {
            app,
            config,
            window: None,
            graphics: None,
            ui: Ui::new(),
            input: InputState::new(),
            viewport: Viewport {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            },
            window_size: (0, 0),
            last_update: std::time::Instant::now(),
            font: None,
            font_rx: None,
        }
    }
}

impl<A: FerrousApp> ApplicationHandler for Runner<A> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.app.configure_ui(&mut self.ui);

        let attributes = Window::default_attributes()
            .with_title(&self.config.title)
            .with_inner_size(winit::dpi::PhysicalSize::new(
                self.config.width,
                self.config.height,
            ));

        let window = Arc::new(event_loop.create_window(attributes).unwrap());
        self.window_size = (self.config.width, self.config.height);
        self.viewport = Viewport {
            x: 0,
            y: 0,
            width: self.config.width,
            height: self.config.height,
        };

        let mut gfx = pollster::block_on(GraphicsState::new(
            &window,
            self.config.width,
            self.config.height,
            self.config.vsync,
        ));
        gfx.renderer.set_viewport(self.viewport);

        // Carga de fuente asíncrona
        if let Some(path) = &self.config.font_path {
            let device = gfx.renderer.context.device.clone();
            let queue = gfx.renderer.context.queue.clone();
            let path = path.clone();
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(Font::load(&path, &device, &queue, ' '..'~'));
            });
            self.font_rx = Some(rx);
        }

        self.window = Some(window.clone());
        self.graphics = Some(gfx);

        let mut ctx = AppContext {
            input: &self.input,
            dt: 0.0,
            window_size: self.window_size,
            window: &window,
            viewport: self.viewport,
            exit_requested: false,
        };
        self.app.setup(&mut ctx);
        self.viewport = ctx.viewport;
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {}

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        self.ui.handle_window_event(&event, &mut self.input);

        if let Some(window) = &self.window {
            let mut ctx = AppContext {
                input: &self.input,
                dt: 0.0,
                window_size: self.window_size,
                window,
                viewport: self.viewport,
                exit_requested: false,
            };
            self.app.on_window_event(&event, &mut ctx);
            if ctx.exit_requested {
                event_loop.exit();
            }
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(gfx) = &mut self.graphics {
                    gfx.resize(size.width, size.height);
                    self.window_size = (size.width, size.height);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let (Some(gfx), Some(window)) = (&mut self.graphics, &self.window) else {
            return;
        };

        // Revisar fuente cargada
        if self.font.is_none() {
            if let Some(Ok(font)) = self.font_rx.as_ref().map(|rx| rx.try_recv()) {
                gfx.renderer
                    .set_font_atlas(&font.atlas.view, &font.atlas.sampler);
                self.font = Some(font);
                self.font_rx = None;
            }
        }

        let now = std::time::Instant::now();
        let dt = (now - self.last_update).as_secs_f32();
        self.last_update = now;

        // 1. UPDATE (Lógica)
        // ctx borrows `self.input` immutably; drop it before we later need a mutable borrow
        {
            let mut ctx = AppContext {
                input: &self.input,
                dt,
                window_size: self.window_size,
                window,
                viewport: self.viewport,
                exit_requested: false,
            };

            self.app.update(&mut ctx);
            if ctx.exit_requested {
                event_loop.exit();
                return;
            }

            // Sincronizar viewport si la app lo cambió
            if self.viewport != ctx.viewport {
                self.viewport = ctx.viewport;
                gfx.renderer.set_viewport(self.viewport);
                self.ui.set_viewport_rect(
                    self.viewport.x as f32,
                    self.viewport.y as f32,
                    self.viewport.width as f32,
                    self.viewport.height as f32,
                );
            }
        }

        // Input de 3D
        if self.viewport.width > 0 && self.viewport.height > 0 && self.ui.viewport_focused() {
            let (mx, my) = self.input.mouse_position();
            if mx >= self.viewport.x as f64
                && mx < (self.viewport.x + self.viewport.width) as f64
                && my >= self.viewport.y as f64
                && my < (self.viewport.y + self.viewport.height) as f64
            {
                gfx.renderer.handle_input(&mut self.input, dt);
            }
        }

        // 2. DRAW 3D (Juegos) & 3. DRAW UI (Interfaces)
        // we need a fresh `ctx` here as well; creating it after the mutable borrow above
        let mut encoder = gfx.renderer.begin_frame();
        let mut ctx = AppContext {
            input: &self.input,
            dt,
            window_size: self.window_size,
            window,
            viewport: self.viewport,
            exit_requested: false,
        };

        if self.viewport.width > 0 {
            self.app.draw_3d(&mut gfx.renderer, &mut ctx);
        }

        let mut gui_batch = GuiBatch::new();
        let mut text_batch = TextBatch::new();
        self.app.draw_ui(
            &mut gui_batch,
            &mut text_batch,
            self.font.as_ref(),
            &mut ctx,
        );
        self.ui
            .draw(&mut gui_batch, &mut text_batch, self.font.as_ref());

        // 4. RENDER FINAL
        let frame = gfx.surface.get_current_texture().unwrap();
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        gfx.renderer
            .render_to_view(&mut encoder, &view, Some(&gui_batch), Some(&text_batch));
        gfx.renderer.context.queue.submit(Some(encoder.finish()));
        frame.present();

        window.request_redraw();
    }
}

pub(crate) fn run_internal<A: FerrousApp + 'static>(config: AppConfig, app: A) {
    let mut runner = Runner::new(app, config);
    let event_loop = EventLoop::new().unwrap();
    event_loop.run_app(&mut runner).unwrap();
}
