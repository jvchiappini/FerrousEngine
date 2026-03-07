# ferrous_gui

`ferrous_gui` is the 2D widget toolkit for FerrousEngine. It provides interactive
widgets, a flex-like layout system, and helpers for integrating with the engine
frame loop and input state.

---

## Module overview

| Module | Key exports | Description |
|--------|-------------|-------------|
| `ui` | `Ui` | Top-level container; add widgets and route input events |
| `canvas` | `Canvas` | Focus-aware widget collection |
| `widget` | `Widget` | Trait every widget implements |
| `button` | `Button` | Clickable rectangle, optional rounded corners |
| `slider` | `Slider` | Horizontal drag control, normalised 0.0-1.0 |
| `textinput` | `TextInput` | Single-line editable text field |
| `color_picker` | `ColorPicker`, `PickerShape` | Colour selection widget |
| `container` | `Container` | Grouping panel with optional background |
| `layout` | `Node`, `Style`, `Row`, `Column`, `UiButton`, `Text` | Declarative layout |
| `viewport_widget` | `ViewportWidget` | Embedded 3D viewport region |
| `renderer` | `GuiBatch`, `TextBatch`, `GuiRenderer` | Low-level draw batches |

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

## Three-step workflow

### Step 1 - Add widgets once (in `configure_ui`)

```rust
fn configure_ui(&mut self, ui: &mut Ui) {
    ui.add(self.my_button.clone());
    ui.add(self.my_slider.clone());
    ui.add(self.my_input.clone());
}
```

`Ui` takes ownership of the widgets and routes mouse/keyboard input to them
automatically every frame. Keep your own copy in your struct to read state.

### Step 2 - Draw every frame (in `draw_ui`)

```rust
fn draw_ui(&mut self, dc: &mut DrawContext<'_, '_>) {
    self.my_button.draw(dc.gui);
    self.my_slider.draw(dc.gui);
    dc.text.draw_text(dc.font, "Hello world", [20.0, 20.0], 18.0, [1.0; 4]);
}
```

### Step 3 - Read state in `update`

```rust
fn update(&mut self, ctx: &mut AppContext) {
    if self.my_button.pressed {
        self.my_button.pressed = false;   // consume
        println!("Clicked!");
    }
    println!("Slider: {:.2}", self.my_slider.value);
    println!("Input:  {}", self.my_input.text);
}
```

---

## Widget reference

- [Button](widgets/button.md)
- [Slider](widgets/slider.md)
- [TextInput](widgets/textinput.md)
- [ColorPicker](widgets/color_picker.md)
- [Container](widgets/container.md)

## Further reading

- [Layout system - Row, Column, Node, Style](layout.md)
- [Core API - Ui, Canvas, Widget trait](api/core.md)
