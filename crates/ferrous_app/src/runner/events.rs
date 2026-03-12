//! `ApplicationHandler` implementation for `Runner` â€” window lifecycle and events.

use ferrous_core::glam::{self, Vec3};
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
use crate::graphics::GraphicsState;
use crate::render_context::RenderContext;
use crate::traits::FerrousApp;
use ferrous_assets::Font;

use super::types::Runner;

impl<A: FerrousApp> ApplicationHandler for Runner<A> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.app.configure_ui(&mut self.ui.tree);

        let attributes = Window::default_attributes()
            .with_title(&self.config.title)
            .with_resizable(self.config.resizable)
            .with_decorations(self.config.decorations)
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

        // â”€â”€ Desktop: synchronous blocking GPU init â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
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
            gfx.renderer
                .set_render_style(self.config.render_style.clone());
            // Propagate the app mode to the renderer
            if self.config.mode == AppMode::Desktop2D {
                gfx.renderer
                    .set_mode(ferrous_renderer::RendererMode::Desktop2D);
            }

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
                let handle = self
                    .asset_server
                    .load::<ferrous_assets::font_importer::FontData>(path.as_str());
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
                    render_stats: ferrous_core::RenderStats::default(),
                    camera_eye: ferrous_core::glam::Vec3::ZERO,
                    camera_target: ferrous_core::glam::Vec3::ZERO,
                    world: &mut self.world,
                    viewport: self.viewport,
                    gizmos: Vec::new(),
                    render: RenderContext::new(&mut gfx.renderer),
                    asset_server: &mut self.asset_server,
                    exit_requested: false,
                    _gpu_backend: backend,
                };
                self.app.setup(&mut ctx);
            }

            self.graphics = Some(gfx);
            self.window = Some(window);
        }

        // â”€â”€ Web (WASM): asynchronous init â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
        #[cfg(target_arch = "wasm32")]
        {
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
                let mut gfx = GraphicsState::new(
                    &window_clone,
                    vp.width,
                    vp.height,
                    vsync,
                    samples,
                    hdri_path.clone(),
                )
                .await;
                gfx.renderer.set_clear_color(bg);
                gfx.renderer.set_viewport(vp);
                gfx.renderer.set_render_style(render_style);
                if app_mode == AppMode::Desktop2D {
                    gfx.renderer
                        .set_mode(ferrous_renderer::RendererMode::Desktop2D);
                }

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

            self.font_pending = Some(font_slot);
            self.window = Some(window.clone());
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {}

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::RedrawRequested => {
                self.render_frame(event_loop);
                return;
            }
            _ => self.last_action_time = Instant::now(),
        }

        match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.input.set_mouse_position(position.x, position.y);
                self.ui.dispatch_event(
                    &mut self.app,
                    ferrous_ui_core::UiEvent::MouseMove {
                        pos: glam::Vec2::new(position.x as f32, position.y as f32),
                    },
                );
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = state == winit::event::ElementState::Pressed;
                self.input.update_mouse_button(button.into(), pressed);
                let (mx, my) = self.input.mouse_position();
                let pos = glam::Vec2::new(mx as f32, my as f32);
                let button = ferrous_events::winit_to_mousebutton(button);
                if pressed {
                    self.ui.dispatch_event(
                        &mut self.app,
                        ferrous_ui_core::UiEvent::MouseDown { button, pos },
                    );
                } else {
                    self.ui.dispatch_event(
                        &mut self.app,
                        ferrous_ui_core::UiEvent::MouseUp { button, pos },
                    );
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    winit::event::MouseScrollDelta::LineDelta(x, y) => (x, y),
                    winit::event::MouseScrollDelta::PixelDelta(pos) => {
                        (pos.x as f32 / 20.0, pos.y as f32 / 20.0)
                    }
                };
                self.ui.dispatch_event(
                    &mut self.app,
                    ferrous_ui_core::UiEvent::MouseWheel {
                        delta_x: dx,
                        delta_y: dy,
                    },
                );
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

                // Ctrl+combos y Shift+combos: despachar variantes especiales y retornar
                if state == winit::event::ElementState::Pressed && (self.ctrl_held || self.shift_held) {
                    let combo_key = match physical_key {
                        // Ctrl+Shift+flechas
                        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowLeft)
                            if self.ctrl_held && self.shift_held =>
                            Some(ferrous_ui_core::GuiKey::CtrlShiftArrowLeft),
                        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowRight)
                            if self.ctrl_held && self.shift_held =>
                            Some(ferrous_ui_core::GuiKey::CtrlShiftArrowRight),
                        // Ctrl+combos
                        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyA)
                            if self.ctrl_held =>
                            Some(ferrous_ui_core::GuiKey::CtrlA),
                        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyC)
                            if self.ctrl_held =>
                            Some(ferrous_ui_core::GuiKey::CtrlC),
                        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyX)
                            if self.ctrl_held =>
                            Some(ferrous_ui_core::GuiKey::CtrlX),
                        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyV)
                            if self.ctrl_held =>
                            Some(ferrous_ui_core::GuiKey::CtrlV),
                        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyZ)
                            if self.ctrl_held =>
                            Some(ferrous_ui_core::GuiKey::CtrlZ),
                        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::KeyY)
                            if self.ctrl_held =>
                            Some(ferrous_ui_core::GuiKey::CtrlY),
                        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowLeft)
                            if self.ctrl_held =>
                            Some(ferrous_ui_core::GuiKey::CtrlArrowLeft),
                        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowRight)
                            if self.ctrl_held =>
                            Some(ferrous_ui_core::GuiKey::CtrlArrowRight),
                        // Shift+flechas y Shift+Home/End
                        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowLeft)
                            if self.shift_held =>
                            Some(ferrous_ui_core::GuiKey::ShiftArrowLeft),
                        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::ArrowRight)
                            if self.shift_held =>
                            Some(ferrous_ui_core::GuiKey::ShiftArrowRight),
                        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Home)
                            if self.shift_held =>
                            Some(ferrous_ui_core::GuiKey::ShiftHome),
                        winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::End)
                            if self.shift_held =>
                            Some(ferrous_ui_core::GuiKey::ShiftEnd),
                        _ => None,
                    };
                    if let Some(key) = combo_key {
                        self.ui.dispatch_event(
                            &mut self.app,
                            ferrous_ui_core::UiEvent::KeyDown { key },
                        );
                        return;
                    }
                }

                if let Some(key) = if let winit::keyboard::PhysicalKey::Code(k) = physical_key {
                    Some(ferrous_events::winit_to_guikey(k))
                } else {
                    None
                } {
                    if state == winit::event::ElementState::Pressed {
                        self.ui.dispatch_event(
                            &mut self.app,
                            ferrous_ui_core::UiEvent::KeyDown { key },
                        );
                    } else {
                        self.ui
                            .dispatch_event(&mut self.app, ferrous_ui_core::UiEvent::KeyUp { key });
                    }
                }

                if let Some(txt) = text {
                    if state == winit::event::ElementState::Pressed && !self.ctrl_held {
                        for c in txt.chars() {
                            self.input.push_char(c);
                            self.ui.dispatch_event(
                                &mut self.app,
                                ferrous_ui_core::UiEvent::Char { c },
                            );
                        }
                    }
                }
            }
            WindowEvent::Resized(size) => {
                if let Some(gfx) = &mut self.graphics {
                    gfx.resize(size.width, size.height);
                }
                self.window_size = (size.width, size.height);
            }
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.about_to_wait_impl(event_loop);
    }
}
