//! egui side of the Phase 9B comparison: the start-screen recipe on
//! eframe 0.29 / egui 0.29 through wgpu. Run with `--seconds N` (default 8); the harness
//! samples the process and reads `frames N` from stdout on exit.
//!
//! DPI: no forced zoom. The window is created with a 1280x800 *logical* inner
//! size and egui uses the monitor's own pixels-per-point. The swapchain is FIFO,
//! two images deep, and the wgpu backend comes from `WGPU_BACKEND` (set by the
//! harness per tier). `--fonts default` keeps the library default font.

use std::io::Write;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use eframe::egui;
use egui::{Align, Color32, Layout, Margin, RichText, Rounding, Stroke, Vec2};

const ROBOTO: &[u8] = include_bytes!("../../../../assets/fonts/Roboto-Regular.ttf");

const PAGE: Color32 = Color32::from_rgb(0x06, 0x08, 0x0D);
const CARD: Color32 = Color32::from_rgb(0x0F, 0x12, 0x17);
const WELL: Color32 = Color32::from_rgb(0x0B, 0x0D, 0x13);
const RAISED: Color32 = Color32::from_rgb(0x18, 0x1B, 0x20);
const BORDER: Color32 = Color32::from_rgb(0x26, 0x29, 0x2F);
const TEXT: Color32 = Color32::from_rgb(0xF1, 0xF5, 0xFE);
const MUTED: Color32 = Color32::from_rgb(0xAD, 0xB1, 0xB9);
const FAINT: Color32 = Color32::from_rgb(0x71, 0x75, 0x7C);
const ACCENT: Color32 = Color32::from_rgb(0x27, 0x50, 0xC7);

struct CompareApp {
    started: Instant,
    seconds: f64,
    frames: Arc<AtomicU64>,
    bench_input: String,
    menu_open: bool,
    show_fallback_text: bool,
}

impl CompareApp {
    fn new(seconds: f64, show_fallback_text: bool) -> Self {
        let frames = Arc::new(AtomicU64::new(0));
        // A watchdog, because eframe's idle repaint is not guaranteed to fire
        // the close request when the window has no input.
        let counter = Arc::clone(&frames);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs_f64(seconds));
            println!("frames {}", counter.load(Ordering::Relaxed));
            let _ = std::io::stdout().flush();
            std::process::exit(0);
        });
        Self {
            started: Instant::now(),
            seconds,
            frames,
            bench_input: String::new(),
            menu_open: false,
            show_fallback_text,
        }
    }

    /// Bench-only control strip: the same logical coordinates in every app, so
    /// the harness can park the pointer on a known point.
    fn bench_strip(&mut self, ctx: &egui::Context) {
        egui::Area::new(egui::Id::new("bench_input_label"))
            .fixed_pos(egui::pos2(40.0, 700.0))
            .interactable(false)
            .show(ctx, |ui| {
                ui.label(RichText::new("Bench input").size(12.0).color(MUTED));
            });

        if self.show_fallback_text {
            egui::Area::new(egui::Id::new("fallback_sample"))
                .fixed_pos(egui::pos2(40.0, 676.0))
                .interactable(false)
                .show(ctx, |ui| {
                    ui.label(RichText::new("漢字 🎬 ✨").size(13.0).color(MUTED));
                });
        }

        egui::Area::new(egui::Id::new("bench_strip"))
            .fixed_pos(egui::pos2(40.0, 724.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // Exact placement: no implicit item spacing between widgets.
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let edit = egui::TextEdit::singleline(&mut self.bench_input)
                        .desired_width(320.0)
                        .text_color(TEXT);
                    ui.add_sized([320.0, 36.0], edit);
                    ui.add_space(20.0);
                    let (rect, response) =
                        ui.allocate_exact_size(Vec2::new(140.0, 36.0), egui::Sense::click());
                    let bg = if response.hovered() { BORDER } else { RAISED };
                    ui.painter().rect_filled(rect, 8.0, bg);
                    ui.painter()
                        .rect_stroke(rect, 8.0, Stroke::new(1.0, BORDER));
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Bench button",
                        egui::FontId::proportional(14.0),
                        TEXT,
                    );
                    ui.add_space(20.0);
                    let (menu_rect, menu_response) =
                        ui.allocate_exact_size(Vec2::new(140.0, 36.0), egui::Sense::click());
                    let menu_bg = if menu_response.hovered() { BORDER } else { RAISED };
                    ui.painter().rect_filled(menu_rect, 8.0, menu_bg);
                    ui.painter()
                        .rect_stroke(menu_rect, 8.0, Stroke::new(1.0, BORDER));
                    ui.painter().text(
                        menu_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Bench menu",
                        egui::FontId::proportional(14.0),
                        TEXT,
                    );
                    if menu_response.clicked() {
                        self.menu_open = !self.menu_open;
                    }
                });
            });

        if self.menu_open {
            egui::Area::new(egui::Id::new("bench_menu"))
                .fixed_pos(egui::pos2(
                    540.0,
                    ctx.screen_rect().height() - 176.0,
                ))
                .show(ctx, |ui| {
                    egui::Frame::none()
                        .fill(CARD)
                        .stroke(Stroke::new(1.0, BORDER))
                        .rounding(Rounding::same(6.0))
                        .inner_margin(Margin::same(8.0))
                        .show(ui, |ui| {
                            for item in ["Bench item 1", "Bench item 2", "Bench item 3"] {
                                ui.label(RichText::new(item).size(14.0).color(TEXT));
                            }
                        });
                });
        }
    }
}

impl eframe::App for CompareApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let frame = self.frames.fetch_add(1, Ordering::Relaxed) + 1;
        if frame == 1 {
            println!(
                "startup_ms {}",
                self.started.elapsed().as_secs_f64() * 1000.0
            );
            println!("bench_ready");
        }

        if self.started.elapsed().as_secs_f64() >= self.seconds {
            ctx.send_viewport_cmd(egui::viewport::ViewportCommand::Close);
        } else if frame == 1 {
            // One wake at the end, not a continuous repaint: the idle window
            // must present no frames between the first and the closing frame.
            let remaining = self.seconds - self.started.elapsed().as_secs_f64();
            ctx.request_repaint_after(Duration::from_secs_f64(remaining.max(0.0)));
        }

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(PAGE)
                    .inner_margin(Margin {
                        left: 28.0,
                        right: 28.0,
                        top: 18.0,
                        bottom: 18.0,
                    }),
            )
            .show(ctx, |ui| start_screen(ui));

        self.bench_strip(ctx);
    }
}



fn start_screen(ui: &mut egui::Ui) {
    // 1. Wrapping header: brand + tagline left, links right.
    ui.horizontal_wrapped(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new("Motioner").size(24.0).strong().color(TEXT));
            ui.label(
                RichText::new("Code-driven motion graphics")
                    .size(13.0)
                    .color(MUTED),
            );
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            link(ui, "Docs");
            link(ui, "Settings");
        });
    });

    // 2. Divider.
    ui.add_space(10.0);
    ui.separator();
    ui.add_space(12.0);

    // 3. Two-column body (55% / 45%), 4. footer.
    let footer_h = 26.0;
    let body_h = (ui.available_height() - footer_h).max(0.0);
    let full_w = ui.available_width();
    let gap = 20.0;
    let recent_w = (full_w - gap) * 0.55;
    let form_w = full_w - gap - recent_w;

    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            Vec2::new(recent_w, body_h),
            Layout::top_down(Align::Min),
            |ui| recent_projects(ui),
        );
        ui.add_space(gap);
        ui.allocate_ui_with_layout(
            Vec2::new(form_w, body_h),
            Layout::top_down(Align::Min),
            |ui| new_project_form(ui),
        );
    });

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("FerrousEngine 0.1")
                .size(12.0)
                .color(FAINT),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new("Apache-2.0").size(12.0).color(FAINT));
        });
    });
}

fn recent_projects(ui: &mut egui::Ui) {
    ui.label(
        RichText::new("Recent projects")
            .size(18.0)
            .strong()
            .color(TEXT),
    );
    ui.add_space(8.0);
    for n in 1..=5 {
        card(ui, n);
    }
}

fn card(ui: &mut egui::Ui, n: usize) {
    egui::Frame::none()
        .fill(CARD)
        .stroke(Stroke::new(1.0, BORDER))
        .rounding(Rounding::same(8.0))
        .inner_margin(Margin {
            left: 14.0,
            right: 14.0,
            top: 10.0,
            bottom: 10.0,
        })
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(format!("Project {n}"))
                            .size(15.0)
                            .color(TEXT),
                    );
                    ui.label(RichText::new("Edited recently").size(12.0).color(MUTED));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new("11:42").size(12.0).color(FAINT));
                });
            });
        });
    ui.add_space(10.0);
}

fn new_project_form(ui: &mut egui::Ui) {
    ui.label(
        RichText::new("New project")
            .size(18.0)
            .strong()
            .color(TEXT),
    );
    ui.add_space(4.0);
    ui.label(
        RichText::new(
            "Start from a blank canvas or a starter composition,\nthen refine every keyframe.",
        )
        .size(13.0)
        .color(MUTED),
    );
    ui.add_space(10.0);

    field(ui, "Name", "Untitled");
    field(ui, "Size", "1920 x 1080");
    field(ui, "Frame rate", "60 fps");
    field(ui, "Engine", "FerrousEngine");

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let create = egui::Button::new(RichText::new("Create").size(14.0).color(TEXT))
            .fill(ACCENT)
            .rounding(Rounding::same(8.0));
        ui.add_sized([110.0, 36.0], create);

        let browse = egui::Button::new(RichText::new("Browse").size(14.0).color(TEXT))
            .fill(RAISED)
            .stroke(Stroke::new(1.0, BORDER))
            .rounding(Rounding::same(8.0));
        ui.add_sized([110.0, 36.0], browse);
    });
}

fn field(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(RichText::new(label).size(12.0).color(MUTED));
    egui::Frame::none()
        .fill(WELL)
        .stroke(Stroke::new(1.0, BORDER))
        .rounding(Rounding::same(6.0))
        .inner_margin(Margin {
            left: 12.0,
            right: 12.0,
            top: 8.0,
            bottom: 8.0,
        })
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new(value).size(14.0).color(TEXT));
        });
    ui.add_space(6.0);
}

fn link(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(14.0).color(MUTED));
}

fn parse_arg(name: &str) -> Option<String> {
    std::env::args().skip_while(|arg| arg != name).nth(1)
}

fn main() -> eframe::Result<()> {
    let seconds = parse_arg("--seconds")
        .and_then(|value| value.parse().ok())
        .unwrap_or(8.0);
    let fonts = parse_arg("--fonts").unwrap_or_else(|| "roboto".to_string());
    println!("backend wgpu");
    println!("present_mode fifo");
    println!("fonts {}", fonts);

    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("guimaker-compare-egui-wgpu")
            .with_inner_size([1280.0, 800.0])
            .with_resizable(false),
        renderer: eframe::Renderer::Wgpu,
        vsync: true,
        multisampling: 0,
        ..Default::default()
    };
    options.wgpu_options = eframe::egui_wgpu::WgpuConfiguration {
        present_mode: eframe::wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: Some(2),
        ..Default::default()
    };

    let load_roboto = fonts != "default";
    eframe::run_native(
        "guimaker-compare-egui-wgpu",
        options,
        Box::new(move |cc| {
            if load_roboto {
                let mut definitions = egui::FontDefinitions::default();
                definitions
                    .font_data
                    .insert("roboto".to_owned(), egui::FontData::from_static(ROBOTO));
                definitions
                    .families
                    .get_mut(&egui::FontFamily::Proportional)
                    .expect("proportional family")
                    .insert(0, "roboto".to_owned());
                cc.egui_ctx.set_fonts(definitions);
            }

            let mut visuals = egui::Visuals::dark();
            visuals.panel_fill = PAGE;
            visuals.window_fill = PAGE;
            visuals.override_text_color = Some(TEXT);
            visuals.widgets.noninteractive.bg_fill = CARD;
            visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
            visuals.widgets.inactive.bg_fill = RAISED;
            visuals.widgets.hovered.bg_fill = BORDER;
            visuals.widgets.active.bg_fill = BORDER;
            visuals.selection.bg_fill = ACCENT;
            cc.egui_ctx.set_visuals(visuals);

            let app: Box<dyn eframe::App> =
                Box::new(CompareApp::new(seconds, fonts == "default"));
            Ok(app)
        }),
    )
}
