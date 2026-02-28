<!--
Reference material for the `ColorPicker` widget.
-->

# ColorPicker widget

`ColorPicker` is a lightweight, highly‑configurable control that allows the
user to select a colour by interacting with a rendered shape.  The
built‑in implementation behaves as a simple hue/saturation wheel, but the
widget is intentionally flexible so that callers can substitute completely
different shapes or colour‑mapping algorithms.

## Data structure

```rust
#[derive(Clone)]
pub struct ColorPicker {
    pub rect: [f32; 4],
    pub colour: [f32; 4],
    pub hovered: bool,
    pub pressed: bool,
    pub shape: PickerShape,
    pub on_pick: Option<Box<dyn Fn(&mut ColorPicker, f32, f32)>>,
}
```

- **`rect`** – x, y, width, height of the control in window coordinates.
- **`colour`** – currently selected colour (RGBA components in the range
  0.0–1.0).
- **`hovered`**, **`pressed`** – internal state used for interaction; the
  application may query or reset these values.
- **`shape`** – determines how the widget is drawn and how hit testing is
  performed.  See [`PickerShape`](#pickershape) below.
- **`on_pick`** – optional callback invoked whenever the user picks a colour
  (either by clicking or dragging).  Coordinates passed to the callback are
  normalised relative to the widget's rect (0.0–1.0).

## PickerShape

```rust
#[derive(Clone)]
pub enum PickerShape {
    Circle,
    Custom(std::sync::Arc<dyn Fn(&ColorPicker, &mut Vec<RenderCommand>)>),
}
```
`Custom` allows arbitrary drawing logic.  The supplied `Arc`‑wrapped closure
  receives a reference to the picker and a mutable command list; it may push
  one or more `RenderCommand` instances describing the appearance.  When
  using `Custom` you are responsible for whatever hit‑testing semantics make
  sense (the default `hit` falls back to a simple bounding box).

- `Circle` renders a circular swatch inscribed in the bounding rect; hit
  testing respects the circular boundary.
- `Custom` allows arbitrary drawing logic.  The supplied closure receives a
  reference to the picker and a mutable command list; it may push one or more
  `RenderCommand` instances describing the appearance.  When using `Custom`
  you are responsible for whatever hit‑testing semantics make sense (the
  default `hit` falls back to a simple bounding box).

## API

```rust
impl ColorPicker {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self;
    pub fn with_colour(mut self, c: [f32; 4]) -> Self;
    pub fn with_shape(mut self, shape: PickerShape) -> Self;
    pub fn on_pick<F>(mut self, f: F) -> Self
        where F: Fn(&mut ColorPicker, f32, f32) + 'static;
    pub fn draw(&self, batch: &mut crate::renderer::GuiBatch);
}
```

- `new` creates a picker with default white colour and circular shape.
- `with_colour` sets the initial colour.
- `with_shape` changes the rendering/hit test shape.
- `on_pick` registers a custom colour‑mapping callback.
- `draw` is a convenience method for pushing the appearance into a
  `GuiBatch` without involving the widget system.

## Rendering behaviour

As a widget the picker implements `Widget::collect`, producing either a
single rounded quad for the circle variant or whatever commands the custom
closure emits.  The `draw` helper can be used in isolation in the same way
as other controls.

By default the picker behaves like a hue/saturation wheel: clicking or
dragging inside the circle updates the colour based on the polar coordinates
of the cursor relative to the centre.  Applications can override this logic
by providing an `on_pick` callback.

### Example default draw

```rust
let mut batch = GuiBatch::new();
color_picker.draw(&mut batch);
```

## Example integration

```rust
let mut ui = ferrous_gui::Ui::new();
let mut cp = ColorPicker::new(50.0, 50.0, 100.0, 100.0)
    .with_colour([1.0,0.0,0.0,1.0]);
ui.add(cp.clone());

// in event loop, `cp.colour` will be updated whenever the user clicks/drag
```

To apply a different selection algorithm, install a custom callback:

```rust
cp = cp.on_pick(|picker, nx, ny| {
    // treat top‑left corner as black, bottom‑right as white, etc.
    picker.colour = [nx, ny, 0.0, 1.0];
});
```

## Notes

- The default circle implementation simply uses a rounded quad with corner
  radii equal to half the widget's smaller dimension; this is not a true
  anti‑aliased circle but is inexpensive and works well for most purposes.
- `Custom` shape callbacks are allowed to push multiple quads or even text
  commands; they are free to ignore `picker.colour` or to render it in any
  fashion.
