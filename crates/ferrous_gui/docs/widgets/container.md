# Container

`Container` is a grouping widget that holds other widgets. It has an optional
background colour, optional scissor clipping, and forwards all input events to
its children. Children manage their own positions — no automatic layout is
applied (use [`PanelBuilder`](panel.md) for automatic layout).

## Fields

```rust
pub struct Container {
    pub rect:       [f32; 4],         // [x, y, w, h]
    pub bg_color:   Option<[f32; 4]>, // optional filled background
    pub clip:       bool,             // scissor children to rect (default false)
    pub constraint: Option<Constraint>, // reactive layout (optional)
    // canvas: Canvas  (internal)
}
```

- Pass `0.0` for `w` or `h` to auto-size that dimension to enclose children.
- When `clip` is `true` the container emits `RenderCommand::PushClip` /
  `PopClip` around its children so the renderer can scissor-test them.

## Construction

```rust
// Fixed size, no background
let c = Container::new(x, y, 300.0, 200.0);

// Auto height, dark background
let c = Container::new(x, y, 300.0, 0.0)
    .with_background([0.08, 0.08, 0.08, 0.92]);

// With scissor clip (e.g. scrollable panel, popup)
let c = Container::new(x, y, 300.0, 200.0)
    .with_background([0.1, 0.1, 0.1, 0.9])
    .with_clip();
```

## Builder API

| Method | Description |
|--------|-----------|
| `with_background(color)` | Solid RGBA background drawn before children |
| `with_clip()` | Enable scissor clipping — emits `PushClip`/`PopClip` |
| `with_constraint(c)` | Attach a reactive [`Constraint`](../constraint.md); position delta is propagated to children |
| `add(widget)` | Add any `Widget + 'static` child |

## Usage pattern

```rust
fn configure_ui(&mut self, ui: &mut Ui) {
    let mut panel = Container::new(40.0, 40.0, 140.0, 0.0)
        .with_background([0.1, 0.1, 0.1, 0.9])
        .with_clip();   // children won't bleed outside the rect

    panel.add(
        Button::new(10.0, 10.0, 120.0, 28.0)
            .with_label("Apply")
            .with_radius(4.0)
    );
    panel.add(Slider::new(10.0, 46.0, 120.0, 20.0).range(0.0, 100.0));
    ui.add(panel);
}
```

## Clip / scissor behaviour

When `clip = true`, `Widget::collect` wraps child commands like this:

```
PushClip { rect: container.rect }
  … child RenderCommands …
PopClip
```

The `GuiBatch` / `TextBatch` layer silently ignores these commands; actual
scissoring requires a renderer pass that interprets `PushClip`/`PopClip` and
sets the GPU scissor rect accordingly.

## Notes

- Mouse events are forwarded to children only when the cursor is inside `rect`.
- Keyboard input is always forwarded to the focused child.
- Background quads are drawn before children, so children render on top.
- `Container` itself implements `Widget` and can be nested inside another `Container`.

## Reactive positioning

When a `Constraint` changes the container’s origin, the position delta is
automatically propagated to all direct child widgets via `Widget::shift`:

```rust
use ferrous_gui::{Constraint, SizeExpr};

// Container always centred in the window
let mut popup = Container::new(0.0, 0.0, 360.0, 240.0)
    .with_background([0.12, 0.12, 0.12, 0.95])
    .with_constraint(Constraint::center(360.0, 240.0));
popup.add(Button::new(120.0, 190.0, 120.0, 32.0).with_label("OK"));
```

See [constraint.md](../constraint.md).
