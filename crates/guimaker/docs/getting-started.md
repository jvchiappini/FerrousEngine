# Getting started

GUIMaker draws nothing itself: `guimaker` builds the view, `guimaker_ferrous`
maps the display list onto the engine batch, and `guimaker_gallery` supplies the
`FerrousApp` host that owns the window, the font atlas and the redraw policy.

The smallest real program is one model, one view and the gallery host:

```rust
use ferrous_engine::{App, AppMode};
use guimaker::prelude::*;
use guimaker_gallery::host::{GuiApp, GuiHost};

struct Counter { clicks: u32 }

impl GuiApp for Counter {
    type Msg = ();
    fn view(&self) -> Element<()> {
        Element::column()
            .key("root")
            .pad(Spacing::S6)
            .gap(Spacing::S4)
            .bg(ColorRole::Bg)
            .child(heading("Hello, GUIMaker", 1, None))
            .child(text(format!("Clicked {} times", self.clicks)).tone(Tone::Muted))
            .child(button("Click me", ButtonVariant::Primary, ()))
    }
    fn update(&mut self, _: Self::Msg) { self.clicks += 1; }
}

fn main() {
    App::new(GuiHost::new(Counter { clicks: 0 }))
        .with_title("Hello")
        .with_size(800, 600)
        .with_mode(AppMode::Flat2D)
        .with_font_bytes(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/fonts/Roboto-Regular.ttf")))
        .run();
}
```

That is 29 lines including the imports. Place the file under `crates/<app>/src/`
next to a `Cargo.toml` that lists `ferrous_engine`, `guimaker` and
`guimaker_gallery` as path dependencies. The same code is the body of
`crates/guimaker_gallery/examples/host.rs`; run the real one with:

```text
cargo run -p guimaker_gallery --example host
```

The host owns the window and the font atlas, calls `Ui::paint` once per frame,
and feeds every routed message back to `GuiApp::update`. Read
[concepts.md](concepts.md) next, then [components.md](components.md) and
[design-guide.md](design-guide.md) before writing a real screen.
