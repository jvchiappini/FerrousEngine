//! `ApplicationHandler` implementation for `Runner` — window lifecycle and events.

use ferrous_core::glam::Vec3;
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

use crate::context::AppContext;
use crate::graphics::GraphicsState;
use crate::render_context::RenderContext;
use crate::traits::FerrousApp;
use ferrous_assets::Font;

use super::types::Runner;

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
        self.viewport = ferrous_core::Viewport {
            x: 0,
            y: 0,
            width: self.config.width,
            height: self.config.height,
        };

        // ── Desktop: synchronous blocking GPU init ───────────────────────────
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
            gfx.renderer.set_clear_color(self.config.background_color.to_wgpu());
            gfx.renderer.set_render_style(self.config.render_style.clone());

            if let Some(bytes) = self.config.font_bytes {
                let font = Font::load_bytes(
                    bytes,
                    &gfx.renderer.context.device,
                    &gfx.renderer.context.queue,
                    ' '..'~',
                );
                gfx.renderer.set_font_atlas(&font.atlas.view, &font.atlas.sampler);
                self.font = Some(font);
            } else if let Some(path) = &self.config.font_path {
                use ferrous_assets::font_importer::FontData;
                let handle = self.asset_server.load::<FontData>(path.as_str());
                self.font_asset_handle = Some(handle);
            }

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
                    render: RenderContext::new(&mut gfx.renderer),
                    exit_requested: false,
                    _gpu_backend: backend,
                    asset_server: &mut self.asset_server,
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

        // ── wasm32: async GPU init via spawn_local ───────────────────────────
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
            let render_style = self.config.render_style.clone();

            let font_slot: Rc<RefCell<Option<Font>>> = Rc::new(RefCell::new(None));
            let font_slot_clone = font_slot.clone();
            let window_for_closure = window.clone();

            wasm_bindgen_futures::spawn_local(async move {
                let mut gfx =
                    GraphicsState::new(&window_for_closure, w, h, vsync, samples, hdri_path.clone())
                        .await;
                gfx.renderer.set_clear_color(bg);
                gfx.renderer.set_viewport(vp);
                gfx.renderer.set_render_style(render_style);

                if let Some(bytes) = font_bytes {
                    let font = Font::load_bytes(
                        bytes,
                        &gfx.renderer.context.device,
                        &gfx.renderer.context.queue,
                        ' '..'~',
                    );
                    gfx.renderer.set_font_atlas(&font.atlas.view, &font.atlas.sampler);
                    *font_slot_clone.borrow_mut() = Some(font);
                }

                *slot_clone.borrow_mut() = Some(gfx);
            });

            self.font_pending = Some(font_slot);
            self.window = Some(window.clone());
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {}

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::RedrawRequested => {}
            _ => self.last_action_time = Instant::now(),
        }

        self.ui.handle_window_event(&event, &mut self.input);

        if let Some(window) = self.window.clone() {
            let time = self.clock.peek();
            let backend = self
                .graphics
                .as_ref()
                .map(|g| g.renderer.context.backend)
                .unwrap_or(wgpu::Backend::Empty);
            if let Some(gfx) = self.graphics.as_mut() {
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
                    render: RenderContext::new(&mut gfx.renderer),
                    exit_requested: false,
                    _gpu_backend: backend,
                    asset_server: &mut self.asset_server,
                };
                self.app.on_window_event(&event, &mut ctx);
                if ctx.exit_requested {
                    event_loop.exit();
                    return;
                }
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
                if let Some(window) = self.window.clone() {
                    if let Some(gfx) = self.graphics.as_mut() {
                        let time = self.clock.peek();
                        let backend = gfx.renderer.context.backend;
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
                            render: RenderContext::new(&mut gfx.renderer),
                            exit_requested: false,
                            _gpu_backend: backend,
                            asset_server: &mut self.asset_server,
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
        self.about_to_wait_impl(event_loop);
    }
}
