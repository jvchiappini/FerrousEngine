use ferrous_assets::Font;
use ferrous_core::{InputState, TimeClock, World};
use ferrous_gui::{GuiBatch, TextBatch, Ui};
use ferrous_renderer::Viewport;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
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
    clock: TimeClock,
    world: World,
    font: Option<Font>,
    font_rx: Option<std::sync::mpsc::Receiver<Font>>,
    /// Timestamp of the last rendered frame, used to enforce the frame budget
    /// even when the OS delivers input events faster than the target FPS.
    last_frame: Instant,
    /// Timestamp of the last received window event (e.g. input), used to determine
    /// if the application is idle and should stop continuous rendering.
    last_action_time: Instant,
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
            clock: TimeClock::new(),
            world: World::new(),
            font: None,
            font_rx: None,
            last_frame: Instant::now(),
            last_action_time: Instant::now(),
        }
    }

    /// Executes one full update + render cycle.  Called exclusively from
    /// `WindowEvent::RedrawRequested` so that input events (mouse moves, etc.)
    /// never trigger extra renders outside the frame budget.
    fn render_frame(&mut self, event_loop: &ActiveEventLoop) {
        let (Some(gfx), Some(window)) = (&mut self.graphics, &self.window) else {
            return;
        };

        // Stamp the frame start for the next deadline calculation.
        self.last_frame = Instant::now();

        // Check for async font completion
        if self.font.is_none() {
            if let Some(Ok(font)) = self.font_rx.as_ref().map(|rx| rx.try_recv()) {
                gfx.renderer
                    .set_font_atlas(&font.atlas.view, &font.atlas.sampler);
                self.font = Some(font);
                self.font_rx = None;
            }
        }

        // Advance the frame clock
        let time = self.clock.tick();

        // ── 1. UPDATE ────────────────────────────────────────────────────────
        {
            let mut ctx = AppContext {
                input: &self.input,
                time,
                window_size: self.window_size,
                window,
                viewport: self.viewport,
                world: &mut self.world,
                renderer: Some(&mut gfx.renderer),
                exit_requested: false,
            };

            self.app.update(&mut ctx);
            if ctx.exit_requested {
                event_loop.exit();
                return;
            }

            // Sync viewport change to renderer + GUI
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

        // ── Auto world → renderer sync ───────────────────────────────────────
        gfx.renderer.sync_world(&self.world);

        // ── 3D camera input ──────────────────────────────────────────────────
        let dt = time.delta;
        if self.viewport.width > 0 && self.viewport.height > 0 {
            let (mx, my) = self.input.mouse_position();
            let in_viewport = mx >= self.viewport.x as f64
                && mx < (self.viewport.x + self.viewport.width) as f64
                && my >= self.viewport.y as f64
                && my < (self.viewport.y + self.viewport.height) as f64;
            if in_viewport {
                gfx.renderer.handle_input(&mut self.input, dt);
            }
        }

        // ── 2. DRAW 3D + DRAW UI ─────────────────────────────────────────────
        let mut encoder = gfx.renderer.begin_frame();
        {
            let mut ctx = AppContext {
                input: &self.input,
                time,
                window_size: self.window_size,
                window,
                viewport: self.viewport,
                world: &mut self.world,
                renderer: None,
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

            // ── 3. RENDER FINAL ──────────────────────────────────────────────
            let frame = match gfx.surface.get_current_texture() {
                Ok(f) => f,
                // Outdated/Lost: the swapchain needs reconfiguring (e.g. window
                // minimised or resized race).  Skip this frame — the next
                // Resized event or WaitUntil wake-up will recover automatically.
                Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                    gfx.resize(self.window_size.0, self.window_size.1);
                    return;
                }
                // Out of memory or unknown driver error — nothing we can do.
                Err(e) => panic!("Surface error: {e:?}"),
            };
            let view = frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            gfx.renderer
                .render_to_view(&mut encoder, &view, Some(gui_batch), Some(text_batch));
            gfx.renderer.context.queue.submit(Some(encoder.finish()));
            frame.present();
        }

        // ── End-of-frame input cleanup ───────────────────────────────────────
        self.input.end_frame();
    }
}

impl<A: FerrousApp> ApplicationHandler for Runner<A> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.app.configure_ui(&mut self.ui);

        let attributes = Window::default_attributes()
            .with_title(&self.config.title)
            .with_resizable(self.config.resizable)
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
            self.config.sample_count,
        ));
        gfx.renderer.set_viewport(self.viewport);
        gfx.renderer
            .set_clear_color(self.config.background_color.to_wgpu());

        // Optional async font load
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

        // Call user setup — borrow ends before we move gfx into self.graphics
        {
            let time = self.clock.peek();
            let mut ctx = AppContext {
                input: &self.input,
                time,
                window_size: self.window_size,
                window: &window,
                viewport: self.viewport,
                world: &mut self.world,
                renderer: Some(&mut gfx.renderer),
                exit_requested: false,
            };
            self.app.setup(&mut ctx);
            self.viewport = ctx.viewport;
        }

        // Sync the GUI viewport widget immediately so camera input works on frame 1
        self.ui.set_viewport_rect(
            self.viewport.x as f32,
            self.viewport.y as f32,
            self.viewport.width as f32,
            self.viewport.height as f32,
        );

        self.window = Some(window.clone());
        self.graphics = Some(gfx);
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {}

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        // Track input/actions so we know when to stay awake vs. idle
        match event {
            WindowEvent::RedrawRequested => {} // Redraw itself doesn't count as an interactive event
            _ => self.last_action_time = Instant::now(),
        }

        // Let the GUI system see the event first
        self.ui.handle_window_event(&event, &mut self.input);

        // Forward to user callback
        if let Some(window) = self.window.clone() {
            let time = self.clock.peek();
            let mut ctx = AppContext {
                input: &self.input,
                time,
                window_size: self.window_size,
                window: &window,
                viewport: self.viewport,
                world: &mut self.world,
                renderer: self.graphics.as_mut().map(|g| &mut g.renderer),
                exit_requested: false,
            };
            self.app.on_window_event(&event, &mut ctx);
            if ctx.exit_requested {
                event_loop.exit();
                return;
            }
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                let new_size = (size.width, size.height);
                if let Some(gfx) = &mut self.graphics {
                    gfx.resize(size.width, size.height);
                    self.window_size = new_size;
                }
                // Notify the user app
                if let Some(window) = self.window.clone() {
                    if let Some(gfx) = &mut self.graphics {
                        let time = self.clock.peek();
                        let mut ctx = AppContext {
                            input: &self.input,
                            time,
                            window_size: new_size,
                            window: &window,
                            viewport: self.viewport,
                            world: &mut self.world,
                            renderer: Some(&mut gfx.renderer),
                            exit_requested: false,
                        };
                        self.app.on_resize(new_size, &mut ctx);
                        self.viewport = ctx.viewport;
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                // Guard: skip if we haven't reached the next frame deadline yet.
                // This prevents mouse events from sneaking in extra renders
                // between WaitUntil wake-ups.
                if let Some(target_fps) = self.config.target_fps {
                    let budget = Duration::from_secs_f64(1.0 / target_fps as f64);
                    if self.last_frame.elapsed() < budget {
                        return;
                    }
                }
                self.render_frame(event_loop);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // about_to_wait fires after every batch of events (including mouse
        // moves).  Do NOT render here — that would bypass the frame budget.
        //
        // Strategy:
        //  - If the frame deadline has arrived  → request_redraw() so winit
        //    emits RedrawRequested (where the actual render happens).
        //  - If the deadline is still in the future → WaitUntil(deadline) so
        //    the OS scheduler sleeps the thread; winit will call about_to_wait
        //    again when it fires, and we'll request_redraw() at that point.
        //
        // This combination keeps the CPU and GPU fully idle between frames
        // regardless of how many input events (mouse moves, etc.) arrive.
        let Some(window) = &self.window else { return };

        let is_idle = if let Some(timeout) = self.config.idle_timeout {
            Instant::now()
                .duration_since(self.last_action_time)
                .as_secs_f32()
                > timeout
        } else {
            false
        };

        if is_idle {
            // We've been idle for longer than the timeout.
            // Just wait for OS events, don't request a continuous redraw.
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }

        if let Some(target_fps) = self.config.target_fps {
            let budget = Duration::from_secs_f64(1.0 / target_fps as f64);
            let next_frame = self.last_frame + budget;
            if Instant::now() >= next_frame {
                window.request_redraw();
            } else {
                event_loop.set_control_flow(ControlFlow::WaitUntil(next_frame));
            }
        } else {
            // Unlimited FPS: always redraw immediately.
            window.request_redraw();
        }
    }
}

pub(crate) fn run_internal<A: FerrousApp + 'static>(config: AppConfig, app: A) {
    let mut runner = Runner::new(app, config);
    let event_loop = EventLoop::new().unwrap();
    // Wait = winit sleeps the thread between redraws; the runner wakes it
    // precisely via WaitUntil so the CPU and GPU idle between frames.
    event_loop.set_control_flow(ControlFlow::Wait);
    event_loop.run_app(&mut runner).unwrap();
}
