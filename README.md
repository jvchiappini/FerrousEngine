# FerrousEngine

**FerrousEngine** is a modular Rust framework for building desktop applications,
editors, and tools. Documentation lives at
[jvchiappini.github.io/FerrousEngine](https://jvchiappini.github.io/FerrousEngine/).

> To build a **UI/GUI desktop application** start with
> [Getting Started](ferrous_app/getting-started.md) and the
> [ferrous_gui widget reference](ferrous_gui/README.md).

---

**FerrousEngine** is a modular Rust framework for building desktop applications,
editors, and games. It has a strict multi-crate architecture that keeps the
application lifecycle, rendering, UI, and asset management fully decoupled.

## Crate map

| Crate | Purpose | Key types |
|-------|---------|-----------|
| `ferrous_app` | **Application shell** — window, event loop, frame callbacks | `App<T>`, `AppBuilder`, `FerrousApp`, `AppContext` |
| `ferrous_gui` | **2D GUI widget toolkit** | `Ui`, `Button`, `Slider`, `TextInput`, `ColorPicker`, `Container`, `Row`, `Column` |
| `ferrous_renderer` | GPU rendering (wgpu) | `Renderer`, `RenderPass`, `RenderStyle`, `MaterialDescriptor` |
| `ferrous_core` | Headless math/ECS primitives | `World`, `Transform`, `Color`, `InputState`, `KeyCode` |
| `ferrous_assets` | Asset loading & caching | `AssetServer`, `Font`, `GltfModel` |
| `ferrous_ecs` | Entity-Component-System | `Entity`, `Stage`, `StagedScheduler`, `System` |

## Building a UI application — minimum viable recipe

```toml
# Cargo.toml
[dependencies]
ferrous_app = { path = "path/to/FerrousEngine/crates/ferrous_app" }
ferrous_gui  = { path = "path/to/FerrousEngine/crates/ferrous_gui"  }
```

```rust
use ferrous_app::{App, AppContext, Color, FerrousApp, KeyCode};
use ferrous_assets::Font;
use ferrous_gui::{Button, GuiBatch, Slider, TextBatch, Ui};

struct MyApp {
    counter: u32,
    btn: Button,
    slider: Slider,
}

impl Default for MyApp {
    fn default() -> Self {
        Self {
            counter: 0,
            btn:    Button::new(20.0, 20.0, 160.0, 40.0).with_radius(6.0),
            slider: Slider::new(20.0, 80.0, 300.0, 24.0, 0.5),
        }
    }
}

impl FerrousApp for MyApp {
    // configure_ui is called once; add persistent widgets here
    fn configure_ui(&mut self, ui: &mut Ui) {
        ui.add(self.btn.clone());
        ui.add(self.slider.clone());
    }

    fn update(&mut self, ctx: &mut AppContext) {
        if ctx.input.just_pressed(KeyCode::Escape) {
            ctx.request_exit();
        }
        if self.btn.pressed {
            self.counter += 1;
            self.btn.pressed = false;   // consume the click
        }
    }

    fn draw_ui(
        &mut self,
        gui: &mut GuiBatch,
        text: &mut TextBatch,
        font: Option<&Font>,
        _ctx: &mut AppContext,
    ) {
        self.btn.draw(gui);
        self.slider.draw(gui);
        if let Some(f) = font {
            text.push_str(
                &format!("clicks: {}  slider: {:.2}", self.counter, self.slider.value),
                10.0, 130.0, 18.0, [1.0; 4], f,
            );
        }
    }
}

fn main() {
    App::new(MyApp::default())
        .with_title("My UI App")
        .with_size(800, 600)
        .with_background_color(Color::rgb(0.08, 0.08, 0.10))
        .with_font("assets/fonts/Roboto-Regular.ttf")
        .run();
}
```

## Documentation sections

- **[Getting Started](ferrous_app/getting-started.md)** — step-by-step from zero to running app
- **[App Builder](ferrous_app/app-builder.md)** — `App<T>` fluent API, all `with_*` options
- **[FerrousApp Trait](ferrous_app/ferrous-app-trait.md)** — the six frame callbacks
- **[AppContext](ferrous_app/app-context.md)** — everything inside `ctx`
- **[GUI Overview](ferrous_gui/README.md)** — `Ui`, `Canvas`, `Widget`, input routing
- **[Widgets](ferrous_gui/widgets/button.md)** — Button, Slider, TextInput, ColorPicker, Container
- **[Layout](ferrous_gui/layout.md)** — `Row`, `Column`, `Node`, `Style`
- **[Renderer](ferrous_renderer/README.md)** — materials, camera, custom passes

## How the documentation site is built

Every crate in `crates/*/docs/` is aggregated by `scripts/build_docs.sh` into a
root `docs/` folder, then rendered with
[MkDocs Material](https://squidfunk.github.io/mkdocs-material/) and deployed to
GitHub Pages on every push to `main`.
