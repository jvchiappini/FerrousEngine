# ferrous_gui

`ferrous_gui` is the 2D widget toolkit for FerrousEngine. It provides interactive
widgets, automatic column/row layout via `PanelBuilder`, shared widget handles
(`Rc<RefCell<…>>`), and helpers for integrating with the engine frame loop and
input state.

---

## Module overview

| Module | Key exports | Description |
|--------|-------------|-------------|
| `ui` | `Ui` | Top-level container; add widgets and route input events |
| `canvas` | `Canvas` | Focus-aware widget collection |
| `widget` | `Widget` | Trait every widget implements (`collect`, `hit`, `tooltip`, …) |
| `button` | `Button` | Clickable rectangle with optional centred label, tooltip, `on_click` |
| `slider` | `Slider` | Horizontal drag control with configurable `min`/`max` range and `on_change` |
| `textinput` | `TextInput` | Single-line text field with visual cursor and `on_change` |
| `label` | `Label` | First-class static text label registrable in `Ui` |
| `checkbox` | `Checkbox` | Tick-box with label and `on_change` |
| `dropdown` | `Dropdown` | Drop-down / combo-box with `on_change` |
| `panel` | `PanelBuilder`, `Panel` | Automatic column/row layout; returns shared widget handles |
| `color_picker` | `ColorPicker`, `PickerShape` | HSV colour picker wheel |
| `container` | `Container` | Grouping panel with optional background and scissor clip |
| `layout` | `Node`, `Style`, `Row`, `Column`, `UiButton`, `Text`, `RenderCommand` | Declarative layout + render commands |
| `key` | `GuiKey` | Lightweight key enum (Backspace, Delete, Arrows, Home, End, Enter, …) |
| `viewport_widget` | `ViewportWidget` | Embedded 3D viewport region |
| `renderer` | `GuiBatch`, `TextBatch`, `GuiRenderer` | Low-level draw batches |

---

## Recommended workflow — shared handles via `PanelBuilder`

The cleanest way to build a panel of controls is `PanelBuilder`. It positions
widgets automatically and returns `Rc<RefCell<…>>` handles so your struct and
the `Ui` share a **single copy** of each widget — no manual clone/sync needed.

```rust
use ferrous_gui::{PanelBuilder, Panel, Ui};

struct MyApp {
    panel: Panel,
}

impl Default for MyApp {
    fn default() -> Self {
        let panel = PanelBuilder::column(20.0, 20.0, 200.0)
            .padding(8.0)
            .gap(6.0)
            .with_background([0.1, 0.1, 0.1, 0.9])
            .add_button("Save")
            .add_button("Load")
            .add_slider(0.0, 100.0, 50.0)
            .add_label("Name:")
            .add_text_input("Enter name…")
            .add_checkbox("Enable VSync", true)
            .add_dropdown(vec!["Low", "Medium", "High"], 1)
            .build();

        Self { panel }
    }
}

impl FerrousApp for MyApp {
    fn configure_ui(&mut self, ui: &mut Ui) {
        // Panel implements Widget — add it directly.
        // The Rc handles inside panel.buttons etc. are shared with Ui.
        ui.add(self.panel.clone());
    }

    fn update(&mut self, _ctx: &mut AppContext) {
        // Read from the shared handles — always in sync with Ui input routing.
        if self.panel.buttons[0].borrow().pressed {
            println!("Save clicked!");
        }
        let volume = self.panel.sliders[0].borrow().value;
        let name   = self.panel.text_inputs[0].borrow().text.clone();
        let vsync  = self.panel.checkboxes[0].borrow().checked;
        let preset = self.panel.dropdowns[0].borrow().selected;
    }

    fn draw_ui(&mut self, dc: &mut DrawContext<'_, '_>) {
        // The panel (and all its children) are already in the Ui widget tree;
        // Ui::draw() handles everything — no manual draw calls needed.
    }
}
```

---

## Alternative workflow — individual widgets with callbacks

For simpler cases or when you prefer callbacks over polling:

```rust
let save_btn = Button::new(20.0, 20.0, 120.0, 36.0)
    .with_label("Save")
    .with_radius(6.0)
    .with_tooltip("Save the current file")
    .on_click(|| println!("Saved!"));

let volume = Slider::new(20.0, 70.0, 200.0, 20.0, 80.0)
    .range(0.0, 100.0)
    .on_change(|v| println!("Volume: {v:.0}"));

ui.add(save_btn);
ui.add(volume);
```

---

## `GuiBatch` shape helpers

`GuiBatch` exposes convenience methods so you rarely need to construct `GuiQuad`
manually:

```rust
// Filled rectangle (sharp corners)
dc.gui.rect(x, y, w, h, color);

// Rounded rectangle — uniform radius
dc.gui.rect_r(x, y, w, h, radius, color);

// Rounded rectangle — per-corner radii [tl, tr, bl, br]
dc.gui.rect_radii(x, y, w, h, [4.0, 4.0, 0.0, 0.0], color);

// Line segment
dc.gui.line(x0, y0, x1, y1, thickness, color);
```

---

## Widget reference

- [Button](widgets/button.md)
- [Slider](widgets/slider.md)
- [TextInput](widgets/textinput.md)
- [Label](widgets/label.md)
- [Checkbox](widgets/checkbox.md)
- [Dropdown](widgets/dropdown.md)
- [Panel / PanelBuilder](widgets/panel.md)
- [ColorPicker](widgets/color_picker.md)
- [Container](widgets/container.md)

## Further reading

- [Layout system — Row, Column, Node, Style](layout.md)
- [Core API — Ui, Canvas, Widget trait, GuiKey](api/core.md)
