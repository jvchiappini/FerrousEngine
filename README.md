# FerrousEngine

**FerrousEngine** is a modular Rust framework for building desktop applications,
editors, and tools. Documentation lives at
[jvchiappini.github.io/FerrousEngine](https://jvchiappini.github.io/FerrousEngine/).

> To build a **UI/GUI desktop application** start with
> [Getting Started](ferrous_engine/getting-started.md) and the
> [ferrous_ui_core widget reference](ferrous_ui_core/README.md).

---

**FerrousEngine** is a modular Rust framework for building desktop applications,
editors, and games. It has a strict multi-crate architecture that keeps the
application lifecycle, rendering, UI, and asset management fully decoupled.

## Crate map

| Crate | Purpose | Key types |
|-------|---------|-----------|
| `ferrous_engine` | **Application shell** — window, event loop, frame callbacks | `App<T>`, `FerrousApp`, `AppContext`, `RenderContext` |
| `ferrous_2d` | 2D shapes, sprites and 2D ECS components | `ShapeInstance`, `SpriteInstance`, `Transform2d` |
| `ferrous_ui_core` | UI widget toolkit and layout primitives | `Widget`, `Button`, `Slider`, `UiTree` |
| `ferrous_gui` | UI facade (widgets + layout + rendering) | `UiSystem`, `GuiBatch` |
| `ferrous_layout` | Layout engine | `Style`, `Node` |
| `ferrous_renderer` | GPU rendering (wgpu) | `Renderer`, `RenderPass`, `RenderStyle`, `MaterialDescriptor` |
| `ferrous_core` | Headless math/ECS primitives | `World`, `Transform`, `Color`, `InputState`, `KeyCode` |
| `ferrous_assets` | Asset loading & caching | `AssetServer`, `Font`, `GltfModel`, `Texture2d` |
| `ferrous_ecs` | Entity-Component-System | `Entity`, `Stage`, `StagedScheduler`, `System` |

## Building a UI application — minimum viable recipe

```toml
# Cargo.toml
[dependencies]
ferrous_engine = { path = "path/to/FerrousEngine/crates/ferrous_engine" }
ferrous_gui    = { path = "path/to/FerrousEngine/crates/ferrous_gui"    }
```

```rust
use ferrous_engine::{App, AppContext, Color, FerrousApp, KeyCode};
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

- **[Getting Started](ferrous_engine/getting-started.md)** — step-by-step from zero to running app
- **[App Builder](ferrous_engine/app-builder.md)** — `App<T>` fluent API, all `with_*` options
- **[FerrousApp Trait](ferrous_engine/ferrous-app-trait.md)** — the frame callbacks
- **[AppContext](ferrous_engine/app-context.md)** — everything inside `ctx`
- **[UI Overview](ferrous_ui_core/README.md)** — widgets and the UI tree
- **[Widgets](ferrous_ui_core/widgets/button.md)** — Button, Slider, TextInput, ColorPicker, Panel
- **[Layout](ferrous_layout/LAYOUT.md)** — layout system reference
- **[2D Rendering](ferrous_2d/README.md)** — shapes, sprites and 2D components
- **[Renderer](ferrous_renderer/README.md)** — materials, camera, custom passes

## How the documentation site is built

Every crate in `crates/*/docs/` is aggregated by `scripts/build_docs.sh` into a
root `docs/` folder, then rendered with
[MkDocs Material](https://squidfunk.github.io/mkdocs-material/) and deployed to
GitHub Pages on every push to `main`.
