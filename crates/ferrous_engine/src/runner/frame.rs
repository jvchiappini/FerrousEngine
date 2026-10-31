//! Per-frame logic: `render_frame` and `about_to_wait`.

use std::time::Duration;

use ferrous_assets::AssetState;
use ferrous_core::glam::Vec3;
use ferrous_gui::GuiBatch;
use ferrous_ui_core::Rect;
use winit::event_loop::{ActiveEventLoop, ControlFlow};

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use crate::builder::AppMode;
use crate::context::AppContext;
use crate::render_context::RenderContext;
use crate::traits::{DrawContext, FerrousApp};

use super::types::Runner;

impl<A: FerrousApp + 'static> Runner<A> {
    /// Executes one full update + render cycle relying on external absolute time or clock elapsed time if interactive.
    pub fn render_frame(&mut self, event_loop: Option<&ActiveEventLoop>, t: f64) -> Option<Vec<u8>> {
        // wasm32: drain async GPU init result before anything else.
        #[cfg(target_arch = "wasm32")]
        {
            if self.graphics.is_none() {
                if let Some(slot) = &self.gfx_pending {
                    if slot.borrow().is_some() {
                        self.graphics = slot.borrow_mut().take();
                        self.gfx_pending = None;
                        if let (Some(gfx), Some(window)) = (&mut self.graphics, &self.window) {
                            #[cfg(target_arch = "wasm32")]
                            let (w, h) = super::canvas_physical_size(
                                window,
                                self.config.resolution_scale,
                            );
                            #[cfg(not(target_arch = "wasm32"))]
                            let (w, h) = {
                                let size = window.inner_size();
                                (size.width, size.height)
                            };
                            self.window_size = (w.max(1), h.max(1));
                            self.viewport = ferrous_core::Viewport {
                                x: 0,
                                y: 0,
                                width: self.window_size.0,
                                height: self.window_size.1,
                            };
                            gfx.resize(self.window_size.0, self.window_size.1);
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
                                requested_resolution_scale: None,
                                keep_awake_requested: false,
                                _gpu_backend: gfx.renderer.context.backend,
                                render: RenderContext::new(&mut gfx.renderer),
                                asset_server: &mut self.asset_server,
                            };
                            self.app.setup(&mut ctx);
                            self.viewport = ctx.viewport;
                            // self.ui.viewport = ctx.viewport;
                        }
                        if let Some(font_slot) = self.font_pending.take() {
                            if let Ok(mut borrow) = font_slot.try_borrow_mut() {
                                if let Some(font) = borrow.take() {
                                    self.font = Some(font);
                                }
                            }
                        }
                    } else {
                        return None;
                    }
                } else {
                    return None;
                }
            }
        }

        let (Some(gfx), Some(window)) = (&mut self.graphics, &self.window) else {
            return None;
        };

        // Immediate-mode 2D geometry is per-frame. The runner owns that
        // lifecycle and clears both 2D batchers once here, before
        // `update`/`draw_3d`/`draw_ui` push new geometry. Previously nothing
        // cleared them, so `draw_2d_shape`/`draw_2d_path` output accumulated
        // across frames ("ghosting"). `clear_2d()` is idempotent, so an
        // application that also clears at the start of its own frame is
        // unaffected; it can remove its manual call.
        gfx.renderer.clear_2d();

        // Wall-clock timing for the whole frame (update + draw + present).
        // Recorded on every return path below; `RenderContext::frame_ms()` and
        // `frame_ms_avg()` expose the result to consumers for logging/telemetry.
        let frame_start = Instant::now();

        // wasm32: proactive size sync as browser layout can be delayed/asynchronous.
        // Measure **the canvas element** (not window.innerWidth) so the buffer
        // is exactly the device-pixel box and the browser never rescales it.
        #[cfg(target_arch = "wasm32")]
        {
            let (phys_w, phys_h) =
                super::canvas_physical_size(window, self.config.resolution_scale);
            if phys_w > 0
                && phys_h > 0
                && (phys_w != self.window_size.0 || phys_h != self.window_size.1)
            {
                self.window_size = (phys_w, phys_h);
                self.viewport = ferrous_core::Viewport {
                    x: 0,
                    y: 0,
                    width: phys_w,
                    height: phys_h,
                };
                gfx.resize(phys_w, phys_h);
                gfx.renderer.set_viewport(self.viewport);
                super::set_canvas_backing_size(window, phys_w, phys_h);
            }
        }

        let now = Instant::now();
        self.last_frame = now;
        // Note: rendering a frame is not "activity" — only input events and
        // `AppContext::keep_awake` are, otherwise the idle timeout never fires.

        #[cfg(not(target_arch = "wasm32"))]
        if let Some(target_fps) = self.config.target_fps {
            let budget = Duration::from_secs_f64(1.0 / target_fps as f64);
            let next = match self.next_frame_deadline {
                Some(prev) => {
                    let candidate = prev + budget;
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

        // Check for async font completion (desktop only)
        #[cfg(not(target_arch = "wasm32"))]
        if self.font.is_none() {
            if let Some(handle) = self.font_asset_handle {
                if let AssetState::Ready(font_data) = self.asset_server.get(handle) {
                    let font = font_data.into_font(
                        &gfx.renderer.context.device,
                        &gfx.renderer.context.queue,
                        ferrous_font::latin_western(),
                    );
                    gfx.renderer
                        .set_font_atlas(&font.atlas.view, &font.atlas.sampler);
                    self.font = Some(font);
                    self.font_asset_handle = None;
                }
            }
        }

        self.asset_server.tick();

        // Advance ECS systems with deterministic timing
        let dt = if let Some(last_t) = self.last_deterministic_t {
            (t - last_t) as f32
        } else {
            0.016
        };
        self.last_deterministic_t = Some(t);
        
        let mut time_snapshot = self.clock.at_tick();
        time_snapshot.delta = dt;
        time_snapshot.elapsed = t;
        time_snapshot.fps = if dt > 0.0 { 1.0 / dt } else { 60.0 };

        self.resources.insert(time_snapshot);
        self.systems
            .run_all(&mut self.world.ecs, &mut self.resources);
        let time = time_snapshot;


        // ── 1. UPDATE ───────────────────────────────────────────────────────
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
                render: RenderContext::new(&mut gfx.renderer),
                exit_requested: false,
                requested_resolution_scale: None,
                keep_awake_requested: false,
                _gpu_backend: backend,
                asset_server: &mut self.asset_server,
            };

            self.app.update(&mut ctx);

            if ctx.keep_awake_requested {
                self.last_action_time = Instant::now();
            }

            // Live render-scale change: resize the surface next frame.
            if let Some(requested) = ctx.requested_resolution_scale.take() {
                self.config.resolution_scale = requested;
            }

            if ctx.exit_requested {
                drop(ctx);
                gfx.renderer
                    .record_frame_time(frame_start.elapsed().as_secs_f32() * 1000.0);
                if let Some(el) = event_loop {
                    el.exit();
                }
                return None;
            }

            if self.viewport != ctx.viewport {
                self.viewport = ctx.viewport;
                gfx.renderer.set_viewport(self.viewport);
            }
            self.ui.update(
                time.delta,
                self.window_size.0 as f32,
                self.window_size.1 as f32,
            );
        }

        // ECS → renderer sync and 3-D camera input are active in Game3D mode
        // OR if the renderer has been explicitly switched to Full3D (hybrid mode).
        let dt = time.delta;
        let renderer_mode = gfx.renderer.mode;
        
        if self.config.mode == AppMode::Game3D || renderer_mode == ferrous_renderer::RendererMode::Full3D {
            self.app.on_sync_world(&self.world);
            gfx.renderer.sync_world(&self.world);

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
        }

        // ── 2. DRAW ─────────────────────────────────────────────────────────
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
                render: RenderContext::new(&mut gfx.renderer),
                exit_requested: false,
                requested_resolution_scale: None,
                keep_awake_requested: false,
                _gpu_backend: backend,
                asset_server: &mut self.asset_server,
            };

            if self.viewport.width > 0
                && self.config.mode == AppMode::Game3D {
                    self.app.draw_3d(&mut ctx);
                }

            for gizmo in ctx.gizmos.drain(..) {
                ctx.render.inner.queue_gizmo(gizmo);
            }

            let mut gui_batch = GuiBatch::new();
            if let Some(font) = self.font.as_ref() {
                let mut dc = DrawContext {
                    gui: &mut gui_batch,
                    font,
                    ctx: &mut ctx,
                };
                self.app.draw_ui(&mut dc);
            }

            let viewport_rect = Rect {
                x: 0.0,
                y: 0.0,
                width: self.window_size.0 as f32,
                height: self.window_size.1 as f32,
            };

            let ui_render_batch = self.ui.render(viewport_rect, self.font.as_ref());
            gui_batch.extend(ui_render_batch);

            // ── 3. RENDER FINAL ─────────────────────────────────────────────
            let frame = match gfx.surface.get_current_texture() {
                Ok(f) => f,
                Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                    gfx.resize(self.window_size.0, self.window_size.1);
                    gfx.renderer
                        .record_frame_time(frame_start.elapsed().as_secs_f32() * 1000.0);
                    return None;
                }
                Err(e) => panic!("Surface error: {e:?}"),
            };
            let view = frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());

            gfx.renderer
                .render_to_view(&mut encoder, &view, Some(gui_batch));

            // Si el motor está en "Modo Exportación", inyectar la copia al readback buffer.
            if let Some(readback) = &gfx.renderer.readback_manager {
                readback.copy_to_buffer(&mut encoder, &frame.texture);
            }

            gfx.renderer.context.queue.submit(Some(encoder.finish()));
            
            frame.present();

            // wasm32: keep a throttled copy of the REAL canvas. `present()`
            // has already copied the rendered frame to the canvas, and the
            // browser has not cleared it yet (same task), so `toDataURL` sees
            // the actual image. AI snapshots use it when a later read-back
            // returns a cleared buffer. Same renderer, at most ~300 ms old.
            #[cfg(target_arch = "wasm32")]
            {
                use std::sync::atomic::{AtomicU64, Ordering};
                static LAST_CAPTURE_MS: AtomicU64 = AtomicU64::new(0);
                let now = js_sys::Date::now() as u64;
                if now.saturating_sub(LAST_CAPTURE_MS.load(Ordering::Relaxed)) >= 300 {
                    LAST_CAPTURE_MS.store(now, Ordering::Relaxed);
                    use winit::platform::web::WindowExtWebSys;
                    if let Some(canvas) = window.canvas() {
                        if let Ok(url) = canvas.to_data_url_with_type("image/jpeg") {
                            if let Some(win) = web_sys::window() {
                                let _ = js_sys::Reflect::set(
                                    win.as_ref(),
                                    &"__koncretaLastFrame".into(),
                                    &wasm_bindgen::JsValue::from_str(&url),
                                );
                            }
                        }
                    }
                }
            }

            // Frame complete (present done): publish the wall-clock duration.
            // Placement here also covers the export/readback path below, which
            // returns the mapped bytes directly.
            gfx.renderer
                .record_frame_time(frame_start.elapsed().as_secs_f32() * 1000.0);
               
            // Important: Clear damage regions after rendering
            self.ui.clear_damage();

            // Sincronizar y obtener los bytes si era una pasada de exportación.
            if let Some(readback) = &gfx.renderer.readback_manager {
                self.input.end_frame();
                #[cfg(not(target_arch = "wasm32"))]
                {
                    use pollster::FutureExt;
                    return readback.poll_and_map(&gfx.renderer.context.device).block_on().ok();
                }
                #[cfg(target_arch = "wasm32")]
                {
                    log::warn!("Blocking readback not supported on wasm32");
                    return None;
                }
            }
        }

        self.input.end_frame();
        None
    }

    /// Called by winit between frames to manage redraw scheduling.
    pub(super) fn about_to_wait_impl(&mut self, event_loop: &ActiveEventLoop) {
        let Some(window) = &self.window else { return };
        let now = Instant::now();

        let is_idle = match self.config.idle_timeout {
            Some(timeout) => now.duration_since(self.last_action_time).as_secs_f32() > timeout,
            None => false,
        };

        if is_idle {
            match self.config.idle_fps {
                // Throttled idle: keep drawing at a low rate.
                Some(fps) if fps > 0 => {
                    let next = self.last_frame + std::time::Duration::from_secs_f64(1.0 / fps as f64);
                    if now >= next {
                        window.request_redraw();
                    } else {
                        event_loop.set_control_flow(ControlFlow::WaitUntil(next));
                    }
                }
                // Full idle: sleep until the next input event.
                _ => event_loop.set_control_flow(ControlFlow::Wait),
            }
            return;
        }

        #[cfg(target_arch = "wasm32")]
        {
            // The browser paces frames (requestAnimationFrame).
            event_loop.set_control_flow(ControlFlow::Wait);
            window.request_redraw();
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(deadline) = self.next_frame_deadline {
                if now >= deadline {
                    window.request_redraw();
                } else {
                    event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
                }
            } else {
                window.request_redraw();
            }
        }
    }
}
