# Checkbox

`Checkbox` is a boolean toggle with an optional text label. It renders an
outlined box; when checked, a smaller filled square appears inside. The label
text is drawn to the right of the box.

## Fields

```rust
pub struct Checkbox {
    pub x:       f32,
    pub y:       f32,
    pub label:   String,
    pub checked: bool,
    pub size:    f32,         // outer box side length (default 16.0)
    pub radius:  f32,         // corner radius of the box (default 3.0)
    pub color:   [f32; 4],    // fill/check colour (default opaque white)
    pub tooltip: Option<String>,
    pub hovered: bool,
    // on_change: Box<dyn Fn(bool)>  (set via .on_change(|v|{…}))
}
```

## Construction

```rust
// Minimal
let cb = Checkbox::new(20.0, 20.0, "Enable shadows");

// Pre-checked
let cb = Checkbox::new(20.0, 20.0, "VSync")
    .checked(true);

// With callback
let cb = Checkbox::new(20.0, 20.0, "Show FPS")
    .checked(false)
    .on_change(|v| set_fps_overlay(v));

// Custom size / corner radius
let cb = Checkbox::new(20.0, 20.0, "Large option")
    .with_size(24.0)
    .with_radius(5.0);

// With tooltip
let cb = Checkbox::new(20.0, 20.0, "Experimental")
    .with_tooltip("Enable experimental features (may be unstable)");
```

## Builder API

| Method | Description |
|--------|-------------|
| `checked(bool)` | Initial checked state |
| `with_size(f32)` | Override outer box size (default `16.0`) |
| `with_radius(f32)` | Box corner radius (default `3.0`) |
| `with_tooltip(text)` | Tooltip returned via `Widget::tooltip()` |
| `on_change(fn)` | Callback `fn(bool)` fired on toggle |

## Reading state

```rust
// Via Panel handle
let is_checked = self.panel.checkboxes[0].borrow().checked;

// Via polling in update()
if my_checkbox_handle.borrow().checked { /* … */ }
```

## Programmatic toggle

```rust
// Toggle without triggering the callback
checkbox_handle.borrow_mut().checked = !checkbox_handle.borrow().checked;

// Toggle and fire on_change
checkbox_handle.borrow_mut().toggle();
```

## Hit test

The hit zone covers both the box **and** the label text (width estimated as
`label.chars().count() as f32 * font_size * 0.6`). Clicking anywhere in this
region toggles the checkbox and fires `on_change`.

## Rendering

Three draw commands are emitted:

1. Outer box — `RenderCommand::Quad` with `radius`.
2. Inset filled square (only when `checked = true`) — slightly smaller quad.
3. Label text — `RenderCommand::Text` to the right of the box.

## Notes

- `Checkbox` is not `Clone`/`Debug`. Use `Rc<RefCell<Checkbox>>` for shared
  access — `CheckboxHandle` is exported from `panel`.
