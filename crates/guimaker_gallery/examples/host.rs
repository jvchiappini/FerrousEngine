//! The smallest possible `GUIMaker` + `FerrousEngine` window.
//!
//! It wires the start-screen recipe into a real `FerrousApp` through
//! [`GuiHost`](guimaker_gallery::host::GuiHost):
//!
//! ```text
//! cargo run -p guimaker_gallery --example host
//! ```
//!
//! Every action in the recipe is printed to stderr by the model's `update`;
//! the screen itself comes from `guimaker::recipe::start_screen`, unchanged.

use ferrous_engine::{App, AppMode};
use guimaker::recipe::{start_screen, StartMsg};
use guimaker::theme::Theme;
use guimaker::view::Element;
use guimaker_gallery::host::{GuiApp, GuiHost};

const ROBOTO: &[u8] = include_bytes!("../../../assets/fonts/Roboto-Regular.ttf");

struct StartModel {
    theme: Theme,
}

impl Default for StartModel {
    fn default() -> Self {
        Self {
            theme: Theme::dark(),
        }
    }
}

impl GuiApp for StartModel {
    type Msg = StartMsg;

    fn view(&self) -> Element<Self::Msg> {
        start_screen(0.0)
    }

    fn update(&mut self, message: Self::Msg) {
        match message {
            StartMsg::NewProject => eprintln!("host example: create project"),
            StartMsg::OpenRecent(index) => eprintln!("host example: open recent {index}"),
            StartMsg::Settings => eprintln!("host example: settings"),
            StartMsg::Docs => eprintln!("host example: docs"),
            StartMsg::Browse => eprintln!("host example: browse"),
        }
    }

    fn theme(&self) -> Theme {
        self.theme.clone()
    }
}

fn main() {
    App::new(GuiHost::new(StartModel::default()))
        .with_title("GUIMaker host adapter")
        .with_size(1280, 800)
        .with_mode(AppMode::Flat2D)
        .with_font_bytes(ROBOTO)
        .run();
}
