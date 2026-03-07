# Label

`Label` is a first-class static text widget that can be registered in a `Ui`
or `Canvas` directly. It renders a single line (or word-wrapped lines up to
`max_width`) with configurable colour and font size.

## Fields

```rust
pub struct Label {
    pub x:          f32,
    pub y:          f32,
    pub text:       String,
    pub color:      [f32; 4],    // default opaque white
    pub font_size:  f32,         // default 14.0
    pub max_width:  Option<f32>, // word-wrap boundary (not yet implemented)
    pub tooltip:    Option<String>,
}
```

## Construction

```rust
// Minimal
let lbl = Label::new(20.0, 20.0, "Hello, world!");

// Styled
let lbl = Label::new(20.0, 20.0, "Health")
    .with_color([0.8, 1.0, 0.8, 1.0])
    .with_font_size(16.0)
    .with_tooltip("Current player health");

// With max_width
let lbl = Label::new(20.0, 20.0, "A long description…")
    .with_max_width(200.0);
```

## Builder API

| Method | Description |
|--------|-------------|
| `with_color([f32;4])` | Text colour (RGBA) |
| `with_font_size(f32)` | Font size in pixels (default `14.0`) |
| `with_max_width(f32)` | Horizontal wrap boundary |
| `with_tooltip(text)` | Tooltip returned via `Widget::tooltip()` |

## Runtime text update

```rust
label_handle.borrow_mut().set_text("Updated!");
// or assign directly:
label_handle.borrow_mut().text = format!("Score: {}", score);
```

## Via `PanelBuilder`

```rust
let panel = PanelBuilder::column(20.0, 20.0, 200.0)
    .add_label("Section header")
    .add_slider(0.0, 100.0, 50.0)
    .build();

// Rc handle
panel.labels[0].borrow_mut().set_text("New header");
```

## `bounding_rect` heuristic

`Widget::bounding_rect` estimates width as `text.chars().count() as f32 * font_size * 0.6`
and height as `font_size`. This is used by `PanelBuilder` for automatic layout
spacing.

## Notes

- `Label` does not handle mouse or keyboard input.
- It emits a `RenderCommand::Text` from `collect`.
- The `text` feature flag must be enabled (default) for text to render.
