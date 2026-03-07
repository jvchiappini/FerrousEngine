# Reactive Constraints — `SizeExpr` & `Constraint`

`ferrous_gui` includes a reactive layout system that lets you describe widget
positions and sizes as *expressions* relative to the window (or parent container)
rather than as hard-coded pixel values. The engine resolves every expression
automatically once per frame — no manual coordinate recalculation on resize.

> **Imports**
> ```rust
> use ferrous_gui::{Constraint, SizeExpr};
> ```

---

## `SizeExpr` — a single axis expression

```rust
pub enum SizeExpr {
    Px(f32),            // fixed pixels
    Pct(f32),           // fraction of container (0.0 = 0 %, 1.0 = 100 %)
    FromRight(f32),     // container_w − widget_w − margin  (pin to right edge)
    FromBottom(f32),    // container_h − widget_h − margin  (pin to bottom edge)
    Add(Box<SizeExpr>, Box<SizeExpr>),  // sum of two expressions
    Center(f32),        // (container − widget) / 2 + offset
}
```

### Constructors

| Helper | Equivalent | Description |
|--------|-----------|-------------|
| `SizeExpr::px(v)` | `Px(v)` | Fixed `v` pixels |
| `SizeExpr::pct(f)` | `Pct(f)` | `f` × container size |
| `SizeExpr::from_right(m)` | `FromRight(m)` | `container_w − widget_w − m` |
| `SizeExpr::from_bottom(m)` | `FromBottom(m)` | `container_h − widget_h − m` |
| `SizeExpr::center()` | `Center(0.0)` | Centred in container |
| `SizeExpr::center_offset(d)` | `Center(d)` | Centred with pixel offset |

### Arithmetic

```rust
// 100 % of container width minus 16 px of margin
SizeExpr::pct(1.0).add(SizeExpr::px(-16.0))
```

### `resolve(container_size, widget_size) -> f32`

Evaluates the expression at runtime.  Width/height are resolved *before* x/y
so that `FromRight`/`FromBottom`/`Center` can use the widget's final size.

---

## `Constraint` — per-widget layout rules

`Constraint` bundles up to four `SizeExpr` fields. Only supplied axes are
overridden; `None` fields leave the widget's existing value untouched.

```rust
pub struct Constraint {
    pub x:      Option<SizeExpr>,
    pub y:      Option<SizeExpr>,
    pub width:  Option<SizeExpr>,
    pub height: Option<SizeExpr>,
}
```

### Fluent builder

```rust
Constraint::new()
    .x(SizeExpr::from_right(20.0))
    .y(SizeExpr::px(12.0))
    .width(SizeExpr::px(160.0))
    .height(SizeExpr::px(36.0))
```

### Shortcuts

| Shortcut | Description |
|----------|-------------|
| `Constraint::pin_right(margin_x, y, w, h)` | Pin to right edge |
| `Constraint::pin_bottom(x, margin_y, w, h)` | Pin to bottom edge |
| `Constraint::center_x(y, w, h)` | Centre horizontally |
| `Constraint::center(w, h)` | Centre in both axes |

---

## Attaching constraints to widgets

Every widget (`Button`, `Slider`, `Label`, `Checkbox`, `Dropdown`, `Container`,
`Panel`) exposes:

```rust
pub constraint: Option<Constraint>  // field
.with_constraint(c: Constraint) -> Self  // builder method
```

The constraint is stored on the widget and resolved every frame without further
user involvement.

### Examples

```rust
// Button pinned to the top-right corner
Button::new(0.0, 0.0, 120.0, 36.0)
    .with_label("Settings")
    .with_constraint(
        Constraint::new()
            .x(SizeExpr::from_right(20.0))
            .y(SizeExpr::px(12.0))
    );

// Panel that always fills the window width minus 16 px of margin
PanelBuilder::column(0.0, 0.0, 0.0)
    .with_constraint(
        Constraint::new()
            .x(SizeExpr::px(8.0))
            .y(SizeExpr::px(44.0))
            .width(SizeExpr::pct(1.0).add(SizeExpr::px(-16.0)))
    )
    .add_button("Save")
    .build();

// Slider centred horizontally, fixed height
Slider::new(0.0, 200.0, 300.0, 24.0, 0.5)
    .with_constraint(
        Constraint::new()
            .x(SizeExpr::center())
    );

// Container centred in the window
Container::new(0.0, 0.0, 400.0, 300.0)
    .with_constraint(Constraint::center(400.0, 300.0));
```

---

## Automatic resolution — `Ui::resolve_constraints`

```rust
ui.resolve_constraints(window_w, window_h);
```

Call this **once per frame, before `draw`**. It iterates every widget in the
root canvas and calls `apply_constraint(window_w, window_h)` on each one. The
engine runner does this automatically; you only need to call it explicitly if
you manage your own frame loop.

For `Container` and `Panel`, `resolve_constraints` also propagates any change
in origin to all direct child widgets, so nested layouts stay coherent.

### Typical integration

```rust
// Engine runner calls this before your draw_ui callback:
ui.resolve_constraints(window_width, window_height);

// Then your draw callback renders everything at the updated positions:
ui.draw(&mut quad_batch, &mut text_batch, Some(&font));
```

---

## `Widget::apply_constraint` and `Widget::shift`

These are methods on the `Widget` trait used internally by the constraint system.
Custom widget implementations can override them:

```rust
// Called by Ui::resolve_constraints — apply the stored constraint
fn apply_constraint(&mut self, container_w: f32, container_h: f32) { /* … */ }

// Called when a parent Container/Panel moves — shift own position by (dx, dy)
fn shift(&mut self, dx: f32, dy: f32) { /* … */ }

// Apply an *external* constraint (used by the shift helper)
fn apply_constraint_with(&mut self, c: &Constraint, cw: f32, ch: f32) { /* … */ }
```

All three have default no-op implementations in the trait; concrete widgets
override `apply_constraint` and `apply_constraint_with` to update their `rect`
(or `pos` for `Label`).

---

## Notes

- Width/height expressions are resolved **before** x/y so that `FromRight`,
  `FromBottom`, and `Center` can use the widget's final computed size.
- `Center` and `FromRight`/`FromBottom` require an accurate `widget_size`.
  For `Button`, `Slider`, `Checkbox`, `Dropdown` this is the stored `rect[2/3]`.
  For `Label` an approximate width (`chars * font_size * 0.6`) is used.
- Constraints describe position/size *relative to the window* (or the
  container's own rect when inside a `Container`). They do **not** create a
  dependency graph between sibling widgets.
- A widget with `constraint: None` is never touched by `resolve_constraints`
  and keeps its original pixel coordinates — fully backwards-compatible.
