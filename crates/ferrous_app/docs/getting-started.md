# Getting Started

This guide walks you through creating a desktop window with interactive GUI
widgets using FerrousEngine, from an empty project to a running app.

---

## 1. Project setup

Create a new binary crate and add FerrousEngine as a path dependency:

```toml
# Cargo.toml
[package]
name    = "my_ui_app"
version = "0.1.0"
edition = "2021"

[dependencies]
ferrous_app = { path = "../FerrousEngine/crates/ferrous_app" }
ferrous_gui = { path = "../FerrousEngine/crates/ferrous_gui" }
# ferrous_assets is already re-exported through ferrous_app; add it only if
# you need AssetServer or Font types in your own type signatures.
```

> **Workspace tip** — if your app lives inside the same Cargo workspace as
> FerrousEngine, add it to the `[workspace] members` list in the root
> `Cargo.toml` and use `{ path = "../ferrous_app" }`.

---

## 2. Implement `FerrousApp`

Your application is any struct that implements
[`FerrousApp`](ferrous-app-trait.md).  All six methods are optional.

```rust
// src/main.rs
use ferrous_app::{App, AppContext, Color, FerrousApp, KeyCode};
use ferrous_assets::Font;
use ferrous_gui::{Button, GuiBatch, Slider, TextBatch, Ui};

// ── Application state ────────────────────────────────────────────────────────

struct MyApp {
    counter: u32,
    btn:     Button,
    slider:  Slider,
}

impl Default for MyApp {
    fn default() -> Self {
        Self {
            counter: 0,
            // Button at (x=20, y=20), 160×40 px, 6 px corner radius
            btn:    Button::new(20.0, 20.0, 160.0, 40.0).with_radius(6.0),
            // Slider at (x=20, y=80), 300 px wide, initial value 0.5
            slider: Slider::new(20.0, 80.0, 300.0, 24.0, 0.5),
        }
    }
}

// ── FerrousApp implementation ─────────────────────────────────────────────────

impl FerrousApp for MyApp {
    /// Called once after the window + GPU are ready.
    /// Add interactive widgets to `ui` here; they persist for the app lifetime.
    fn configure_ui(&mut self, ui: &mut Ui) {
        ui.add(self.btn.clone());
        ui.add(self.slider.clone());
    }

    /// Called every frame before rendering.  Business logic goes here.
    fn update(&mut self, ctx: &mut AppContext) {
        if ctx.input.just_pressed(KeyCode::Escape) {
            ctx.request_exit();
        }
        if self.btn.pressed {
            self.counter += 1;
            self.btn.pressed = false;       // consume the event
        }
    }

    /// Emit 2-D draw commands.  Called after `update`.
    fn draw_ui(
        &mut self,
        gui:  &mut GuiBatch,
        text: &mut TextBatch,
        font: Option<&Font>,
        _ctx: &mut AppContext,
    ) {
        // Draw widgets manually into the batch for this frame.
        self.btn.draw(gui);
        self.slider.draw(gui);

        // Render a text label (requires a font to be loaded).
        if let Some(f) = font {
            text.push_str(
                &format!("clicks: {}   slider: {:.2}", self.counter, self.slider.value),
                20.0, 130.0, 18.0, [0.9, 0.9, 0.9, 1.0], f,
            );
        }
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    App::new(MyApp::default())
        .with_title("My UI App")
        .with_size(800, 600)
        .with_background_color(Color::rgb(0.08, 0.08, 0.10))
        .with_font("assets/fonts/Roboto-Regular.ttf")  // optional
        .run();                                          // blocks until window closes
}
```

---

## 3. Run

```
cargo run
```

The window opens immediately.  Click the button to increment the counter; drag
the slider thumb to change the value.  Press **Escape** to quit.

---

## 4. Key concepts

### `configure_ui` vs `draw_ui`

| Callback | When called | What to put here |
|----------|-------------|-----------------|
| `configure_ui(ui)` | **Once** at startup | `ui.add(widget)` for interactive widgets that persist |
| `draw_ui(gui, text, font, ctx)` | **Every frame** | Push draw commands into `GuiBatch` / `TextBatch` |

Interactive widgets (`Button`, `Slider`, `TextInput`, `ColorPicker`) need to be
added in `configure_ui` so the engine can route input events to them.  If you
only need to draw static shapes or text you can skip `configure_ui` and push
everything in `draw_ui`.

### Widget input flow

```
winit WindowEvent
    └─▶ Runner::handle_window_event
            ├─▶ InputState (keyboard / mouse state)
            └─▶ Ui::handle_window_event
                    └─▶ Canvas dispatches to each Widget
                            └─▶ widget.mouse_input / keyboard_input
                                    └─▶ widget.pressed / widget.value / widget.text updated
```

Your `update` callback runs **after** all input has been processed, so
`self.btn.pressed` and `self.slider.value` are already up to date.

### Font loading

Fonts are optional.  Without one, `draw_ui` receives `font: None` and text
rendering is skipped.  Supply a path with `.with_font("path/to/font.ttf")` on
`App`, or embed bytes with `.with_font_bytes(include_bytes!("..."))` for
cross-platform / WASM builds.

---

## 5. Next steps

- **More widgets** → [Button](../ferrous_gui/widgets/button.md),
  [Slider](../ferrous_gui/widgets/slider.md),
  [TextInput](../ferrous_gui/widgets/textinput.md),
  [ColorPicker](../ferrous_gui/widgets/color_picker.md),
  [Container](../ferrous_gui/widgets/container.md)
- **Declarative layout** → [Row / Column / UiButton / Text](../ferrous_gui/layout.md)
- **App configuration** → [App Builder reference](app-builder.md)
- **All frame callbacks** → [FerrousApp trait](ferrous-app-trait.md)
- **Input, time, world** → [AppContext reference](app-context.md)
- **Render styles / materials** → [RenderContext reference](render-context.md)
