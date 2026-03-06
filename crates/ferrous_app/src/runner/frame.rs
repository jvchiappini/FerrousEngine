//! Per-frame logic: `render_frame` and `about_to_wait`.

use std::time::Duration;

use ferrous_assets::AssetState;
use ferrous_core::glam::Vec3;
use ferrous_gui::{GuiBatch, TextBatch};
use winit::event_loop::{ActiveEventLoop, ControlFlow};

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use crate::context::AppContext;
use crate::render_context::RenderContext;
use crate::traits::FerrousApp;

use super::types::Runner;

impl<A: FerrousApp> Runner<A> {
    /// Executes one full update + render cycle.
    pub(super) fn render_frame(&mut self, event_loop: &ActiveEventLoop) {
        // wasm32: drain async GPU init result before anything else.
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
                                render: RenderContext::new(&mut gfx.renderer),
                                asset_server: &mut self.asset_server,
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
                        if let Some(font_slot) = self.font_pending.take() {
                            if let Ok(mut borrow) = font_slot.try_borrow_mut() {
                                if let Some(font) = borrow.take() {
                                    self.font = Some(font);
                                }
                            }
                        }
                    } else {
                        return;
                    }
                } else {
                    return;
                }
            }
        }

        let (Some(gfx), Some(window)) = (&mut self.graphics, &self.window) else {
            return;
        };

        let now = Instant::now();
        self.last_frame = now;
        self.last_action_time = now;

        #[cfg(not(target_arch = "wasm32"))]
        if let Some(target_fps) = self.config.target_fps {
            let budget = Duration::from_secs_f64(1.0 / target_fps as f64);
            let next = match self.next_frame_deadline {
                Some(prev) => {
                    let candidate = prev + budget;
                    if candidate < now { now + budget } else { candidate }
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
                        ' '..'~',
                    );
                    gfx.renderer.set_font_atlas(&font.atlas.view, &font.atlas.sampler);
                    self.font = Some(font);
                    self.font_asset_handle = None;
                }
            }
        }

        self.asset_server.tick();

        // Advance ECS systems
        self.resources.insert(self.clock);
        self.systems.run_all(&mut self.world.ecs, &mut self.resources);
        self.clock = *self.resources.get::<ferrous_core::TimeClock>().unwrap();
        let time = self.clock.at_tick();

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
                _gpu_backend: backend,
                asset_server: &mut self.asset_server,
            };

            self.app.update(&mut ctx);
            if ctx.exit_requested {
                event_loop.exit();
                return;
            }

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

        gfx.renderer.sync_world(&self.world);

        // 3D camera input
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
                _gpu_backend: backend,
                asset_server: &mut self.asset_server,
            };

            if self.viewport.width > 0 {
                self.app.draw_3d(&mut ctx);
            }

            for gizmo in ctx.gizmos.drain(..) {
                ctx.render.inner.queue_gizmo(gizmo);
            }

            let mut gui_batch = GuiBatch::new();
            let mut text_batch = TextBatch::new();
            self.app.draw_ui(
                &mut gui_batch,
                &mut text_batch,
                self.font.as_ref(),
                &mut ctx,
            );
            self.ui.draw(&mut gui_batch, &mut text_batch, self.font.as_ref());

            // ── 3. RENDER FINAL ─────────────────────────────────────────────
            let frame = match gfx.surface.get_current_texture() {
                Ok(f) => f,
                Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                    gfx.resize(self.window_size.0, self.window_size.1);
                    return;
                }
                Err(e) => panic!("Surface error: {e:?}"),
            };
            let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
            gfx.renderer
                .render_to_view(&mut encoder, &view, Some(gui_batch), Some(text_batch));
            gfx.renderer.context.queue.submit(Some(encoder.finish()));
            frame.present();
        }

        self.input.end_frame();
    }

    /// Called by winit between frames to manage redraw scheduling.
    pub(super) fn about_to_wait_impl(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            return;
        }

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
            } else {
                window.request_redraw();
            }
        }
    }
}
