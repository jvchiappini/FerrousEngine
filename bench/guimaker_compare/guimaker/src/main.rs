//! GUIMaker side of the Phase 9C comparison: the start-screen recipe plus the
//! shared bench controls (one editable field and one hoverable button).
//! Run with `--seconds N`; the harness samples the process and reads the
//! startup trace from stderr.

use std::io::Write;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use ferrous_engine::{App, AppContext, AppMode, RedrawPolicy};
use guimaker::components::{button, text_field, ButtonVariant};
use guimaker::recipe::{start_screen, StartMsg};
use guimaker::style::{Align, Justify, SizeSpec};
use guimaker::theme::{Spacing, Theme};
use guimaker::view::Element;
use guimaker_gallery::host::{GuiApp, GuiHost};

struct Model {
    started: Instant,
    seconds: f64,
    frames: Arc<AtomicU64>,
    text_chars: Arc<AtomicU64>,
    bench_text: String,
    bench_focused: bool,
    bench_clicks: u64,
}

impl Model {
    fn new(seconds: f64) -> Self {
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
            bench_text: String::new(),
            bench_focused: false,
            bench_clicks: 0,
        }
    }

    /// Bench-only strip, at the same logical coordinates as every other app:
    /// input x=40..360, button x=380..520, surface y=720..760.
    fn bench_strip(&self) -> Element<StartMsg> {
        let field = text_field(
            Some("Bench input"),
            "Type here",
            &self.bench_text,
            self.bench_focused,
            None,
            false,
            None,
            None,
            None,
            None,
            StartMsg::Settings,
        )
        .width(SizeSpec::Px(320.0));
        let action = button(
            "Bench button",
            ButtonVariant::Secondary,
            StartMsg::Browse,
        )
        .width(SizeSpec::Px(140.0));
        Element::row()
            .key("bench_strip")
            .gap(Spacing::S5)
            .align(Align::Center)
            .child(field)
            .child(action)
    }
}

impl GuiApp for Model {
    type Msg = StartMsg;

    fn view(&self) -> Element<Self::Msg> {
        Element::stack()
            .key("bench_root")
            .width(SizeSpec::Fill)
            .height(SizeSpec::Fill)
            // 11:42, the instant the committed layout snapshots use.
            .child(start_screen(42_120_000.0))
            .child(
                Element::column()
                    .width(SizeSpec::Fill)
                    .height(SizeSpec::Fill)
                    .justify(Justify::End)
                    .align(Align::Stretch)
                    .pad(Spacing::S10)
                    .child(self.bench_strip()),
            )
    }

    fn update(&mut self, message: Self::Msg) {
        match message {
            StartMsg::Settings => self.bench_focused = true,
            StartMsg::Browse => self.bench_clicks = self.bench_clicks.saturating_add(1),
            StartMsg::NewProject | StartMsg::OpenRecent(_) | StartMsg::Docs => {}
        }
    }

    fn theme(&self) -> Theme {
        Theme::dark()
    }

    fn on_text(&mut self, text: &str) {
        if self.bench_focused {
            self.bench_text
                .extend(text.chars().filter(|ch| !ch.is_control()));
            self.text_chars
                .store(self.bench_text.chars().count() as u64, Ordering::Relaxed);
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

fn main() {
    let seconds = std::env::args()
        .skip_while(|arg| arg != "--seconds")
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(8.0);

    App::new(GuiHost::new(Model::new(seconds)))
        .with_title("guimaker-compare-guimaker")
        // 1280x800 logical at the monitor scale (1.25 here); the other apps
        // force 1.5, so the physical pixels differ while the logical layout is
        // the same.
        .with_size(1600, 1000)
        .with_mode(AppMode::Flat2D)
        .with_vsync(true)
        .with_redraw_policy(RedrawPolicy::OnDemand)
        .with_redraw_on_pointer_move(false)
        .with_font_bytes(include_bytes!(
            "../../../../assets/fonts/Roboto-Regular.ttf"
        ))
        .with_startup_trace(true)
        .run();
}
