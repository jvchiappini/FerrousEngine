//! Per-frame logic: `render_frame` and `about_to_wait`.

use std::sync::Arc;
use std::time::Duration;

use ferrous_assets::AssetState;
use ferrous_core::frame_trace::{self, Stage};
use ferrous_core::glam::Vec3;
use ferrous_core::startup;
use ferrous_core::{RenderStats, Time, Viewport};
use ferrous_ui_core::Rect;
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::window::Window;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use crate::builder::AppMode;
use crate::traits::{DrawContext, FerrousApp};

use super::types::Runner;

/// What `FerrousApp::update` (and the callbacks it drives) asked the runner to
/// do once the call returns.
#[allow(clippy::struct_excessive_bools)] // reason: four independent one-shot requests
struct UpdateOutcome {
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))] // no readback on the web
    screenshot: bool,
    redraw_request: bool,
    redraw_at: Option<Instant>,
    keep_awake: bool,
    resolution_scale: Option<f64>,
    exit: bool,
    viewport: Viewport,
}

/// What the draw callbacks produced.
struct DrawOutcome {
    gui_batch: ferrous_gui::GuiBatch,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))] // no readback on the web
    screenshot: bool,
    redraw_request: bool,
    redraw_at: Option<Instant>,
}

impl<A: FerrousApp + 'static> Runner<A> {
    /// Runs `update`, the file drops and everything else that needs the app,
    /// through the one `AppContext` builder.
    fn run_update(&mut self, window: &Arc<Window>, time: Time) -> Option<UpdateOutcome> {
        let size = self.window_size;
        let scale = self.scale_factor;
        let drops = std::mem::take(&mut self.pending_file_drops);
        let (screenshot, redraw_request, redraw_at, keep_awake, resolution_scale, exit, viewport) =
            self.with_ctx(
                window,
                time,
                RenderStats::default(),
                Vec3::ZERO,
                Vec3::ZERO,
                size,
                scale,
                move |app, ctx| {
                    let update_stage = frame_trace::stage(Stage::AppUpdate);
                    app.update(ctx);
                    drop(update_stage);
                    for event in drops {
                        app.on_file_drop(event, ctx);
                    }
                    (
                        ctx.screenshot_requested,
                        ctx.redraw_requested,
                        ctx.redraw_at,
                        ctx.keep_awake_requested,
                        ctx.requested_resolution_scale.take(),
                        ctx.exit_requested,
                        ctx.viewport,
                    )
                },
            )?;
        Some(UpdateOutcome {
            screenshot,
            redraw_request,
            redraw_at,
            keep_awake,
            resolution_scale,
            exit,
            viewport,
        })
    }

    /// Runs `draw_3d`/`draw_ui` through the one `AppContext` builder and
    /// returns the batch they filled.
    fn run_draw(&mut self, window: &Arc<Window>, time: Time) -> Option<DrawOutcome> {
        let (render_stats, camera_eye, camera_target) = {
            let gfx = self.graphics.as_mut()?;
            (
                gfx.renderer.render_stats,
                gfx.renderer.camera().eye,
                gfx.renderer.camera().target,
            )
        };
        let mut gui_batch = {
            let gfx = self.graphics.as_mut()?;
            // Reuse the batch the renderer reclaimed last frame. Clearing it
            // keeps every primitive Vec's capacity, so the chrome's steady
            // state does not allocate (measured by Motioner's alloc counter).
            let mut batch = gfx.renderer.take_recycled_ui_batch().unwrap_or_default();
            batch.clear();
            batch
        };
        let mode = self.config.mode;
        let viewport = self.viewport;
        let size = self.window_size;
        let scale = self.scale_factor;
        let (gui_batch, screenshot, redraw_request, redraw_at) = self.with_ctx(
            window,
            time,
            render_stats,
            camera_eye,
            camera_target,
            size,
            scale,
            move |app, ctx| {
                let _collect = frame_trace::stage(Stage::CommandCollection);
                if viewport.width > 0 && mode == AppMode::Game3D {
                    app.draw_3d(ctx);
                }

                for gizmo in ctx.gizmos.drain(..) {
                    ctx.render.inner.queue_gizmo(gizmo);
                }

                if let Some(font) = ctx.font {
                    let mut dc = DrawContext {
                        gui: &mut gui_batch,
                        font,
                        ctx,
                    };
                    app.draw_ui(&mut dc);
                }
                (
                    gui_batch,
                    ctx.screenshot_requested,
                    ctx.redraw_requested,
                    ctx.redraw_at,
                )
            },
        )?;
        Some(DrawOutcome {
            gui_batch,
            screenshot,
            redraw_request,
            redraw_at,
        })
    }

    /// Executes one full update + render cycle relying on external absolute time or clock elapsed time if interactive.
    pub fn render_frame(
        &mut self,
        event_loop: Option<&ActiveEventLoop>,
        t: f64,
    ) -> Option<Vec<u8>> {
        // Prompt 17 S3b: install the GPU state as soon as the worker has it (and
        // show the window if the timeout expired).
        #[cfg(not(target_arch = "wasm32"))]
        let _ = self.poll_startup(event_loop);

        // wasm32: drain async GPU init result before anything else.
        #[cfg(target_arch = "wasm32")]
        {
            if self.graphics.is_none() {
                let Some(slot) = self.gfx_pending.clone() else {
                    return None;
                };
                if slot.borrow().is_some() {
                    self.graphics = slot.borrow_mut().take();
                    self.gfx_pending = None;
                    if self.graphics.is_some() && self.window.is_some() {
                        let Some(window) = self.window.clone() else {
                            return None;
                        };
                        #[cfg(target_arch = "wasm32")]
                        let (w, h) =
                            super::canvas_physical_size(&window, self.config.resolution_scale);
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
                        if let Some(gfx) = self.graphics.as_mut() {
                            gfx.resize(self.window_size.0, self.window_size.1);
                            gfx.renderer.set_viewport(self.viewport);
                        }
                        let time = self.clock.peek();
                        let size = self.window_size;
                        let scale = self.scale_factor;
                        let viewport = self.with_ctx(
                            &window,
                            time,
                            RenderStats::default(),
                            Vec3::ZERO,
                            Vec3::ZERO,
                            size,
                            scale,
                            |app, ctx| {
                                app.setup(ctx);
                                ctx.viewport
                            },
                        );
                        if let Some(viewport) = viewport {
                            self.viewport = viewport;
                        }
                    }
                    if let Some(font_slot) = self.font_pending.take() {
                        if let Ok(mut borrow) = font_slot.try_borrow_mut() {
                            if let Some(font) = borrow.take() {
                                self.font = Some(std::sync::Arc::new(font));
                            }
                        }
                    }
                } else {
                    return None;
                }
            }
        }

        if self.graphics.is_none() || self.window.is_none() {
            return None;
        }
        let window = self.window.clone()?;

        // Immediate-mode 2D geometry is per-frame. The runner owns that
        // lifecycle and clears both 2D batchers once here, before
        // `update`/`draw_3d`/`draw_ui` push new geometry. Previously nothing
        // cleared them, so `draw_2d_shape`/`draw_2d_path` output accumulated
        // across frames ("ghosting"). `clear_2d()` is idempotent, so an
        // application that also clears at the start of its own frame is
        // unaffected; it can remove its manual call.
        if let Some(gfx) = self.graphics.as_mut() {
            gfx.renderer.clear_2d();
        }

        // Prompt 18 P0: per-frame stage trace.  The guard closes the frame on
        // every return path below; it is a no-op unless FERROUS_TRACE_FRAMES
        // is enabled.
        let _frame = frame_trace::begin_frame();
        // Prompt 18 P1: this frame answers any queued input/redraw request.
        self.redraw.begin_frame();

        // Wall-clock timing for the whole frame (update + draw + present).
        // Recorded on every return path below; `RenderContext::frame_ms()` and
        // `frame_ms_avg()` expose the result to consumers for logging/telemetry.
        let frame_start = Instant::now();

        // wasm32: proactive size sync as browser layout can be delayed/asynchronous.
        // Measure **the canvas element** (not window.innerWidth) so the buffer
        // is exactly the device-pixel box and the browser never rescales it.
        #[cfg(target_arch = "wasm32")]
        if let Some(gfx) = self.graphics.as_mut() {
            let (phys_w, phys_h) =
                super::canvas_physical_size(&window, self.config.resolution_scale);
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
                super::set_canvas_backing_size(&window, phys_w, phys_h);
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
                    if let Some(gfx) = self.graphics.as_mut() {
                        let font = font_data.into_font(
                            &gfx.renderer.context.device,
                            &gfx.renderer.context.queue,
                            ferrous_font::latin_western(),
                        );
                        startup::mark("engine: fonts ready");
                        gfx.renderer
                            .set_font_atlas(&font.atlas.view, &font.atlas.sampler);
                        ferrous_core::memory::register_gpu(
                            "font:ui atlas texture",
                            font.atlas.gpu_bytes(),
                            ferrous_core::memory::Subsystem::Fonts,
                        );
                        self.font = Some(std::sync::Arc::new(font));
                    }
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
        let update = self.run_update(&window, time)?;
        if update.keep_awake {
            self.last_action_time = Instant::now();
        }
        // Live render-scale change: resize the surface next frame.
        if let Some(requested) = update.resolution_scale {
            self.config.resolution_scale = requested;
            self.redraw.request();
        }
        if update.exit {
            if let Some(gfx) = self.graphics.as_mut() {
                gfx.renderer
                    .record_frame_time(frame_start.elapsed().as_secs_f32() * 1000.0);
            }
            self.begin_close(event_loop);
            return None;
        }
        if self.viewport != update.viewport {
            self.viewport = update.viewport;
            if let Some(gfx) = self.graphics.as_mut() {
                gfx.renderer.set_viewport(self.viewport);
            }
        }
        let mut redraw_request = update.redraw_request;
        let mut redraw_at = update.redraw_at;
        {
            let _layout = frame_trace::stage(Stage::UiLayout);
            self.ui.update(
                time.delta,
                self.window_size.0 as f32,
                self.window_size.1 as f32,
            );
        }

        // Runtime lazy fallback: the text of this frame asked for characters the
        // atlas lacks. Re-bake once, *before* `draw_ui` encodes UVs, so the frame
        // that is painted matches the atlas it uses.
        if !self.glyph_requests.is_empty() {
            let requested = std::mem::take(&mut self.glyph_requests);
            if let Some(runtime) = self.runtime_font.as_mut() {
                if let Some(gfx) = self.graphics.as_mut() {
                    let device = gfx.renderer.context.device.clone();
                    let queue = gfx.renderer.context.queue.clone();
                    if runtime.ensure(&requested, &device, &queue) {
                        let font = runtime.font().clone();
                        gfx.renderer
                            .set_font_atlas(&font.atlas.view, &font.atlas.sampler);
                        self.font = Some(font);
                        redraw_request = true;
                    }
                }
            }
        }

        // ECS → renderer sync and 3-D camera input are active in Game3D mode
        // OR if the renderer has been explicitly switched to Full3D (hybrid mode).
        let dt = time.delta;
        let renderer_mode = self.graphics.as_ref()?.renderer.mode;
        if self.config.mode == AppMode::Game3D
            || renderer_mode == ferrous_renderer::RendererMode::Full3D
        {
            self.app.on_sync_world(&self.world);
            let gfx = self.graphics.as_mut()?;
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
        let mut encoder = self.graphics.as_mut()?.renderer.begin_frame();
        let draw = self.run_draw(&window, time)?;
        let mut gui_batch = draw.gui_batch;
        redraw_request |= draw.redraw_request;
        if let Some(when) = draw.redraw_at {
            redraw_at = Some(redraw_at.map_or(when, |current| current.min(when)));
        }

        {
            let viewport_rect = Rect {
                x: 0.0,
                y: 0.0,
                width: self.window_size.0 as f32,
                height: self.window_size.1 as f32,
            };

            let _batch = frame_trace::stage(Stage::BatchBuild);
            self.ui
                .render_into(&mut gui_batch, viewport_rect, self.font.as_deref());
            drop(_batch);

            // A screenshot asked for during `update` or `draw_ui` is copied on
            // the frame that is about to be presented, so it shows exactly
            // what the user sees.  Blocking readback is desktop-only.
            #[cfg(target_arch = "wasm32")]
            let want_screenshot = false;
            #[cfg(not(target_arch = "wasm32"))]
            let want_screenshot = screenshot_wanted(
                update.screenshot,
                draw.screenshot,
                self.graphics
                    .as_ref()
                    .is_some_and(|gfx| gfx.config.usage.contains(wgpu::TextureUsages::COPY_SRC)),
            );

            let gfx = self.graphics.as_mut()?;
            #[cfg(not(target_arch = "wasm32"))]
            let mut first_frame_shown = false;

            // ── 3. RENDER FINAL ─────────────────────────────────────────────
            let frame = {
                let _wait = frame_trace::stage(Stage::PresentWait);
                match gfx.surface.get_current_texture() {
                    Ok(f) => f,
                    Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                        gfx.resize(self.window_size.0, self.window_size.1);
                        gfx.renderer
                            .record_frame_time(frame_start.elapsed().as_secs_f32() * 1000.0);
                        // The new swapchain has not presented yet; in on-demand
                        // mode nobody else would ask for that frame.
                        self.redraw.request();
                        return None;
                    }
                    Err(e) => panic!("Surface error: {e:?}"),
                }
            };
            let view = frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());

            // Prompt 18 P7: opt-in persistent off-screen present.  When the GUI
            // damage stays under the break-even, render composition + GUI into
            // a persistent texture and present that texture with a full-image
            // copy/blit.  The whole frame is still repainted, so this is
            // pixel-identical; it is the infrastructure a partial update needs.
            let window_area = f64::from(self.window_size.0) * f64::from(self.window_size.1);
            let damage_fraction = match gui_batch.damage_union {
                Some(damage) if window_area > 0.0 => (f64::from(damage.width.max(0.0))
                    * f64::from(damage.height.max(0.0))
                    / window_area)
                    .clamp(0.0, 1.0) as f32,
                _ => 0.0,
            };
            let use_persistent_ui = self.config.persistent_ui
                && self.config.mode != AppMode::Game3D
                && self.config.sample_count == 1
                && damage_fraction <= self.config.persistent_ui_break_even.clamp(0.0, 1.0);

            if use_persistent_ui {
                let format = gfx.config.format;
                let device = gfx.renderer.context.device.clone();
                if self.ui_presenter.as_ref().map(|p| p.format()) != Some(format) {
                    self.ui_presenter = Some(ferrous_ui_render::UiPresenter::new(&device, format));
                }
                let presenter = self.ui_presenter.as_mut().expect("just created");
                presenter.ensure(&device, self.window_size.0, self.window_size.1);
                match presenter.view() {
                    Some(target_view) => {
                        gfx.renderer
                            .render_to_view(&mut encoder, target_view, Some(gui_batch));
                        if gfx.config.usage.contains(wgpu::TextureUsages::COPY_DST) {
                            presenter.copy_full(&mut encoder, &frame.texture);
                        } else {
                            presenter.blit(&mut encoder, &view);
                        }
                    }
                    // A zero-sized frame has no off-screen target; use the
                    // direct path so nothing is skipped.
                    None => gfx
                        .renderer
                        .render_to_view(&mut encoder, &view, Some(gui_batch)),
                }
            } else {
                gfx.renderer
                    .render_to_view(&mut encoder, &view, Some(gui_batch));
            }

            // Si el motor está en "Modo Exportación", inyectar la copia al readback buffer.
            if let Some(readback) = &gfx.renderer.readback_manager {
                readback.copy_to_buffer(&mut encoder, &frame.texture);
            }

            let capture = want_screenshot.then(|| {
                let (width, height) = self.window_size;
                let buffer =
                    crate::screenshot::capture_buffer(&gfx.renderer.context.device, width, height);
                let padded = crate::screenshot::padded_bytes_per_row(width);
                encoder.copy_texture_to_buffer(
                    wgpu::ImageCopyTexture {
                        texture: &frame.texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::ImageCopyBuffer {
                        buffer: &buffer,
                        layout: wgpu::ImageDataLayout {
                            offset: 0,
                            bytes_per_row: Some(padded),
                            rows_per_image: Some(height.max(1)),
                        },
                    },
                    wgpu::Extent3d {
                        width: width.max(1),
                        height: height.max(1),
                        depth_or_array_layers: 1,
                    },
                );
                buffer
            });
            let _submit = frame_trace::stage(Stage::Submit);
            frame_trace::record_submit();
            gfx.renderer.context.queue.submit(Some(encoder.finish()));
            drop(_submit);
            // Prompt 18 P0: start the non-blocking GPU timestamp readback.
            gfx.renderer.after_submit();

            let _present = frame_trace::stage(Stage::PresentWait);
            frame.present();
            frame_trace::record_present();
            drop(_present);
            // Prompt 17 S3b: the first frame is on the swapchain. Show the
            // window now, so its first visible content is this frame.
            #[cfg(not(target_arch = "wasm32"))]
            {
                if let Some(gate) = self.startup.as_mut() {
                    if !gate.accepts_input() {
                        // Prompt 17 S6: the frame is presented at
                        // `frame.present()`; the mark is taken before
                        // `ShowWindow` so the trace proves the window was not
                        // visible before its first frame. The report is emitted
                        // below, after the visibility mark.
                        startup::mark("engine: first frame presented");
                        if self.config.visible && !gate.is_shown() {
                            crate::window_style::show_ready(&window);
                            startup::mark("engine: window visible");
                            first_frame_shown = true;
                        }
                        gate.mark_first_frame();
                    }
                }
            }
            // wasm has no startup gate: the old mark + report path still
            // applies there. On desktop this only prints the report (the marks
            // were taken above, on the first frame only).
            #[cfg(target_arch = "wasm32")]
            startup::first_frame_presented();
            startup::report_first_frame();
            // Prompt 17 S3: the startup pipelines exist after the first frame;
            // persist the wgpu cache now (and again at exit).
            #[cfg(not(target_arch = "wasm32"))]
            gfx.save_pipeline_cache(false);
            if let Some(buffer) = capture {
                let (width, height) = self.window_size;
                let bgra = crate::screenshot::is_bgra(gfx.config.format);
                if let Some(shot) = crate::screenshot::read_capture(
                    &gfx.renderer.context.device,
                    &buffer,
                    width,
                    height,
                    bgra,
                ) {
                    self.app.on_screenshot(shot);
                } else {
                    log::warn!("screenshot: the presented frame could not be read back");
                }
            }

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

            // Sincronizar y obtener los bytes si era una pasada de exportación.
            if let Some(readback) = &gfx.renderer.readback_manager {
                self.input.end_frame();
                #[cfg(not(target_arch = "wasm32"))]
                {
                    use pollster::FutureExt;
                    return readback
                        .poll_and_map(&gfx.renderer.context.device)
                        .block_on()
                        .ok();
                }
                #[cfg(target_arch = "wasm32")]
                {
                    log::warn!("Blocking readback not supported on wasm32");
                    return None;
                }
            }

            // Prompt 18 P1: whatever the app asked for during this frame.
            if redraw_request {
                self.redraw.request();
            }
            if let Some(when) = redraw_at {
                self.redraw.request_at(when);
            }

            self.input.end_frame();

            // Prompt 17 S3b: the first frame is visible; deliver the input that
            // arrived while the window was hidden, in order.
            #[cfg(not(target_arch = "wasm32"))]
            if first_frame_shown {
                let queued = self
                    .startup
                    .as_mut()
                    .map(super::show::StartupGate::take_queue)
                    .unwrap_or_default();
                if let Some(event_loop) = event_loop {
                    for (id, event) in queued {
                        <Self as winit::application::ApplicationHandler<
                            crate::redraw::EngineEvent,
                        >>::window_event(self, event_loop, id, event);
                    }
                }
            }
        }
        None
    }

    /// Writes the pipeline cache at exit.
    ///
    /// The first frame already saved it; this forced write also captures
    /// pipelines created later (a mode switch, a new text face).
    pub(super) fn save_pipeline_cache(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(gfx) = self.graphics.as_mut() {
            gfx.save_pipeline_cache(true);
        }
    }

    /// Hides the window, then exits the event loop.
    ///
    /// Hiding first is what makes closing feel instant: the OS removes the
    /// window while the GPU teardown (device and swapchain drop) is still
    /// running. The OS close button and `AppContext::request_exit` both go
    /// through here, so the closing report can measure the hide separately
    /// from the teardown. Later calls do nothing.
    pub(super) fn begin_close(&mut self, event_loop: Option<&ActiveEventLoop>) {
        if self.closing {
            return;
        }
        self.closing = true;
        startup::mark("engine: close requested");
        if let Some(window) = &self.window {
            window.set_visible(false);
        }
        startup::mark("engine: window hidden");
        // Prompt 17 S5: flush app state after the window is gone, before the
        // (expensive) GPU teardown. The window is already invisible, so this
        // never delays what the user sees.
        self.app.on_exit();
        startup::mark("engine: app exit saved");
        if let Some(event_loop) = event_loop {
            event_loop.exit();
        }
    }

    /// Called by winit between frames to manage redraw scheduling.
    pub(super) fn about_to_wait_impl(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(not(target_arch = "wasm32"))]
        let installed = self.poll_startup(Some(event_loop));
        #[cfg(target_arch = "wasm32")]
        let installed = false;

        // Prompt 18 P1: popup commands are applied here and only here, because
        // this is the one callback that has the event loop a new window needs.
        #[cfg(not(target_arch = "wasm32"))]
        self.drain_popups(event_loop);

        // An evidence capture asked for by the app reads back our own popup
        // surfaces; each armed popup presents once more and delivers its frame.
        #[cfg(not(target_arch = "wasm32"))]
        if self.popups.take_capture_request() {
            let handles = self
                .windows
                .as_mut()
                .map(crate::windows::WindowMap::arm_capture)
                .unwrap_or_default();
            for handle in handles {
                if let Some(windows) = self.windows.as_ref() {
                    windows.backend().request_redraw(handle);
                }
            }
        }

        let Some(window) = &self.window else { return };
        let now = Instant::now();

        // Prompt 17 S3b: while the first frame is being prepared the loop only
        // has to wake for the show timeout (or for the GPU worker). A hidden
        // window never gets `RedrawRequested`, so the first frame is rendered
        // here as soon as the state is installed.
        #[cfg(not(target_arch = "wasm32"))]
        if self.graphics.is_none() {
            let poll = now + std::time::Duration::from_millis(5);
            let wake = match &self.startup {
                Some(gate) if !gate.is_shown() => gate.deadline().min(poll),
                _ => poll,
            };
            event_loop.set_control_flow(ControlFlow::WaitUntil(wake));
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        if installed {
            let t = self.clock.peek().elapsed;
            let _ = self.render_frame(Some(event_loop), t);
            return;
        }

        // A never-shown window gets no `RedrawRequested`, so the scheduler
        // renders its frames here (benchmarks/CI). It still presents to the
        // hidden surface, so the frame code is identical.
        if !self.config.visible {
            let now = Instant::now();
            let continuous_deadline =
                if self.redraw.policy() == crate::redraw::RedrawPolicy::Continuous {
                    self.next_frame_deadline
                } else {
                    None
                };
            match self.redraw.next(now, continuous_deadline) {
                crate::redraw::NextFrame::Draw => {
                    let t = self.clock.peek().elapsed;
                    let _ = self.render_frame(Some(event_loop), t);
                }
                crate::redraw::NextFrame::WaitUntil(wake) => {
                    event_loop.set_control_flow(ControlFlow::WaitUntil(wake));
                }
                crate::redraw::NextFrame::Wait => {
                    event_loop.set_control_flow(ControlFlow::Wait);
                }
            }
            return;
        }

        // Continuous apps keep the historical timer-based idle throttle.
        // On-demand apps never tick on a clock: the scheduler below waits for
        // input, an app request, a wake deadline or a worker event.
        if self.redraw.policy() == crate::redraw::RedrawPolicy::Continuous {
            let is_idle = match self.config.idle_timeout {
                Some(timeout) => now.duration_since(self.last_action_time).as_secs_f32() > timeout,
                None => false,
            };
            if is_idle {
                match self.config.idle_fps {
                    // Throttled idle: keep drawing at a low rate.
                    Some(fps) if fps > 0 => {
                        let next =
                            self.last_frame + std::time::Duration::from_secs_f64(1.0 / fps as f64);
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
        }

        #[cfg(target_arch = "wasm32")]
        {
            // The browser paces frames (requestAnimationFrame).
            event_loop.set_control_flow(ControlFlow::Wait);
            window.request_redraw();
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let continuous_deadline =
                if self.redraw.policy() == crate::redraw::RedrawPolicy::Continuous {
                    self.next_frame_deadline
                } else {
                    None
                };
            match self.redraw.next(now, continuous_deadline) {
                crate::redraw::NextFrame::Draw => window.request_redraw(),
                crate::redraw::NextFrame::WaitUntil(wake) => {
                    event_loop.set_control_flow(ControlFlow::WaitUntil(wake));
                }
                crate::redraw::NextFrame::Wait => {
                    event_loop.set_control_flow(ControlFlow::Wait);
                }
            }
        }
    }
}

/// Whether this frame must be copied for `FerrousApp::on_screenshot`.
///
/// A request made in `update` or in `draw_ui` both count; the copy also needs
/// `COPY_SRC` on the window surface.
#[cfg(not(target_arch = "wasm32"))]
fn screenshot_wanted(update_requested: bool, draw_requested: bool, copy_src: bool) -> bool {
    (update_requested || draw_requested) && copy_src
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::screenshot_wanted;

    #[test]
    fn a_screenshot_requested_in_update_is_copied() {
        assert!(screenshot_wanted(true, false, true));
    }

    #[test]
    fn a_screenshot_requested_in_draw_ui_is_copied() {
        assert!(screenshot_wanted(false, true, true));
    }

    #[test]
    fn no_request_or_no_copy_src_copies_nothing() {
        assert!(!screenshot_wanted(false, false, true));
        assert!(!screenshot_wanted(true, true, false));
    }
}
