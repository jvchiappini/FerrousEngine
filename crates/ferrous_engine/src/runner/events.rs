//! `ApplicationHandler` implementation for `Runner` — window lifecycle and events.

use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::ActiveEventLoop,
    window::{Window, WindowId},
};

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;
#[cfg(target_arch = "wasm32")]
use std::rc::Rc;

use crate::builder::AppMode;
use crate::context::AppContext;
use crate::redraw::EngineEvent;
use crate::render_context::RenderContext;
use crate::traits::{FerrousApp, FileDropEvent};
#[cfg(target_arch = "wasm32")]
use ferrous_assets::Font;

use super::types::Runner;

impl<A: FerrousApp> Runner<A> {
    /// Window size in physical pixels for [`Self::resumed`].
    ///
    /// On the web the CSS size is multiplied by the device pixel ratio and the
    /// configured resolution scale so the surface matches physical pixels; a
    /// CSS-sized surface would be upscaled by the browser and look jagged.
    #[cfg(target_arch = "wasm32")]
    fn resumed_window_size(&self) -> (u32, u32) {
        let mut window_size = (self.config.width, self.config.height);
        if let Some(win) = web_sys::window() {
            let dpr = win.device_pixel_ratio();
            let scale = self.config.resolution_scale;
            let css_w = win
                .inner_width()
                .ok()
                .and_then(|v| v.as_f64())
                .unwrap_or(window_size.0 as f64);
            let css_h = win
                .inner_height()
                .ok()
                .and_then(|v| v.as_f64())
                .unwrap_or(window_size.1 as f64);
            let phys_w = (css_w * dpr * scale).round() as u32;
            let phys_h = (css_h * dpr * scale).round() as u32;
            window_size = (phys_w.max(1), phys_h.max(1));
            web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(&format!(
                "[WGPU-Init] DPR={:.2} Scale={:.2} CSS={}x{} Physical={}x{}",
                dpr, scale, css_w as u32, css_h as u32, phys_w, phys_h
            )));
        }
        window_size
    }

    /// Window size in physical pixels for [`Self::resumed`].
    #[cfg(not(target_arch = "wasm32"))]
    fn resumed_window_size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }
}

impl<A: FerrousApp + 'static> Runner<A> {
    /// Forwards one scale change to the app.
    ///
    /// The app gets the new scale and the suggested physical size once; the
    /// surface resize is the runner's job (desktop `Resized` / wasm canvas), so
    /// an app that ignores the callback still works.
    fn deliver_scale_change(&mut self, scale_factor: f64, size: (u32, u32)) {
        let Some(window) = self.window.clone() else {
            return;
        };
        let Some(gfx) = self.graphics.as_mut() else {
            return;
        };
        let waker = self.waker.clone();
        let (redraw, redraw_at) = {
            let backend = gfx.renderer.context.backend;
            let time = self.clock.peek();
            let mut ctx = AppContext {
                input: &self.input,
                time,
                window_size: size,
                window: &window,
                viewport: self.viewport,
                render_stats: ferrous_core::RenderStats::default(),
                camera_eye: ferrous_core::glam::Vec3::ZERO,
                camera_target: ferrous_core::glam::Vec3::ZERO,
                gizmos: Vec::new(),
                world: &mut self.world,
                render: RenderContext::new(&mut gfx.renderer),
                asset_server: &mut self.asset_server,
                exit_requested: false,
                requested_resolution_scale: None,
                keep_awake_requested: false,
                screenshot_requested: false,
                redraw_requested: false,
                redraw_at: None,
                waker: &waker,
                _gpu_backend: backend,
                scale_factor,
            };
            self.app
                .on_scale_factor_changed(scale_factor, size, &mut ctx);
            (ctx.redraw_requested, ctx.redraw_at)
        };
        if redraw {
            self.redraw.request();
        }
        if let Some(when) = redraw_at {
            self.redraw.request_at(when);
        }
    }
}

impl<A: FerrousApp + 'static> ApplicationHandler<EngineEvent> for Runner<A> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Creating a second window would start a second GPU state; resumed is
        // expected once per run on desktop.
        if self.window.is_some() {
            return;
        }
        self.app.configure_ui(&mut self.ui);

        let window_size = self.resumed_window_size();

        let attributes = Window::default_attributes()
            .with_title(&self.config.title)
            .with_resizable(self.config.resizable)
            .with_decorations(self.config.decorations)
            // Prompt 17 S3b: created hidden; shown when the first frame is
            // ready (or by the safety timeout). Never an unpainted window.
            // `AppConfig::visible == false` keeps it hidden forever.
            .with_visible(false)
            .with_inner_size(winit::dpi::PhysicalSize::new(window_size.0, window_size.1));

        let window = Arc::new(event_loop.create_window(attributes).unwrap());
        ferrous_core::startup::mark("engine: window created");
        self.window_size = window_size;
        self.viewport = ferrous_core::Viewport {
            x: 0,
            y: 0,
            width: window_size.0,
            height: window_size.1,
        };

        // ── Desktop: hidden window, GPU state on a worker ───────────────────
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut work = self.startup_work.take();
            let mut gate = super::show::StartupGate::new(
                super::show::resolved_timeout(&self.config),
                self.config.visible && self.config.show_window_when_ready,
                Instant::now(),
            );
            if self.config.visible && !gate.show_when_ready() {
                // Opt-out: the S3 behaviour, background colour first.
                crate::window_style::show_timeout(&window, self.config.background_color);
                ferrous_core::startup::mark("engine: window visible");
                gate.mark_shown_early();
            }
            // Instance, device and swapchain stay on this thread; the renderer
            // (the slow part) runs on a worker.
            let probe = work
                .as_mut()
                .and_then(crate::graphics::StartupWork::take_gpu)
                .and_then(crate::graphics::GpuProbeHandle::join);
            let init = crate::graphics::GpuInit::from_config(&self.config);
            let device = crate::graphics::GraphicsState::init_device(
                &window,
                &init,
                probe,
                !self.config.show_window_when_ready,
            );
            self.init = Some(super::show::InitJob::start(device));
            self.startup_work = work;
            self.startup = Some(gate);
            self.window = Some(window);
        }

        // ── Web (WASM): asynchronous init ───────────────────────────────────
        #[cfg(target_arch = "wasm32")]
        {
            window.set_visible(true);
            let slot = Rc::new(RefCell::new(None));
            let font_slot = Rc::new(RefCell::new(None));
            let slot_clone = slot.clone();
            let font_slot_clone = font_slot.clone();
            let window_clone = window.clone();
            let app_mode = self.config.mode;
            let bg = self.config.background_color.to_wgpu();
            let vp = self.viewport;
            let render_style = self.config.render_style.clone();
            let vsync = self.config.vsync;
            let samples = self.config.sample_count;
            let hdri_path = self.config.hdri_path.clone();
            let font_bytes = self.config.font_bytes;

            self.gfx_pending = Some(slot);
            wasm_bindgen_futures::spawn_local(async move {
                let mut gfx = crate::graphics::GraphicsState::new(
                    &window_clone,
                    vp.width,
                    vp.height,
                    vsync,
                    samples,
                    hdri_path.clone(),
                    crate::graphics::renderer_mode_from(app_mode),
                )
                .await;
                gfx.renderer.set_clear_color(bg);
                gfx.renderer.set_viewport(vp);
                gfx.renderer.set_render_style(render_style);
                if app_mode == AppMode::Flat2D {
                    gfx.renderer
                        .set_mode(ferrous_renderer::RendererMode::Flat2D);
                }

                if let Some(bytes) = font_bytes {
                    let font = Font::load_bytes(
                        bytes,
                        &gfx.renderer.context.device,
                        &gfx.renderer.context.queue,
                        ferrous_font::latin_western(),
                    );
                    gfx.renderer
                        .set_font_atlas(&font.atlas.view, &font.atlas.sampler);
                    ferrous_core::memory::register_gpu(
                        "font:ui atlas texture",
                        font.atlas.gpu_bytes(),
                        ferrous_core::memory::Subsystem::Fonts,
                    );
                    *font_slot_clone.borrow_mut() = Some(font);
                }

                *slot_clone.borrow_mut() = Some(gfx);
            });

            self.font_pending = Some(font_slot);
            self.window = Some(window.clone());
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {}

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: EngineEvent) {
        // A worker finished (or asked for a frame): in on-demand mode this is
        // what turns a background result into exactly one presented frame. It
        // is not an input, so it does not feed `input_to_present_ms`.
        self.redraw.on_input();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        // Prompt 18 P1: measure only real event callbacks. `RedrawRequested`
        // carries the whole frame, so bracketing it made the previous frame's
        // render time appear as the next frame's "event handling".
        let is_redraw = matches!(event, WindowEvent::RedrawRequested);
        let _event = (!is_redraw).then(ferrous_core::frame_trace::event_guard);
        // Prompt 17 S3b: user input waits for the first frame, then is
        // delivered in order.  Closing, destruction and redraws pass at once.
        #[cfg(not(target_arch = "wasm32"))]
        let event = match self.queue_while_starting(id, event) {
            Some(event) => event,
            None => return,
        };

        // Prompt 18 P2: the app sees the event before the built-in handling
        // and can ask for a frame only when the pointer changed something
        // visible. The pointer position is updated first so hover hit-testing
        // uses the event's position.
        // Prompt 18 P3: enter/leave are pointer events too. They change
        // nothing visible unless the app's hover actually changes, so they
        // follow the same opt-out as a move instead of forcing a frame.
        let requests_frame =
            crate::redraw::event_requests_frame(&event, self.config.redraw_on_pointer_move);
        if let WindowEvent::CursorMoved { position, .. } = &event {
            #[cfg(target_arch = "wasm32")]
            let (mx, my) = (
                position.x * self.config.resolution_scale,
                position.y * self.config.resolution_scale,
            );
            #[cfg(not(target_arch = "wasm32"))]
            let (mx, my) = (position.x, position.y);
            self.input.set_mouse_position(mx, my);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if !is_redraw {
            self.deliver_window_event(&event);
        }

        match event {
            WindowEvent::RedrawRequested => {
                let current_t = self.clock.peek().elapsed;
                let _ = self.render_frame(Some(event_loop), current_t);
                return;
            }
            _ => {
                self.last_action_time = Instant::now();
                // Prompt 18 P1: one frame to process the input. A pointer
                // move is opt-out: the app's `on_window_event` decides, and
                // `event_requests_frame` is the same decision the test drives.
                if requests_frame {
                    // Prompt 18 P6: the presented frame answers this input
                    // instant. Only an event that asks for a redraw counts:
                    // recording every event (a pointer move the app opted out
                    // of, a focus change, a worker wake) attributed stale
                    // instants to later timer-driven frames.
                    ferrous_core::frame_trace::record_input(Instant::now());
                    self.redraw.on_input();
                }
            }
        }

        match event {
            WindowEvent::CursorMoved { position, .. } => {
                let (mx, my) = (position.x, position.y);
                #[cfg(target_arch = "wasm32")]
                // On web the render buffer is `css × dpr × resolution_scale`
                // (see `resumed`), while winit reports the pointer in
                // `css × dpr` physical pixels. Scale the pointer into the
                // same buffer space as `window_size`, otherwise every
                // interaction is offset by the resolution_scale factor.
                let (mx, my) = (
                    mx * self.config.resolution_scale,
                    my * self.config.resolution_scale,
                );
                self.input.set_mouse_position(mx, my);
                // Raw input only: GUIMaker routes the pointer through its tree.
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = state == winit::event::ElementState::Pressed;
                self.input.update_mouse_button(button.into(), pressed);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                // LineDelta & PixelDelta: positive y = scroll up (wheel away from user / zoom in)
                let (dx, dy) = match delta {
                    winit::event::MouseScrollDelta::LineDelta(x, y) => (x, y),
                    winit::event::MouseScrollDelta::PixelDelta(pos) => {
                        let norm = 100.0_f32;
                        (pos.x as f32 / norm, pos.y as f32 / norm)
                    }
                };
                self.input.add_scroll(dx, dy);
            }
            WindowEvent::ModifiersChanged(mods) => {
                self.ctrl_held = mods.state().control_key();
                self.shift_held = mods.state().shift_key();
            }
            WindowEvent::KeyboardInput { ref event, .. } => {
                let winit::event::KeyEvent {
                    physical_key,
                    state,
                    ref text,
                    ..
                } = *event;
                if let winit::keyboard::PhysicalKey::Code(code) = physical_key {
                    self.input
                        .update_key(code.into(), state == winit::event::ElementState::Pressed);
                }

                if let Some(txt) = text {
                    if state == winit::event::ElementState::Pressed && !self.ctrl_held {
                        for c in txt.chars() {
                            self.input.push_char(c);
                        }
                    }
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                // When the browser zoom or display density changes, we must
                // recalculate the physical canvas size.
                log::info!("[Ferrous] Scale factor changed: {}", scale_factor);

                #[cfg(target_arch = "wasm32")]
                if let Some(window) = &self.window {
                    let (phys_w, phys_h) =
                        super::canvas_physical_size(window, self.config.resolution_scale);
                    if let Some(gfx) = &mut self.graphics {
                        gfx.resize(phys_w, phys_h);
                    }
                    self.window_size = (phys_w, phys_h);
                    super::set_canvas_backing_size(window, phys_w, phys_h);
                }
                // One event, one callback. The desktop surface follows through
                // the `Resized` the OS sends next; per-monitor DPI awareness is
                // what winit requests at event-loop creation (per-monitor v2 on
                // Windows), so the app only has to re-lay out in logical units.
                let size = self
                    .window
                    .as_ref()
                    .map(|window| {
                        let physical = window.inner_size();
                        (physical.width.max(1), physical.height.max(1))
                    })
                    .unwrap_or(self.window_size);
                self.scale_factor = scale_factor;
                self.deliver_scale_change(scale_factor, size);
            }
            WindowEvent::Resized(_size) => {
                #[cfg(target_arch = "wasm32")]
                let size = {
                    let (w, h) = self
                        .window
                        .as_ref()
                        .map(|window| {
                            super::canvas_physical_size(window, self.config.resolution_scale)
                        })
                        .unwrap_or((_size.width.max(1), _size.height.max(1)));
                    if let Some(window) = &self.window {
                        super::set_canvas_backing_size(window, w, h);
                    }
                    winit::dpi::PhysicalSize::new(w, h)
                };
                #[cfg(not(target_arch = "wasm32"))]
                let size = _size;

                if let Some(gfx) = &mut self.graphics {
                    gfx.resize(size.width, size.height);
                }
                self.window_size = (size.width, size.height);
            }
            WindowEvent::CloseRequested => {
                self.begin_close(Some(event_loop));
            }
            WindowEvent::HoveredFile(path) => {
                self.pending_file_drops.push(FileDropEvent::entered(path));
            }
            WindowEvent::HoveredFileCancelled => {
                if let Some(event) = self.pending_file_drops.last() {
                    if event.kind == crate::traits::FileDropKind::Entered {
                        let path = event.path.clone();
                        self.pending_file_drops.push(FileDropEvent::left(path));
                    }
                }
            }
            WindowEvent::DroppedFile(path) => {
                self.pending_file_drops.push(FileDropEvent::dropped(path));
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.about_to_wait_impl(event_loop);
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<A: FerrousApp + 'static> Runner<A> {
    /// Queues user input until the first frame is presented, or returns it for
    /// normal handling.
    fn queue_while_starting(&mut self, id: WindowId, event: WindowEvent) -> Option<WindowEvent> {
        let Some(gate) = self.startup.as_mut() else {
            return Some(event);
        };
        if gate.accepts_input() {
            return Some(event);
        }
        // Closing must work at any time, and redraws are what produce the
        // first frame.
        if matches!(
            &event,
            WindowEvent::CloseRequested | WindowEvent::Destroyed | WindowEvent::RedrawRequested
        ) {
            return Some(event);
        }
        gate.queue(id, event);
        None
    }

    /// Delivers one raw event to the app with a context, then applies any
    /// redraw request it made.
    fn deliver_window_event(&mut self, event: &WindowEvent) {
        let Some(window) = self.window.clone() else {
            return;
        };
        let Some(gfx) = self.graphics.as_mut() else {
            return;
        };
        let waker = self.waker.clone();
        let (redraw, redraw_at) = {
            let backend = gfx.renderer.context.backend;
            let time = self.clock.peek();
            let mut ctx = AppContext {
                input: &self.input,
                time,
                window_size: self.window_size,

                scale_factor: self.scale_factor,
                window: &window,
                viewport: self.viewport,
                render_stats: ferrous_core::RenderStats::default(),
                camera_eye: ferrous_core::glam::Vec3::ZERO,
                camera_target: ferrous_core::glam::Vec3::ZERO,
                gizmos: Vec::new(),
                world: &mut self.world,
                render: RenderContext::new(&mut gfx.renderer),
                asset_server: &mut self.asset_server,
                exit_requested: false,
                requested_resolution_scale: None,
                keep_awake_requested: false,
                screenshot_requested: false,
                redraw_requested: false,
                redraw_at: None,
                waker: &waker,
                _gpu_backend: backend,
            };
            self.app.on_window_event(event, &mut ctx);
            (ctx.redraw_requested, ctx.redraw_at)
        };
        if redraw {
            self.redraw.request();
        }
        if let Some(when) = redraw_at {
            self.redraw.request_at(when);
        }
    }

    /// Installs the GPU state produced by the worker and runs the app setup.
    fn install_gpu(&mut self, result: super::show::InitResult) {
        let Some(window) = self.window.clone() else {
            return;
        };
        let mut gfx = result.device.into_state(result.renderer);
        gfx.renderer.set_viewport(self.viewport);
        gfx.renderer
            .set_clear_color(self.config.background_color.to_wgpu());
        gfx.renderer.set_render_style(self.config.render_style);
        if self.config.mode == AppMode::Flat2D {
            gfx.renderer
                .set_mode(ferrous_renderer::RendererMode::Flat2D);
        }
        if let Some(config) = &self.config.font_cache {
            if let Some(cache) = ferrous_font::AtlasCache::from_config(config) {
                gfx.renderer.set_font_cache(std::sync::Arc::new(cache));
            }
        }
        if let Some(bytes) = self.config.font_bytes {
            // The UI font was baked on a worker while the GPU initialised; fall
            // back to the cache-backed bake when that failed.
            let baked = self.startup_work.take().and_then(|work| work.join_font());
            let registered = match baked {
                Some(baked) => gfx.renderer.register_baked_font_2d(baked),
                None => gfx
                    .renderer
                    .register_font_2d(bytes, ferrous_font::latin_western()),
            };
            match registered {
                Ok(id) => {
                    if let Some(font) = gfx.renderer.font_2d(id) {
                        gfx.renderer
                            .set_font_atlas(&font.atlas.view, &font.atlas.sampler);
                        ferrous_core::startup::mark("engine: fonts ready");
                        self.font = Some(font);
                    }
                }
                Err(error) => {
                    log::error!("the embedded UI font could not be registered: {error}");
                }
            }
        } else if let Some(path) = &self.config.font_path {
            let handle = self
                .asset_server
                .load::<ferrous_assets::font_importer::FontData>(path.as_str());
            self.font_asset_handle = Some(handle);
        }
        {
            let time = self.clock.peek();
            let backend = gfx.renderer.context.backend;
            let waker = self.waker.clone();
            let (redraw, redraw_at) = {
                let mut ctx = AppContext {
                    input: &self.input,
                    time,
                    window_size: self.window_size,

                    scale_factor: self.scale_factor,
                    window: &window,
                    render_stats: ferrous_core::RenderStats::default(),
                    camera_eye: ferrous_core::glam::Vec3::ZERO,
                    camera_target: ferrous_core::glam::Vec3::ZERO,
                    world: &mut self.world,
                    viewport: self.viewport,
                    gizmos: Vec::new(),
                    render: RenderContext::new(&mut gfx.renderer),
                    asset_server: &mut self.asset_server,
                    exit_requested: false,
                    requested_resolution_scale: None,
                    keep_awake_requested: false,
                    screenshot_requested: false,
                    redraw_requested: false,
                    redraw_at: None,
                    waker: &waker,
                    _gpu_backend: backend,
                };
                self.app.setup(&mut ctx);
                (ctx.redraw_requested, ctx.redraw_at)
            };
            if redraw {
                self.redraw.request();
            }
            if let Some(when) = redraw_at {
                self.redraw.request_at(when);
            }
        }
        ferrous_core::startup::mark("engine: app setup done");
        self.graphics = Some(gfx);
    }

    /// Polls the startup: installs the GPU state when the worker is done and
    /// enforces the show timeout. Returns whether the GPU state was installed
    /// on this call.
    pub(super) fn poll_startup(&mut self, event_loop: Option<&ActiveEventLoop>) -> bool {
        let mut installed = false;
        if let Some(job) = self.init.as_mut() {
            if let Some(result) = job.try_take() {
                self.init = None;
                self.install_gpu(result);
                installed = true;
            }
        }
        let (Some(gate), Some(window)) = (self.startup.as_mut(), self.window.as_ref()) else {
            return installed;
        };
        if self.config.visible && !gate.is_shown() && Instant::now() >= gate.deadline() {
            // The driver is slower than the timeout: show the app colour so the
            // user knows the app opened.
            crate::window_style::show_timeout(window, self.config.background_color);
            ferrous_core::startup::mark("engine: window visible (timeout)");
            gate.mark_timeout();
            let _ = event_loop;
        }
        installed
    }
}
