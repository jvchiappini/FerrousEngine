use ferrous_assets::Font;
use ferrous_core::{glam::Vec3, InputState, TimeClock, Viewport, World};
use ferrous_gui::{GuiBatch, TextBatch, Ui};
use std::sync::Arc;
use std::time::Duration;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

// â”€â”€â”€ Platform-specific Instant â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

// On wasm32 we need single-threaded shared ownership for the pending GPU state.
#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;
#[cfg(target_arch = "wasm32")]
use std::rc::Rc;

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
    /// Desktop-only: channel receiver for the async font-loading thread.
    #[cfg(not(target_arch = "wasm32"))]
    font_rx: Option<std::sync::mpsc::Receiver<Font>>,
    /// wasm32-only: receives the GraphicsState once the async GPU init completes.
    /// A spawn_local in resumed() fills this; render_frame drains it on the
    /// first frame after GPU init is done.
    #[cfg(target_arch = "wasm32")]
    gfx_pending: Option<Rc<RefCell<Option<GraphicsState>>>>,
    /// wasm32-only: receives the Font once the async GPU init completes.
    /// Populated in the same spawn_local as gfx_pending; drained in render_frame.
    #[cfg(target_arch = "wasm32")]
    font_pending: Option<Rc<RefCell<Option<Font>>>>,
    /// Timestamp of the last rendered frame, used to track activity.
    last_frame: Instant,
    /// Accumulated deadline for the next frame. Updated by adding the exact
    /// frame budget each tick so timing errors never accumulate (tick-based
    /// scheduling). `None` until the first frame is rendered.
    next_frame_deadline: Option<Instant>,
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
            #[cfg(not(target_arch = "wasm32"))]
            font_rx: None,
            #[cfg(target_arch = "wasm32")]
            gfx_pending: None,
            #[cfg(target_arch = "wasm32")]
            font_pending: None,
            last_frame: Instant::now(),
            next_frame_deadline: None,
            last_action_time: Instant::now(),
        }
    }

    /// Executes one full update + render cycle.  Called exclusively from
    /// `WindowEvent::RedrawRequested` so that input events (mouse moves, etc.)
    /// never trigger extra renders outside the frame budget.
    fn render_frame(&mut self, event_loop: &ActiveEventLoop) {
        // wasm32: drain async GPU init result before anything else.
        // Must run before `self.graphics` is mutably borrowed below.
        #[cfg(target_arch = "wasm32")]
        {
            if self.graphics.is_none() {
                if let Some(slot) = &self.gfx_pending {
                    if slot.borrow().is_some() {
                        self.graphics = slot.borrow_mut().take();
                        self.gfx_pending = None;
                        if let (Some(gfx), Some(window)) = (&mut self.graphics, &self.window) {
                            gfx.renderer.set_viewport(self.viewport);
                            let time = self.clock.peek();
                            let mut ctx = AppContext {
                                input: &self.input,
                                time,
                                window_size: self.window_size,
                                window,
                                viewport: self.viewport,
                                render_stats: Default::default(),
                                camera_eye: Vec3::ZERO,
                                camera_target: Vec3::ZERO,
                                gizmos: Vec::new(),
                                world: &mut self.world,
                                exit_requested: false,
                                _gpu_backend: gfx.renderer.context.backend,
                                renderer: &mut gfx.renderer,
                            };
                            self.app.setup(&mut ctx);
                            self.viewport = ctx.viewport;
                            self.ui.set_viewport_rect(
                                self.viewport.x as f32,
                                self.viewport.y as f32,
                                self.viewport.width as f32,
                                self.viewport.height as f32,
                            );
                        }
                        // Drain the font slot that was filled in the same async block.
                        if let Some(font_slot) = self.font_pending.take() {
                            if let Ok(mut borrow) = font_slot.try_borrow_mut() {
                                if let Some(font) = borrow.take() {
                                    self.font = Some(font);
                                }
                            }
                        }
                    } else {
                        return; // GPU not ready yet
                    }
                } else {
                    return; // no pending init and no graphics
                }
            }
        }

        let (Some(gfx), Some(window)) = (&mut self.graphics, &self.window) else {
            return;
        };

        // Stamp the frame start for the next deadline calculation.
        let now = Instant::now();
        self.last_frame = now;
        self.last_action_time = now;
        // Advance the accumulated deadline by exactly one budget tick so that
        // timing errors never pile up (tick-based scheduler).
        // If we're more than one full budget behind (e.g. after a stall), clamp
        // to now so we don't schedule a burst of catch-up frames.
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(target_fps) = self.config.target_fps {
            let budget = Duration::from_secs_f64(1.0 / target_fps as f64);
            let next = match self.next_frame_deadline {
                Some(prev) => {
                    let candidate = prev + budget;
                    // If we've fallen more than one budget behind, reset to now
                    // to avoid a burst of immediate redraws.
                    if candidate < now {
                        now + budget
                    } else {
                        candidate
                    }
                }
                None => now + budget,
            };
            self.next_frame_deadline = Some(next);
        }

        // Check for async font completion (desktop only; wasm32 loads synchronously)
        #[cfg(not(target_arch = "wasm32"))]
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

        // â”€â”€ 1. UPDATE â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
        {
            let backend = gfx.renderer.context.backend;
            let mut ctx = AppContext {
                input: &self.input,
                time,
                window_size: self.window_size,
                window,
                viewport: self.viewport,
                render_stats: Default::default(),
                camera_eye: Vec3::ZERO,
                camera_target: Vec3::ZERO,
                gizmos: Vec::new(),
                world: &mut self.world,
                renderer: &mut gfx.renderer,
                exit_requested: false,
                _gpu_backend: backend,
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

        // â”€â”€ Auto world â†’ renderer sync â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
        gfx.renderer.sync_world(&self.world);

        // â”€â”€ 3D camera input â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
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

        // â”€â”€ 2. DRAW 3D + DRAW UI â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
        let mut encoder = gfx.renderer.begin_frame();
        {
            let render_stats = gfx.renderer.render_stats;
            let camera_eye = gfx.renderer.camera().eye;
            let camera_target = gfx.renderer.camera().target;
            let backend = gfx.renderer.context.backend;
            let mut ctx = AppContext {
                input: &self.input,
                time,
                window_size: self.window_size,
                window,
                viewport: self.viewport,
                render_stats,
                camera_eye,
                camera_target,
                gizmos: Vec::new(),
                world: &mut self.world,
                renderer: &mut gfx.renderer,
                exit_requested: false,
                _gpu_backend: backend,
            };

            if self.viewport.width > 0 {
                self.app.draw_3d(&mut ctx);
            }

            // Drain any gizmos queued by draw_3d into the renderer.
            for gizmo in ctx.gizmos.drain(..) {
                // use the mutable borrow already held inside ctx
                ctx.renderer.queue_gizmo(gizmo);
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

            // â”€â”€ 3. RENDER FINAL â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
            let frame = match gfx.surface.get_current_texture() {
                Ok(f) => f,
                // Outdated/Lost: the swapchain needs reconfiguring (e.g. window
                // minimised or resized race).  Skip this frame â€” the next
                // Resized event or WaitUntil wake-up will recover automatically.
                Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                    gfx.resize(self.window_size.0, self.window_size.1);
                    return;
                }
                // Out of memory or unknown driver error â€” nothing we can do.
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

        // â”€â”€ End-of-frame input cleanup â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
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

        // â”€â”€ Desktop: synchronous blocking GPU init â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut gfx = pollster::block_on(GraphicsState::new(
                &window,
                self.config.width,
                self.config.height,
                self.config.vsync,
                self.config.sample_count,
                self.config.hdri_path.clone(),
            ));
            gfx.renderer.set_viewport(self.viewport);
            gfx.renderer
                .set_clear_color(self.config.background_color.to_wgpu());

            // Font loading: font_bytes takes priority (works on all platforms).
            if let Some(bytes) = self.config.font_bytes {
                let font = Font::load_bytes(
                    bytes,
                    &gfx.renderer.context.device,
                    &gfx.renderer.context.queue,
                    ' '..'~',
                );
                gfx.renderer
                    .set_font_atlas(&font.atlas.view, &font.atlas.sampler);
                self.font = Some(font);
            } else if let Some(path) = &self.config.font_path {
                let device = gfx.renderer.context.device.clone();
                let queue = gfx.renderer.context.queue.clone();
                let path = path.clone();
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    let _ = tx.send(Font::load(&path, &device, &queue, ' '..'~'));
                });
                self.font_rx = Some(rx);
            }

            // Call user setup
            {
                let time = self.clock.peek();
                let backend = gfx.renderer.context.backend;
                let mut ctx = AppContext {
                    input: &self.input,
                    time,
                    window_size: self.window_size,
                    window: &window,
                    viewport: self.viewport,
                    render_stats: Default::default(),
                    camera_eye: Vec3::ZERO,
                    camera_target: Vec3::ZERO,
                    gizmos: Vec::new(),
                    world: &mut self.world,
                    renderer: &mut gfx.renderer,
                    exit_requested: false,
                    _gpu_backend: backend,
                };
                self.app.setup(&mut ctx);
                self.viewport = ctx.viewport;
            }

            self.ui.set_viewport_rect(
                self.viewport.x as f32,
                self.viewport.y as f32,
                self.viewport.width as f32,
                self.viewport.height as f32,
            );

            self.window = Some(window.clone());
            self.graphics = Some(gfx);
        }

        // â”€â”€ wasm32: async GPU init via spawn_local â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
        // We cannot block on the GPU future here, so we spawn it and store
        // the result in a shared Rc<RefCell<Option<GraphicsState>>>.  The
        // render_frame loop drains it on the first frame after it's ready.
        #[cfg(target_arch = "wasm32")]
        {
            let slot: Rc<RefCell<Option<GraphicsState>>> = Rc::new(RefCell::new(None));
            let slot_clone = slot.clone();
            self.gfx_pending = Some(slot);

            let w = self.config.width;
            let h = self.config.height;
            let vsync = self.config.vsync;
            let samples = self.config.sample_count;
            let bg = self.config.background_color.to_wgpu();
            let vp = self.viewport;
            let font_bytes = self.config.font_bytes;
            let hdri_path = self.config.hdri_path.clone();

            // Clone the font out of the runner so we can set it from the async block.
            // We use a second Rc<RefCell> slot to pass the font back.
            let font_slot: Rc<RefCell<Option<Font>>> = Rc::new(RefCell::new(None));
            let font_slot_clone = font_slot.clone();

            let window_for_closure = window.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let mut gfx = GraphicsState::new(
                    &window_for_closure,
                    w,
                    h,
                    vsync,
                    samples,
                    // pass HDRI path along (clone moved above)
                    hdri_path.clone(),
                )
                .await;
                gfx.renderer.set_clear_color(bg);
                gfx.renderer.set_viewport(vp);

                if let Some(bytes) = font_bytes {
                    let font = Font::load_bytes(
                        bytes,
                        &gfx.renderer.context.device,
                        &gfx.renderer.context.queue,
                        ' '..'~',
                    );
                    gfx.renderer
                        .set_font_atlas(&font.atlas.view, &font.atlas.sampler);
                    *font_slot_clone.borrow_mut() = Some(font);
                }

                *slot_clone.borrow_mut() = Some(gfx);
            });

            // Store the font slot so render_frame can drain it once the async
            // block has finished and stored the Font into font_slot_clone.
            self.font_pending = Some(font_slot);

            // Store the window; setup will be called in render_frame
            // once graphics is available (graphics.is_none() guard).
            self.window = Some(window.clone());
        }
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
            let backend = self
                .graphics
                .as_ref()
                .map(|g| g.renderer.context.backend)
                .unwrap_or(wgpu::Backend::Empty);
            let mut ctx = AppContext {
                input: &self.input,
                time,
                window_size: self.window_size,
                window: &window,
                viewport: self.viewport,
                render_stats: Default::default(),
                camera_eye: Vec3::ZERO,
                camera_target: Vec3::ZERO,
                gizmos: Vec::new(),
                world: &mut self.world,
                renderer: &mut self.graphics.as_mut().unwrap().renderer,
                exit_requested: false,
                _gpu_backend: backend,
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
                    if self.graphics.is_some() {
                        let time = self.clock.peek();
                        let backend = self
                            .graphics
                            .as_ref()
                            .map(|g| g.renderer.context.backend)
                            .unwrap_or(wgpu::Backend::Empty);
                        let mut ctx = AppContext {
                            input: &self.input,
                            time,
                            window_size: new_size,
                            window: &window,
                            viewport: self.viewport,
                            render_stats: Default::default(),
                            camera_eye: Vec3::ZERO,
                            camera_target: Vec3::ZERO,
                            gizmos: Vec::new(),
                            world: &mut self.world,
                            renderer: &mut self.graphics.as_mut().unwrap().renderer,
                            exit_requested: false,
                            _gpu_backend: backend,
                        };
                        self.app.on_resize(new_size, &mut ctx);
                        self.viewport = ctx.viewport;
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                self.render_frame(event_loop);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // On wasm32: request_redraw() maps to requestAnimationFrame, which the
        // browser fires at the monitor refresh rate. Call it once; browser handles cadence.
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            return;
        }

        // Desktop: precise frame-budget + idle sleep logic.
        #[cfg(not(target_arch = "wasm32"))]
        {
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
                event_loop.set_control_flow(ControlFlow::Wait);
                return;
            }

            if let Some(deadline) = self.next_frame_deadline {
                if Instant::now() >= deadline {
                    window.request_redraw();
                } else {
                    event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
                }
            } else if self.config.target_fps.is_some() {
                // First frame: deadline not yet set, request immediately.
                window.request_redraw();
            } else {
                window.request_redraw();
            }
        }
    }
}

/// Desktop entry point: blocks the calling thread running the event loop.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn run_internal<A: FerrousApp + 'static>(config: AppConfig, app: A) {
    let mut runner = Runner::new(app, config);
    let event_loop = EventLoop::new().unwrap();
    // Wait = winit sleeps the thread between redraws; the runner wakes it
    // precisely via WaitUntil so the CPU and GPU idle between frames.
    event_loop.set_control_flow(ControlFlow::Wait);
    event_loop.run_app(&mut runner).unwrap();
}

/// wasm32 entry point: runs Runner directly in the browser event loop.
/// Window created in resumed(), GPU init async via spawn_local.
/// We use ControlFlow::Wait (same as desktop) so that about_to_wait's
/// request_redraw() maps to requestAnimationFrame, letting the browser
/// sync rendering to its own vsync/refresh-rate scheduler instead of
/// busy-looping with Poll.
#[cfg(target_arch = "wasm32")]
pub(crate) fn run_internal<A: FerrousApp + 'static>(config: AppConfig, app: A) {
    console_error_panic_hook::set_once();
    let mut runner = Runner::new(app, config);
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);
    event_loop.run_app(&mut runner).unwrap();
}
