# Button

`Button` is a clickable rectangular widget that tracks hover and press state.
It supports an optional centred text label, per-corner rounding, a tooltip
string, and an `on_click` callback.

> **Import** — `ferrous_gui::Button`  
> The alias `ferrous_gui::InteractiveButton` is kept for backward compatibility.

## Fields

```rust
pub struct Button {
    pub rect:             [f32; 4],      // [x, y, width, height] in window pixels
    pub hovered:          bool,
    pub pressed:          bool,
    pub color:            [f32; 4],      // RGBA base colour
    pub radii:            [f32; 4],      // per-corner radii [TL, TR, BL, BR]
    pub label:            Option<String>,
    pub label_font_size:  f32,           // default 14.0
    pub label_color:      [f32; 4],      // default opaque white
    pub tooltip:          Option<String>,
    pub constraint:       Option<Constraint>, // reactive layout (optional)
    // on_click: Box<dyn Fn()>  (not public; set via .on_click(||{…}))
}
```

- `pressed` — `true` on mouse-down inside the rect; cleared on mouse-up.  
  When using `on_click`, the callback fires automatically — no manual polling.
- `hovered` — `true` while the cursor is over the rect.
- `color` — base RGBA. Brightened on hover (×1.3), darkened on press (×0.7).
- `label` — centred inside the rect using a fixed-width character estimate.
- `tooltip` — returned by `Widget::tooltip()` for the caller to display.

## Construction

```rust
// Minimal
let btn = Button::new(x, y, w, h);

// With label and rounding
let btn = Button::new(20.0, 20.0, 120.0, 36.0)
    .with_label("Save")
    .with_radius(6.0)
    .with_tooltip("Save the current file");

// With callback — fires on mouse-release while hovered
let btn = Button::new(20.0, 20.0, 120.0, 36.0)
    .with_label("Delete")
    .with_radius(4.0)
    .on_click(|| println!("Deleted!"));

// Per-corner radii — rounded top only
let btn = Button::new(20.0, 20.0, 120.0, 36.0)
    .with_radii([8.0, 8.0, 0.0, 0.0]);

// Fine-grained corner helpers
let btn = Button::new(20.0, 20.0, 120.0, 36.0)
    .round_tl(8.0).round_tr(8.0);

// Custom label style
let btn = Button::new(20.0, 20.0, 120.0, 36.0)
    .with_label("OK")
    .with_label_font_size(16.0)
    .with_label_color([0.9, 1.0, 0.9, 1.0]);
```

## Builder API

| Method | Description |
|--------|-----------|
| `with_label(text)` | Centred text label |
| `with_label_font_size(f32)` | Override label font size (default `14.0`) |
| `with_label_color([f32;4])` | Override label colour |
| `with_tooltip(text)` | Tooltip returned via `Widget::tooltip()` |
| `on_click(fn)` | Callback fired on click (`Box<dyn Fn() + Send + Sync>`) |
| `with_radius(f32)` | Uniform corner radius |
| `with_radii([f32;4])` | Per-corner radii `[TL, TR, BL, BR]` |
| `round(tl, tr, bl, br)` | Alias for `with_radii` |
| `round_tl/tr/bl/br(f32)` | Set one corner at a time |
| `with_constraint(c)` | Attach a reactive [`Constraint`](../constraint.md) |

## Usage — polling `pressed`

When the widget lives inside a `Panel` (via `PanelBuilder`) or is wrapped in
`Rc<RefCell<Button>>`, poll `pressed` through the handle:

```rust
if self.panel.buttons[0].borrow().pressed {
    // clear pressed so it isn't re-triggered next frame
    self.panel.buttons[0].borrow_mut().pressed = false;
    save_file();
}
```

## Usage — `on_click` callback

```rust
let btn = Button::new(20.0, 20.0, 120.0, 36.0)
    .with_label("Save")
    .on_click(|| save_file());
ui.add(btn);
// No update() polling needed — callback fires automatically.
```

## Drawing

`draw(batch)` — push background quad only (no label text).  
`draw_with_text(quad_batch, text_batch, font)` — background **and** centred label.  
When used via `Ui::draw()` / `Widget::collect()`, the label is emitted as a
`RenderCommand::Text` automatically.

```rust
// Manual draw (background + label via text batch)
self.btn.draw_with_text(&mut dc.gui, &mut dc.text, Some(dc.font));
```

## Notes

- Hit-test uses the full `rect`; corner radii do not clip the hit zone.
- `Button` is not `Clone` or `Debug` (closures aren’t). Use
  `Rc<RefCell<Button>>` for shared access (what `PanelBuilder` gives you).
- `Widget::tooltip()` returns `self.tooltip.as_deref()`; the application is
  responsible for querying hovered widgets and rendering the tooltip quad.

## Reactive positioning

```rust
use ferrous_gui::{Constraint, SizeExpr};

// Always 20 px from the right edge, 12 px from the top
Button::new(0.0, 0.0, 120.0, 36.0)
    .with_label("Settings")
    .with_constraint(
        Constraint::new()
            .x(SizeExpr::from_right(20.0))
            .y(SizeExpr::px(12.0))
    );
```

See [constraint.md](../constraint.md) for the full API.
