//! Dear ImGui side of the Phase 9B comparison: the start-screen recipe on
//! imgui-rs + imgui-wgpu + imgui-winit-support. Run with `--seconds N`
//! (default 8); `frames N` is printed to stdout on exit.
//!
//! Versions (mutually compatible, checked against crates.io):
//!   imgui 0.12 | imgui-wgpu 0.28 (-> wgpu 29) | imgui-winit-support 0.13
//!   (-> winit 0.30) | winit 0.30 | wgpu 29 | pollster 0.4
//!
//! DPI: winit creates a 1280x800 *logical* window; winit has no per-window
//! scale override on Windows, so the 1.5 factor comes from the monitor. The
//! ImGui layout is authored in physical pixels (`window.scale_factor()`), which
//! matches the spec when the host monitor is at 1.5x.

use std::io::Write;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use imgui::{Condition, FontConfig, FontId, FontSource, StyleColor, StyleVar, Ui};
use imgui_wgpu::{Renderer, RendererConfig};
use imgui_winit_support::{HiDpiMode, WinitPlatform};
use pollster::block_on;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

const PAGE: [f32; 4] = [6.0 / 255.0, 8.0 / 255.0, 13.0 / 255.0, 1.0];
const CARD: [f32; 4] = [15.0 / 255.0, 18.0 / 255.0, 23.0 / 255.0, 1.0];
const WELL: [f32; 4] = [11.0 / 255.0, 13.0 / 255.0, 19.0 / 255.0, 1.0];
const RAISED: [f32; 4] = [24.0 / 255.0, 27.0 / 255.0, 32.0 / 255.0, 1.0];
const BORDER: [f32; 4] = [38.0 / 255.0, 41.0 / 255.0, 47.0 / 255.0, 1.0];
const TEXT: [f32; 4] = [241.0 / 255.0, 245.0 / 255.0, 254.0 / 255.0, 1.0];
const MUTED: [f32; 4] = [173.0 / 255.0, 177.0 / 255.0, 185.0 / 255.0, 1.0];
const FAINT: [f32; 4] = [113.0 / 255.0, 117.0 / 255.0, 124.0 / 255.0, 1.0];
const ACCENT: [f32; 4] = [39.0 / 255.0, 80.0 / 255.0, 199.0 / 255.0, 1.0];

const ROBOTO: &[u8] = include_bytes!("../../../../assets/fonts/Roboto-Regular.ttf");

fn font_source(size_pixels: f32, roboto: bool) -> FontSource<'static> {
    if roboto {
        FontSource::TtfData {
            data: ROBOTO,
            size_pixels,
            config: None,
        }
    } else {
        FontSource::DefaultFontData {
            config: Some(FontConfig {
                size_pixels,
                ..Default::default()
            }),
        }
    }
}

const CLEAR: wgpu::Color = wgpu::Color {
    r: 6.0 / 255.0,
    g: 8.0 / 255.0,
    b: 13.0 / 255.0,
    a: 1.0,
};

struct Fonts {
    title: FontId,
    heading: FontId,
    body: FontId,
    small: FontId,
}

struct Gfx {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    scale: f32,
    context: imgui::Context,
    platform: WinitPlatform,
    renderer: Renderer,
    fonts: Fonts,
    show_fallback: bool,
    last_frame: Instant,
    last_cursor: Option<imgui::MouseCursor>,
}

fn create(event_loop: &ActiveEventLoop, roboto: bool) -> Gfx {
    // wgpu 29 reads `WGPU_BACKEND` through `Backends::from_env` (the older
    // `util::backend_bits_from_env` helper no longer exists).
    let backends = wgpu::Backends::from_env().unwrap_or(wgpu::Backends::PRIMARY);
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends,
        ..wgpu::InstanceDescriptor::new_with_display_handle(Box::new(
            event_loop.owned_display_handle(),
        ))
    });

    let window = {
        let attributes = Window::default_attributes()
            .with_title("guimaker-compare-imgui")
            .with_inner_size(LogicalSize::new(1280.0, 800.0))
            .with_resizable(false);
        Arc::new(event_loop.create_window(attributes).unwrap())
    };

    let size = window.inner_size();
    let scale = window.scale_factor() as f32;
    let surface = instance.create_surface(window.clone()).unwrap();

    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: Some(&surface),
        force_fallback_adapter: false,
    }))
    .unwrap();

    let (device, queue) =
        block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();

    let capabilities = surface.get_capabilities(&adapter);
    let format = capabilities
        .formats
        .iter()
        .copied()
        .find(|format| format.is_srgb())
        .unwrap_or(capabilities.formats[0]);

    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: size.width,
        height: size.height,
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 2,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        view_formats: vec![],
    };
    surface.configure(&device, &config);

    let mut context = imgui::Context::create();
    let mut platform = WinitPlatform::new(&mut context);
    platform.attach_window(context.io_mut(), &*window, HiDpiMode::Default);
    context.set_ini_filename(None::<std::path::PathBuf>);
    context.io_mut().font_global_scale = 1.0;

    let title = context
        .fonts()
        .add_font(&[font_source(26.0 * scale, roboto)]);
    let heading = context
        .fonts()
        .add_font(&[font_source(19.0 * scale, roboto)]);
    let body = context
        .fonts()
        .add_font(&[font_source(15.0 * scale, roboto)]);
    let small = context
        .fonts()
        .add_font(&[font_source(12.0 * scale, roboto)]);
    let fonts = Fonts {
        title,
        heading,
        body,
        small,
    };

    {
        let style = context.style_mut();
        style.window_padding = [28.0 * scale, 18.0 * scale];
        style.cell_padding = [14.0 * scale, 8.0 * scale];
        style.window_border_size = 0.0;
        style.child_border_size = 1.0;
        style.frame_padding = [10.0 * scale, 8.0 * scale];
        style.item_spacing = [8.0 * scale, 8.0 * scale];
        style.colors[StyleColor::WindowBg as usize] = PAGE;
        style.colors[StyleColor::ChildBg as usize] = PAGE;
        style.colors[StyleColor::Border as usize] = BORDER;
        style.colors[StyleColor::Text as usize] = TEXT;
        style.colors[StyleColor::Separator as usize] = BORDER;
    }

    let renderer = Renderer::new(
        &mut context,
        &device,
        &queue,
        RendererConfig {
            texture_format: format,
            ..Default::default()
        },
    );

    Gfx {
        window,
        surface,
        device,
        queue,
        config,
        scale,
        context,
        platform,
        renderer,
        fonts,
        show_fallback: !roboto,
        last_frame: Instant::now(),
        last_cursor: None,
    }
}

fn text(ui: &Ui, font: FontId, color: [f32; 4], value: &str) {
    let _font = ui.push_font(font);
    let _color = ui.push_style_color(StyleColor::Text, color);
    ui.text(value);
}

fn text_wrapped(ui: &Ui, font: FontId, color: [f32; 4], value: &str) {
    let _font = ui.push_font(font);
    let _color = ui.push_style_color(StyleColor::Text, color);
    ui.text_wrapped(value);
}

fn card(ui: &Ui, fonts: &Fonts, scale: f32, n: usize) {
    let names = ["##card1", "##card2", "##card3", "##card4", "##card5"];
    let width = ui.content_region_avail()[0];

    let _bg = ui.push_style_color(StyleColor::ChildBg, CARD);
    ui.child_window(names[n - 1])
        .size([width, 56.0 * scale])
        .border(true)
        .build(|| {
            let origin = ui.cursor_pos();
            let inner_width = ui.content_region_avail()[0];
            text(ui, fonts.body, TEXT, &format!("Project {n}"));
            text(ui, fonts.small, MUTED, "Edited recently");

            let time_width = ui.calc_text_size("11:42")[0];
            ui.set_cursor_pos([origin[0] + inner_width - time_width, origin[1]]);
            text(ui, fonts.small, FAINT, "11:42");
            ui.set_cursor_pos([origin[0], origin[1] + 34.0 * scale]);
        });
    ui.dummy([0.0, 10.0 * scale]);
}

fn field(ui: &Ui, fonts: &Fonts, scale: f32, id: &str, label: &str, value: &str) {
    text(ui, fonts.small, MUTED, label);
    let width = ui.content_region_avail()[0];
    let _bg = ui.push_style_color(StyleColor::ChildBg, WELL);
    ui.child_window(id)
        .size([width, 36.0 * scale])
        .border(true)
        .build(|| {
            let origin = ui.cursor_pos();
            ui.set_cursor_pos([origin[0], origin[1] + 6.0 * scale]);
            text(ui, fonts.body, TEXT, value);
        });
    ui.dummy([0.0, 8.0 * scale]);
}

fn form(ui: &Ui, fonts: &Fonts, scale: f32) {
    text(ui, fonts.heading, TEXT, "New project");
    ui.dummy([0.0, 6.0 * scale]);
    text_wrapped(
        ui,
        fonts.body,
        MUTED,
        "Start from a blank canvas or a starter composition, then refine every keyframe.",
    );
    ui.dummy([0.0, 10.0 * scale]);

    field(ui, fonts, scale, "##name", "Name", "Untitled");
    field(ui, fonts, scale, "##size", "Size", "1920 x 1080");
    field(ui, fonts, scale, "##fps", "Frame rate", "60 fps");
    field(ui, fonts, scale, "##engine", "Engine", "FerrousEngine");

    ui.dummy([0.0, 4.0 * scale]);

    {
        let _b = ui.push_style_color(StyleColor::Button, ACCENT);
        let _bh = ui.push_style_color(StyleColor::ButtonHovered, ACCENT);
        let _ba = ui.push_style_color(StyleColor::ButtonActive, ACCENT);
        let _t = ui.push_style_color(StyleColor::Text, TEXT);
        ui.button_with_size("Create", [110.0 * scale, 38.0 * scale]);
    }

    ui.same_line_with_spacing(0.0, 12.0 * scale);

    {
        let _b = ui.push_style_color(StyleColor::Button, RAISED);
        let _bh = ui.push_style_color(StyleColor::ButtonHovered, RAISED);
        let _ba = ui.push_style_color(StyleColor::ButtonActive, RAISED);
        let _t = ui.push_style_color(StyleColor::Text, TEXT);
        ui.button_with_size("Browse", [110.0 * scale, 38.0 * scale]);
    }
}

fn draw_start(
    ui: &Ui,
    fonts: &Fonts,
    scale: f32,
    bench: &mut String,
    menu_open: &mut bool,
    show_fallback: bool,
) {
    let full = ui.io().display_size;

    ui.window("##root")
        .position([0.0, 0.0], Condition::Always)
        .size(full, Condition::Always)
        .title_bar(false)
        .resizable(false)
        .movable(false)
        .collapsible(false)
        .scroll_bar(false)
        .bg_alpha(1.0)
        .build(|| {
            // 1. Header.
            let start = ui.cursor_pos();
            text(ui, fonts.title, TEXT, "Motioner");
            text(ui, fonts.small, MUTED, "Code-driven motion graphics");
            let after_brand = ui.cursor_pos();

            let right = start[0] + ui.content_region_avail()[0];
            let docs_width = ui.calc_text_size("Docs")[0];
            let settings_width = ui.calc_text_size("Settings")[0];
            let link_gap = 18.0 * scale;

            ui.set_cursor_pos([right - docs_width, start[1]]);
            text(ui, fonts.body, MUTED, "Docs");
            ui.set_cursor_pos([right - docs_width - link_gap - settings_width, start[1]]);
            text(ui, fonts.body, MUTED, "Settings");

            ui.set_cursor_pos([start[0], after_brand[1] + 8.0 * scale]);

            // 2. Divider.
            ui.separator();
            ui.dummy([0.0, 8.0 * scale]);

            // 3. Body: 55% / 45%.
            let body_start = ui.cursor_pos();
            let available = ui.content_region_avail();
            let footer_height = 28.0 * scale;
            let body_height = (available[1] - footer_height).max(0.0);
            let gap = 20.0 * scale;
            let recent_width = (available[0] - gap) * 0.55;
            let form_width = available[0] - gap - recent_width;

            ui.child_window("##recent")
                .size([recent_width, body_height])
                .border(false)
                .build(|| {
                    text(ui, fonts.heading, TEXT, "Recent projects");
                    ui.dummy([0.0, 8.0 * scale]);
                    for n in 1..=5 {
                        card(ui, fonts, scale, n);
                    }
                });

            ui.same_line_with_spacing(0.0, gap);

            ui.child_window("##form")
                .size([form_width, body_height])
                .border(false)
                .build(|| {
                    form(ui, fonts, scale);
                });

            // 4. Footer.
            let footer_y = body_start[1] + body_height + 14.0 * scale;
            ui.set_cursor_pos([start[0], footer_y]);
            text(ui, fonts.small, FAINT, "FerrousEngine 0.1");

            let license = "Apache-2.0";
            let license_width = ui.calc_text_size(license)[0];
            ui.set_cursor_pos([right - license_width, footer_y]);
            text(ui, fonts.small, FAINT, license);

            // Bench-only control strip below the start screen. These are raw
            // logical coordinates (imgui works in logical units and the
            // renderer applies the framebuffer scale), so the strip lands at
            // the same place as in the other apps.
            ui.set_cursor_screen_pos([40.0, 704.0]);
            text(ui, fonts.small, MUTED, "Bench input");

            if show_fallback {
                ui.set_cursor_screen_pos([40.0, 676.0]);
                text(ui, fonts.small, MUTED, "漢字 🎬 ✨");
            }

            ui.set_cursor_screen_pos([40.0, 724.0]);
            ui.set_next_item_width(320.0);
            {
                let _font = ui.push_font(fonts.body);
                let _pad = ui.push_style_var(StyleVar::FramePadding([6.0, 6.0]));
                ui.input_text("##bench_input", bench).build();
            }

            ui.set_cursor_screen_pos([380.0, 724.0]);
            {
                let _font = ui.push_font(fonts.body);
                let _b = ui.push_style_color(StyleColor::Button, RAISED);
                let _bh = ui.push_style_color(StyleColor::ButtonHovered, BORDER);
                let _ba = ui.push_style_color(StyleColor::ButtonActive, BORDER);
                let _t = ui.push_style_color(StyleColor::Text, TEXT);
                ui.button_with_size("Bench button", [140.0, 36.0]);
            }

            ui.set_cursor_screen_pos([540.0, 724.0]);
            {
                let _font = ui.push_font(fonts.body);
                let _b = ui.push_style_color(StyleColor::Button, RAISED);
                let _bh = ui.push_style_color(StyleColor::ButtonHovered, BORDER);
                let _ba = ui.push_style_color(StyleColor::ButtonActive, BORDER);
                let _t = ui.push_style_color(StyleColor::Text, TEXT);
                if ui.button_with_size("Bench menu", [140.0, 36.0]) {
                    *menu_open = !*menu_open;
                }
            }

            if *menu_open {
                ui.set_cursor_screen_pos([540.0, 600.0]);
                let _bg = ui.push_style_color(StyleColor::ChildBg, CARD);
                ui.child_window("##bench_menu")
                    .size([180.0, 96.0])
                    .border(true)
                    .build(|| {
                        text(ui, fonts.body, TEXT, "Bench item 1");
                        text(ui, fonts.body, TEXT, "Bench item 2");
                        text(ui, fonts.body, TEXT, "Bench item 3");
                    });
            }
        });
}

fn render_frame(gfx: &mut Gfx, bench: &mut String, menu_open: &mut bool) {
    let now = Instant::now();
    gfx.context.io_mut().update_delta_time(now - gfx.last_frame);
    gfx.last_frame = now;

    let frame = match gfx.surface.get_current_texture() {
        wgpu::CurrentSurfaceTexture::Success(frame) => frame,
        wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
        wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => return,
        wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
            gfx.surface.configure(&gfx.device, &gfx.config);
            return;
        }
        other => {
            eprintln!("get_current_texture: {other:?}");
            return;
        }
    };

    gfx.platform
        .prepare_frame(gfx.context.io_mut(), &*gfx.window)
        .expect("prepare_frame");

    let ui = gfx.context.frame();
    draw_start(
        &*ui,
        &gfx.fonts,
        gfx.scale,
        bench,
        menu_open,
        gfx.show_fallback,
    );
    if gfx.last_cursor != ui.mouse_cursor() {
        gfx.last_cursor = ui.mouse_cursor();
        gfx.platform.prepare_render(&*ui, &*gfx.window);
    }

    let mut encoder = gfx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("guimaker-compare-imgui"),
        });
    let view = frame
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("imgui"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &view,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(CLEAR),
                store: wgpu::StoreOp::Store,
            },
            depth_slice: None,
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });

    let draw_data = gfx.context.render();
    gfx.renderer
        .render(draw_data, &gfx.queue, &gfx.device, &mut pass)
        .expect("imgui render");

    drop(pass);
    gfx.queue.submit(Some(encoder.finish()));
    frame.present();
}

struct App {
    gfx: Option<Gfx>,
    started: Instant,
    frames: Arc<AtomicU64>,
    bench_input: String,
    menu_open: bool,
    roboto: bool,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let gfx = create(event_loop, self.roboto);
        gfx.window.request_redraw();
        self.gfx = Some(gfx);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(gfx) = self.gfx.as_mut() else {
            return;
        };

        // Forward every event to imgui so the bench text input and button get
        // hover, focus and typing.
        let forwarded: winit::event::Event<()> = winit::event::Event::WindowEvent {
            window_id,
            event: event.clone(),
        };
        gfx.platform
            .handle_event(gfx.context.io_mut(), &*gfx.window, &forwarded);

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                gfx.config.width = size.width;
                gfx.config.height = size.height;
                gfx.surface.configure(&gfx.device, &gfx.config);
                gfx.window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                let frame = self.frames.fetch_add(1, Ordering::Relaxed) + 1;
                if frame == 1 {
                    println!(
                        "startup_ms {}",
                        self.started.elapsed().as_secs_f64() * 1000.0
                    );
                    let _ = std::io::stdout().flush();
                }
                render_frame(gfx, &mut self.bench_input, &mut self.menu_open);
                if frame == 1 {
                    println!("bench_ready");
                    let _ = std::io::stdout().flush();
                }
            }
            // Event-driven repaint: hover/typing stay visible, but an idle
            // window presents no extra frames.
            WindowEvent::CursorMoved { .. }
            | WindowEvent::CursorEntered { .. }
            | WindowEvent::CursorLeft { .. }
            | WindowEvent::MouseInput { .. }
            | WindowEvent::MouseWheel { .. }
            | WindowEvent::KeyboardInput { .. }
            | WindowEvent::ModifiersChanged(..)
            | WindowEvent::Ime(..) => {
                gfx.window.request_redraw();
            }
            _ => {}
        }
    }
}

fn parse_arg(name: &str) -> Option<String> {
    std::env::args().skip_while(|arg| arg != name).nth(1)
}

fn main() {
    let seconds = parse_arg("--seconds")
        .and_then(|value| value.parse().ok())
        .unwrap_or(8.0);
    let fonts = parse_arg("--fonts").unwrap_or_else(|| "roboto".to_string());
    println!("backend wgpu");
    println!("present_mode fifo");
    println!("fonts {}", fonts);

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Wait);

    let frames = Arc::new(AtomicU64::new(0));
    let counter = Arc::clone(&frames);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs_f64(seconds));
        println!("frames {}", counter.load(Ordering::Relaxed));
        let _ = std::io::stdout().flush();
        std::process::exit(0);
    });

    let mut app = App {
        gfx: None,
        started: Instant::now(),
        frames,
        bench_input: String::new(),
        menu_open: false,
        roboto: fonts != "default",
    };

    event_loop.run_app(&mut app).unwrap();
}
