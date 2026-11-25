//! GUIMaker side of the Phase 9B comparison: the start-screen recipe on the
//! engine host. Run with `--seconds N`; the harness samples the process and
//! reads the startup trace from stderr.

use std::time::{Duration, Instant};

use ferrous_engine::{App, AppContext, AppMode, RedrawPolicy};
use guimaker::recipe::{start_screen, StartMsg};
use guimaker::theme::Theme;
use guimaker::view::Element;
use guimaker_gallery::host::{GuiApp, GuiHost};

struct Model {
    started: Instant,
    seconds: f64,
    frames: u64,
    exit_armed: bool,
}

impl Model {
    fn new(seconds: f64) -> Self {
        Self {
            started: Instant::now(),
            seconds,
            frames: 0,
            exit_armed: false,
        }
    }
}

impl GuiApp for Model {
    type Msg = StartMsg;

    fn view(&self) -> Element<Self::Msg> {
        // 11:42, the instant the committed layout snapshots use.
        start_screen(42_120_000.0)
    }

    fn update(&mut self, _message: Self::Msg) {}

    fn theme(&self) -> Theme {
        Theme::dark()
    }

    fn tick(&mut self, ctx: &mut AppContext<'_>) {
        self.frames = self.frames.saturating_add(1);
        // On-demand redraw: one wake at the end, not a frame per tick.
        if !self.exit_armed {
            self.exit_armed = true;
            ctx.redraw_at(Instant::now() + Duration::from_secs_f64(self.seconds));
        }
        if self.started.elapsed().as_secs_f64() >= self.seconds {
            ctx.request_exit();
        }
    }
}

impl Drop for Model {
    fn drop(&mut self) {
        println!("frames {}", self.frames);
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
        .with_size(1280, 800)
        .with_mode(AppMode::Flat2D)
        .with_vsync(true)
        .with_redraw_policy(RedrawPolicy::OnDemand)
        .with_font_bytes(include_bytes!(
            "../../../../assets/fonts/Roboto-Regular.ttf"
        ))
        .with_startup_trace(true)
        .run();
}
