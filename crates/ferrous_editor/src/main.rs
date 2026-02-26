use ferrous_core::{context::EngineContext, InputState};
use ferrous_gui::{GuiBatch, GuiQuad, Widget};
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

// helper to create a tiny in-memory font with a single square glyph for 'A'.
// this is essentially the same code used in the assets crate's tests but
// duplicated here so we can build an atlas without shipping a real font file.
fn build_test_font() -> Vec<u8> {
    let mut tables: Vec<([u8; 4], Vec<u8>)> = Vec::new();

    // cmap mapping 'A'->0 (glyph index 0)
    let mut cmap = Vec::new();
    cmap.extend(&0u16.to_be_bytes()); // version
    cmap.extend(&1u16.to_be_bytes()); // numSubtables
    let subtable_record_pos = cmap.len();
    cmap.extend(&3u16.to_be_bytes()); // platform
    cmap.extend(&1u16.to_be_bytes()); // encoding
    cmap.extend(&0u32.to_be_bytes()); // offset placeholder

    let fmt_start = cmap.len();
    cmap.extend(&4u16.to_be_bytes()); // format
    cmap.extend(&0u16.to_be_bytes()); // length placeholder
    cmap.extend(&0u16.to_be_bytes()); // language
    cmap.extend(&2u16.to_be_bytes()); // segCountX2
    cmap.extend(&0u16.to_be_bytes()); // searchRange
    cmap.extend(&0u16.to_be_bytes()); // entrySelector
    cmap.extend(&0u16.to_be_bytes()); // rangeShift
    cmap.extend(&('A' as u16).to_be_bytes()); // endCodes
    cmap.extend(&0u16.to_be_bytes()); // reservedPad
    cmap.extend(&('A' as u16).to_be_bytes()); // startCodes
    cmap.extend(&(-65i16).to_be_bytes()); // idDeltas: -65 -> map 65 to 0
    cmap.extend(&0u16.to_be_bytes()); // idRangeOffsets

    let fmt_length = (cmap.len() - fmt_start) as u16;
    cmap[fmt_start + 2..fmt_start + 4].copy_from_slice(&fmt_length.to_be_bytes());
    let offset_val = fmt_start as u32;
    cmap[subtable_record_pos + 4..subtable_record_pos + 8]
        .copy_from_slice(&offset_val.to_be_bytes());

    tables.push((*b"cmap", cmap));

    // head table: basic header with unitsPerEm and indexToLocFormat
    let mut head = vec![0u8; 54];
    head[18..20].copy_from_slice(&1000u16.to_be_bytes());
    head[50..52].copy_from_slice(&1i16.to_be_bytes());
    tables.push((*b"head", head));

    // glyf: simple square
    let mut glyf = Vec::new();
    glyf.extend(&1i16.to_be_bytes()); // numberOfContours
    glyf.extend(&0i16.to_be_bytes()); // xMin
    glyf.extend(&0i16.to_be_bytes()); // yMin
    glyf.extend(&100i16.to_be_bytes()); // xMax
    glyf.extend(&100i16.to_be_bytes()); // yMax
    glyf.extend(&3u16.to_be_bytes()); // endPtsOfContours[0]
    glyf.extend(&0u16.to_be_bytes()); // instructionLength
    for _ in 0..4 {
        glyf.push(0x01); // on-curve flags
    }
    // coords
    glyf.extend(&0i16.to_be_bytes());
    glyf.extend(&0i16.to_be_bytes());
    glyf.extend(&0i16.to_be_bytes());
    glyf.extend(&100i16.to_be_bytes());
    glyf.extend(&100i16.to_be_bytes());
    glyf.extend(&0i16.to_be_bytes());
    glyf.extend(&0i16.to_be_bytes());
    glyf.extend(&(-100i16).to_be_bytes());
    tables.push((*b"glyf", glyf));

    // loca entries for two glyphs
    let mut loca = Vec::new();
    loca.extend(&0u32.to_be_bytes());
    let glyf_len = tables.iter().find(|(t, _)| t == b"glyf").unwrap().1.len() as u32;
    loca.extend(&glyf_len.to_be_bytes());
    tables.push((*b"loca", loca));

    // assemble font file
    let mut data = Vec::new();
    data.extend(&0u32.to_be_bytes());
    let num_tables = tables.len() as u16;
    data.extend(&num_tables.to_be_bytes());
    data.extend(&0u16.to_be_bytes());
    data.extend(&0u16.to_be_bytes());
    data.extend(&0u16.to_be_bytes());

    let mut offset = 12 + (16 * tables.len());
    let mut positions = Vec::new();
    for (_, tbl) in &tables {
        positions.push(offset as u32);
        offset += tbl.len();
    }
    for ((tag, tbl), &pos) in tables.iter().zip(&positions) {
        data.extend(tag);
        data.extend(&0u32.to_be_bytes());
        data.extend(&pos.to_be_bytes());
        data.extend(&(tbl.len() as u32).to_be_bytes());
    }
    for (_, tbl) in &tables {
        data.extend(tbl);
    }
    data
}

/// Simple interactive widget used for testing hit‑testing and color change.
struct TestButton {
    rect: [f32; 4], // x, y, width, height
    hovered: bool,
    pressed: bool,
}

impl TestButton {
    fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            rect: [x, y, w, h],
            hovered: false,
            pressed: false,
        }
    }

    fn hit(&self, x: f64, y: f64) -> bool {
        let x = x as f32;
        let y = y as f32;
        x >= self.rect[0]
            && x <= self.rect[0] + self.rect[2]
            && y >= self.rect[1]
            && y <= self.rect[1] + self.rect[3]
    }
}

impl Widget for TestButton {
    fn draw(&self, batch: &mut GuiBatch) {
        let color = if self.pressed {
            [0.8, 0.2, 0.2, 1.0]
        } else if self.hovered {
            [0.2, 0.8, 0.2, 1.0]
        } else {
            [0.2, 0.2, 0.8, 1.0]
        };
        batch.push(GuiQuad {
            pos: [self.rect[0], self.rect[1]],
            size: [self.rect[2], self.rect[3]],
            color,
        });
    }
}

/// Application state managed by winit's `ApplicationHandler` API.
struct EditorApp {
    renderer: Option<Renderer>,
    window: Option<Arc<Window>>,
    surface: Option<wgpu::Surface<'static>>,
    config: Option<wgpu::SurfaceConfiguration>,
    input: InputState,
    test_button: TestButton,
    viewport: Viewport,
    window_size: (u32, u32),
    last_update: std::time::Instant,
    // atlas used for text rendering; built once on resume
    font_atlas: Option<ferrous_assets::FontAtlas>,
}

impl EditorApp {
    fn new() -> Self {
        Self {
            renderer: None,
            window: None,
            surface: None,
            config: None,
            input: InputState::new(),
            test_button: TestButton::new(50.0, 50.0, 100.0, 100.0),
            viewport: Viewport {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            },
            window_size: (0, 0),
            last_update: std::time::Instant::now(),
            font_atlas: None,
        }
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

        let mut renderer = Renderer::new(context, config.width, config.height, config.format);

        // build a tiny font atlas containing only the 'A' glyph so we can
        // exercise text rendering.  In a real application you would load a
        // proper TTF/OTF file and select a larger character set.
        let font_bytes = build_test_font();
        let parser = ferrous_assets::font_parser::FontParser::new(font_bytes).expect("parser");
        let atlas = ferrous_assets::FontAtlas::new(
            &renderer.context.device,
            &renderer.context.queue,
            &parser,
            vec!['A'],
        )
        .expect("atlas build");
        // hand atlas to renderer which will forward to its GUI component
        renderer.set_font_atlas(&atlas.view, &atlas.sampler);

        self.font_atlas = Some(atlas);

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
                    self.window_size = (w, h);
                    renderer.resize(w, h);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                // we prefer to use the physical key so WASD movement applies
                // consistently regardless of keyboard layout. the `event`
                // structure contains both `physical_key` and `state`.
                let winit::event::KeyEvent {
                    physical_key,
                    state,
                    ..
                } = event;
                if let winit::keyboard::PhysicalKey::Code(code) = physical_key {
                    self.input
                        .update_key(code, state == winit::event::ElementState::Pressed);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.input.set_mouse_position(position.x, position.y);
                self.test_button.hovered = self.test_button.hit(position.x, position.y);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = state == winit::event::ElementState::Pressed;
                self.input.update_mouse_button(button, pressed);
                if pressed {
                    let (mx, my) = self.input.mouse_position();
                    if self.test_button.hit(mx, my) {
                        self.test_button.pressed = true;
                    } else {
                        self.test_button.pressed = false;
                    }
                } else {
                    self.test_button.pressed = false;
                }
            }
            _ => (),
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // draw a frame each time the loop is about to sleep
        if let (Some(surface), Some(renderer), Some(config)) =
            (&mut self.surface, &mut self.renderer, &mut self.config)
        {
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
            if inside && !self.test_button.hovered {
                renderer.handle_input(&mut self.input, dt);
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
            self.test_button.draw(&mut batch);

            // build a small text batch if we have an atlas
            let mut text_batch = ferrous_gui::TextBatch::new();
            if let Some(atlas) = &self.font_atlas {
                // draw a few "A" characters to prove the pipeline works
                text_batch.draw_text(atlas, "AAA", [10.0, 10.0], 32.0, [1.0, 1.0, 0.0, 1.0]);
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
