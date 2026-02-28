````markdown
<!--
Documentation for the `Container` widget.
-->

# Container widget

`Container` is a simple grouping widget that holds a collection of other
`Widget` objects.  It does not impose any automatic layout; child widgets
are expected to manage their own positions.  The primary purpose is to treat
multiple widgets as a single unit for input handling and optional
background rendering.

Because `Container` itself implements the `Widget` trait it can be added
directly to a `Ui` or another container.  Events and draw commands are
forwarded to children in the order they were inserted.

## Features

* **Background (optional)** – supply an RGBA colour to have the container
  draw a filled quad behind its contents.  Leaving the background `None`
  makes the container invisible.
* **Hit testing** – the container responds to hit tests based on its own
  rectangle.  Mouse events are only propagated to children when the cursor
  lies inside that region; keyboard input is always sent to the currently
  focused child.
* **Focusable children** – focus handling (for keyboard events) is managed by
  the internal `Canvas`, identical to how `Ui` works.

## Structure

```rust
pub struct Container {
    pub rect: [f32; 4],            // x, y, width, height in window coords
    pub bg_color: Option<[f32; 4]>,// optional background colour
    canvas: Canvas,                // manages child widgets
}
```

- **`rect`** – bounding box of the container.
- **`bg_color`** – if `Some`, the colour used to draw a filled quad before
  children are rendered.
- **`canvas`** – internal helper that stores and routes events to the child
  widgets.  Its API is largely re‑exposed by the container.

## API

```rust
impl Container {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self;
    pub fn with_background(mut self, color: [f32;4]) -> Self;
    pub fn add(&mut self, widget: impl Widget + 'static);
    pub fn mouse_move(&mut self, mx: f64, my: f64);
    pub fn mouse_input(&mut self, mx: f64, my: f64, pressed: bool);
    pub fn keyboard_input(&mut self, text: Option<&str>, key: Option<KeyCode>, pressed: bool);
}
```

* `new` constructs an empty container with no background.
* `with_background` attaches a solid colour and returns `self` for chaining.
* `add` inserts a child widget (by value); the container takes ownership.
* The event helpers mirror those on `Canvas` but restrict mouse events to the
  container's rect.

## Usage example

```rust
let mut ui = ferrous_gui::Ui::new();
let mut group = ferrous_gui::Container::new(10.0, 10.0, 200.0, 150.0)
    .with_background([0.1, 0.1, 0.1, 0.8]);

let btn = ferrous_gui::Button::new(20.0, 20.0, 80.0, 30.0);
group.add(btn);

let slider = ferrous_gui::Slider::new(20.0, 60.0, 160.0, 20.0);
group.add(slider);

ui.add(group);
```

The rectangle supplied to `Container::new` is used for hit tests; the
children are free to use any coordinates (typically they will lie inside the
box, but this is not strictly required).

## Notes

* Containers do **not** clip their children; drawing commands from widgets
  outside the `rect` will still appear.  If clipping is required, the
  application must handle it separately.
* Background quads are drawn before child commands, so children will appear
  on top.

````