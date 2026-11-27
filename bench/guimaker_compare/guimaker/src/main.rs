//! GUIMaker side of the Phase 9D comparison: the start-screen recipe plus the
//! shared bench controls (one editable field, one hoverable button and one
//! toggleable menu).
//!
//! Every app follows the monitor DPI: the engine takes *physical* pixels, so
//! the harness passes `BENCH_DPI_SCALE` (the primary monitor's effective DPI)
//! and this app multiplies the 1280x800 logical spec by it. No forced scale.
//!
//! Run with `--seconds N` and `--fonts roboto|default`; the harness measures
//! presents from outside the process and reads the startup trace / text count
//! as a cross-check only.

use std::io::Write;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use ferrous_engine::{App, AppContext, AppMode, RedrawPolicy};
use guimaker::components::{button, ButtonVariant, TextFieldModel};
use guimaker::input::text_editor::TextEditor;
use guimaker::recipe::{start_screen, StartMsg};
use guimaker::style::{Align, Justify, SizeSpec};
use guimaker::theme::{Spacing, Theme};
use guimaker::view::{text, Element};
use guimaker_gallery::host::{GuiApp, GuiHost};

const LOGICAL_WIDTH: f64 = 1280.0;
const LOGICAL_HEIGHT: f64 = 800.0;

struct Model {
    started: Instant,
    seconds: f64,
    frames: Arc<AtomicU64>,
    text_chars: Arc<AtomicU64>,
    /// The standard editable field: the editor owns text, caret, selection and
    /// undo. `on_text` feeds it; `view_with_focus` draws it (prompt 15 Phase 9D
    /// finding 4, replacing the string + flag workaround).
    field: TextFieldModel,
    field_focused: bool,
    bench_clicks: u64,
    menu_open: bool,
    /// When measured with each library's own default font, draw a fixed CJK +
    /// emoji sample so the default font's fallback cost is part of RSS.
    show_fallback_text: bool,
}

impl Model {
    fn new(seconds: f64, show_fallback_text: bool) -> Self {
        let frames = Arc::new(AtomicU64::new(0));
        let text_chars = Arc::new(AtomicU64::new(0));
        let counter = Arc::clone(&frames);
        let chars = Arc::clone(&text_chars);
        // A watchdog prints the count and exits; it does not schedule a frame
        // (the old `redraw_at` wake was one of the three idle frames).
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs_f64(seconds));
            println!("frames {}", counter.load(Ordering::Relaxed));
            println!("bench_text_chars {}", chars.load(Ordering::Relaxed));
            let _ = std::io::stdout().flush();
            std::process::exit(0);
        });
        Self {
            started: Instant::now(),
            seconds,
            frames,
            text_chars,
            field: TextFieldModel::new(TextEditor::new("")),
            field_focused: false,
            bench_clicks: 0,
            menu_open: false,
            show_fallback_text,
        }
    }

    /// Bench-only strip, at the same logical coordinates as every other app:
    /// input x=40..360, button x=380..520, menu x=540..680,
    /// surface y=height-76..height-40.
    fn bench_strip(&self) -> Element<StartMsg> {
        let field = self
            .field
            .view_with_focus(self.field_focused, |_| StartMsg::Settings)
            .width(SizeSpec::Px(320.0));
        let action = button(
            "Bench button",
            ButtonVariant::Secondary,
            StartMsg::Browse,
        )
        .width(SizeSpec::Px(140.0));
        // The bench menu reuses the Docs message because the start-screen
        // recipe's `StartMsg` is fixed; the menu is a bench-only control.
        let menu = button("Bench menu", ButtonVariant::Secondary, StartMsg::Docs)
            .width(SizeSpec::Px(140.0));
        Element::row()
            .key("bench_strip")
            .gap(Spacing::S5)
            .align(Align::Center)
            .child(field)
            .child(action)
            .child(menu)
    }

    fn bench_menu(&self) -> Element<StartMsg> {
        let mut column = Element::column()
            .key("bench_menu")
            .width(SizeSpec::Px(180.0))
            .gap(Spacing::S2);
        for label in ["Bench item 1", "Bench item 2", "Bench item 3"] {
            column = column.child(button(label, ButtonVariant::Ghost, StartMsg::Docs));
        }
        column
    }
}

impl GuiApp for Model {
    type Msg = StartMsg;

    fn view(&self) -> Element<Self::Msg> {
        let mut bottom = Element::column()
            .width(SizeSpec::Fill)
            .height(SizeSpec::Fill)
            .justify(Justify::End)
            .align(Align::Stretch)
            .pad(Spacing::S10);
        if self.menu_open {
            bottom = bottom.child(self.bench_menu());
        }
        if self.show_fallback_text {
            bottom = bottom.child(text("漢字 🎬 ✨"));
        }
        bottom = bottom.child(self.bench_strip());

        Element::stack()
            .key("bench_root")
            .width(SizeSpec::Fill)
            .height(SizeSpec::Fill)
            // 11:42, the instant the committed layout snapshots use.
            .child(start_screen(42_120_000.0))
            .child(bottom)
    }

    fn update(&mut self, message: Self::Msg) {
        match message {
            // The field's click message focuses the real editor.
            StartMsg::Settings => self.field_focused = true,
            StartMsg::Browse => self.bench_clicks = self.bench_clicks.saturating_add(1),
            // Bench menu toggle (see `bench_strip`).
            StartMsg::Docs => self.menu_open = !self.menu_open,
            StartMsg::NewProject | StartMsg::OpenRecent(_) => {}
        }
    }

    fn theme(&self) -> Theme {
        Theme::dark()
    }

    fn on_text(&mut self, text: &str) {
        if self.field_focused {
            self.field.insert_text(text, 0.0);
            self.text_chars.store(
                self.field.editor.text().chars().count() as u64,
                Ordering::Relaxed,
            );
        }
    }

    fn tick(&mut self, ctx: &mut AppContext<'_>) {
        let frame = self.frames.fetch_add(1, Ordering::Relaxed) + 1;
        if frame == 1 {
            println!("bench_ready");
            let _ = std::io::stdout().flush();
        }
        if self.started.elapsed().as_secs_f64() >= self.seconds {
            ctx.request_exit();
        }
    }
}

fn parse_arg(name: &str) -> Option<String> {
    std::env::args().skip_while(|arg| arg != name).nth(1)
}

fn bench_scale() -> f64 {
    std::env::var("BENCH_DPI_SCALE")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value: &f64| *value > 0.0)
        .unwrap_or(1.0)
}

fn main() {
    let seconds = parse_arg("--seconds")
        .and_then(|value| value.parse().ok())
        .unwrap_or(8.0);
    let fonts = parse_arg("--fonts").unwrap_or_else(|| "roboto".to_string());
    let scale = bench_scale();
    let width = (LOGICAL_WIDTH * scale).round().max(1.0) as u32;
    let height = (LOGICAL_HEIGHT * scale).round().max(1.0) as u32;
    println!("backend wgpu");
    println!("present_mode fifo");
    println!("fonts {}", fonts);

    let mut app = App::new(GuiHost::new(Model::new(seconds, fonts == "default")))
        .with_title("guimaker-compare-guimaker")
        .with_size(width, height)
        // Same window chrome as the other apps, so PrintWindow captures have
        // the same bounds; the resize scenario still resizes programmatically.
        .with_resizable(false)
        .with_mode(AppMode::Flat2D)
        .with_vsync(true)
        .with_redraw_policy(RedrawPolicy::OnDemand)
        .with_redraw_on_pointer_move(false)
        .with_startup_trace(true);
    if fonts != "default" {
        app = app.with_font_bytes(include_bytes!(
            "../../../../assets/fonts/Roboto-Regular.ttf"
        ));
    }
    app.run();
}
