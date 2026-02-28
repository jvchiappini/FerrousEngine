<!--
Reference material for the `ColorPicker` widget.
-->

# ColorPicker widget

`ColorPicker` is a lightweight, highly‑configurable control that allows the
user to select a colour by interacting with a rendered shape.  By default
it behaves like a simple hue/saturation wheel, but the widget is intentionally
flexible so that callers can substitute completely different shapes or
colour‑mapping algorithms (even supplying their own hit‑testing).

## Data structure

```rust
#[derive(Clone)]
pub struct ColorPicker {
  pub rect: [f32; 4],            // x, y, width, height in window coords
  pub colour: [f32; 4],          // selected colour (RGBA 0.0..1.0)
  pub hovered: bool,             // true when pointer is over the widget
  pub pressed: bool,             // true while the primary button is held
  pub shape: PickerShape,        // determines rendering & hit testing
  pub on_pick: Option<Arc<dyn Fn(&mut ColorPicker, f32, f32)>>,
  pub pick_pos: Option<[f32; 2]>, // last normalized pick coordinates (0..1)
}
```

* **`rect`** – x, y, width, height of the control in window coordinates.
* **`colour`** – currently selected colour (RGBA components in the range
  0.0–1.0).
* **`hovered`**, **`pressed`** – internal state flags used during pointer
  interaction; the application may inspect or clear them if desired.
* **`shape`** – determines how the widget is drawn and how hit testing is
  performed.  See [`PickerShape`](#pickershape) below for details.
* **`on_pick`** – optional callback invoked whenever the user clicks or drags
  inside the widget.  The two `f32` parameters are normalized coordinates
  within the rect (0.0..1.0).
* **`pick_pos`** – if a pick interaction has occurred, this holds the
  normalized position of the last pick.  It is used to position the
  selection indicator when the colour alone is ambiguous (e.g. hue wraps
  around at 0/1).

## PickerShape

```rust
#[derive(Clone)]
pub enum PickerShape {
    Circle,
    Rect,
    Triangle,
    Custom(std::sync::Arc<dyn Fn(&ColorPicker, &mut Vec<RenderCommand>)>),
}
```
`Custom` allows arbitrary drawing logic.  The supplied `Arc`‑wrapped closure
receives a reference to the picker and a mutable command list; it may push
one or more `RenderCommand` instances describing the appearance.  When using
`Custom` you are responsible for any hit‑testing semantics (the default
`hit` implementation will simply test the bounding box).

* `Circle` renders a circular swatch inscribed in the bounding rect; hit
  testing respects the circular boundary.
* `Rect` produces a linear hue/saturation rectangle (hue left→right,
  saturation top→bottom).  The gradient is generated in the shader and
  selection coordinates are mapped directly to HSV space.
* `Triangle` restricts interaction to the upper‑left right triangle of the
  bounding rect (normalized coords satisfying `nx + ny <= 1.0`).  Hue runs
  along the hypotenuse fan originating at the bottom‑left corner, and
  saturation decreases from the base toward the origin.  Points outside the
  triangle are treated as misses by the default hit test.

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

* `new` creates a picker with default white colour, circular shape, and no
  pick history.
* `with_colour` sets the initial colour (clearing any stored `pick_pos`).
* `with_shape` changes the rendering/hit‑test shape.
* `on_pick` registers a custom colour‑mapping callback.
* `draw` is a convenience helper that pushes the current appearance into a
  `GuiBatch` without requiring the full widget machinery.

## Rendering behaviour

As a widget the picker implements `Widget::collect`, producing either a
single rounded quad for the circle variant or whatever commands the custom
closure emits.  The `draw` helper can be used in isolation in the same way
as other controls.

By default the picker behaves like a hue/saturation wheel: clicking or
dragging anywhere inside the active region updates `colour` using the
polar mapping (or the corresponding linear/triangular map for the other
shapes).  Rather than creating a mesh of coloured squares (which looked
"pixelated"), the renderer uses a single quad and a fragment shader that
generates the gradient on‑the‑fly.  The widget is responsible only for
pushing the quad and for drawing a small white indicator at the current
`pick_pos`.

Applications can override the selection algorithm by supplying an
`on_pick` callback; the supplied normalized coordinates will follow the
shape's hit region but may be interpreted however the application chooses.

### Example default draw and selection marker

```rust
let mut batch = GuiBatch::new();
color_picker.draw(&mut batch); // renders a wheel with current hue/sat indicator
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
