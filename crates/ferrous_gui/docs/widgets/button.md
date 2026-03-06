# Button

`Button` is a clickable rectangular widget that tracks hover and press state.

## Struct

```rust
#[derive(Debug, Clone)]
pub struct Button {
    pub rect:    [f32; 4],   // [x, y, width, height] in window pixels
    pub hovered: bool,
    pub pressed: bool,
    pub color:   [f32; 4],   // RGBA base colour
    pub radii:   [f32; 4],   // per-corner radii [top-left, top-right, bottom-left, bottom-right]
}
```

- `pressed` — set to `true` on mouse-down inside the rect; cleared on any mouse-up.  
  Read this in `update()` and set it back to `false` to consume the click.
- `hovered` — set while the cursor is over the rect.
- `color` — base RGBA colour. Tinted slightly on hover (green), more on press (red).
- `radii` — corner radii in pixels; zero = sharp corners.

## Construction

```rust
// Minimal: position + size
let btn = Button::new(x, y, width, height);

// Uniform corner radius
let btn = Button::new(20.0, 20.0, 120.0, 40.0).with_radius(6.0);

// Per-corner radii
let btn = Button::new(20.0, 20.0, 120.0, 40.0)
    .with_radii([8.0, 8.0, 0.0, 0.0]);  // rounded top only

// Fine-grained per-corner builders
let btn = Button::new(20.0, 20.0, 120.0, 40.0)
    .round_tl(8.0).round_tr(8.0);

// Set colour directly (field is pub)
let mut btn = Button::new(20.0, 20.0, 120.0, 40.0);
btn.color = [0.2, 0.6, 1.0, 1.0];
```

## Usage pattern

```rust
struct MyApp {
    save_btn: Button,
}

impl Default for MyApp {
    fn default() -> Self {
        Self {
            save_btn: Button::new(20.0, 20.0, 120.0, 40.0).with_radius(6.0),
        }
    }
}

impl FerrousApp for MyApp {
    fn configure_ui(&mut self, ui: &mut Ui) {
        ui.add(self.save_btn.clone());
    }

    fn update(&mut self, ctx: &mut AppContext) {
        if self.save_btn.pressed {
            self.save_btn.pressed = false;   // consume
            save_file();
        }
    }

    fn draw_ui(&mut self, gui: &mut GuiBatch, _text: &mut TextBatch,
               _font: Option<&Font>, _ctx: &mut AppContext) {
        self.save_btn.draw(gui);
    }
}
```

## Drawing without `Ui`

If you do not need input routing, draw the button directly:

```rust
fn draw_ui(&mut self, gui: &mut GuiBatch, ..) {
    self.btn.draw(gui);
}
```

## Notes

- The hit test uses the full `rect`; corner radii do not affect it.
- There is no built-in label; render text on top in `draw_ui` using `TextBatch`.


# Button widget

The Button control is a basic rectangular clickable element suitable
for most immediate-mode UIs. It manages its own hover and press state
and is rendered as a coloured quad; rounded corners are supported.

## Structure

```rust
#[derive(Debug, Clone)]
pub struct Button {
    pub rect: [f32; 4], // x, y, width, height
    pub hovered: bool,
    pub pressed: bool,
    pub color: [f32; 4],
    pub radii: [f32; 4],
}
```

- **`rect`** specifies the button’s geometry in window coordinates.
- **`hovered`** is true when the mouse cursor is currently over the
  control.
- **`pressed`** indicates that a mouse button was pressed while the
  cursor was inside the rect. It is cleared on any mouse-up event.
- **`color`** is the base RGBA colour; the draw code tints it when
  hovered or pressed.
- **`radii`** contains per-corner radii in pixels (`[top-left, top-right,
  bottom-left, bottom-right]`). Zero means sharp corners.

## Construction and configuration

```rust
impl Button {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self;
    pub fn with_radius(self, r: f32) -> Self;
    pub fn with_radii(self, radii: [f32; 4]) -> Self;
    pub fn round(self, tl: f32, tr: f32, bl: f32, br: f32) -> Self;
    pub fn round_tl(self, r: f32) -> Self;
    pub fn round_tr(self, r: f32) -> Self;
    pub fn round_bl(self, r: f32) -> Self;
    pub fn round_br(self, r: f32) -> Self;
}
```

These methods are chainable and return the modified button, so you can
configure an instance in a fluent style. For example:

```rust
let btn = Button::new(10.0, 10.0, 120.0, 40.0)
    .with_radius(5.0)
    .color([0.3, 0.6, 0.9, 1.0]);
```

(The `color` field may be set directly since it is public.)

## Interaction

The widget provides the following helper methods:

- `hit(&self, mx: f64, my: f64) -> bool` – returns true if the given
  point lies within `rect`.

As part of its `Widget` trait implementation the button updates its
internal state automatically:

- `mouse_move` toggles `hovered` based on the hit test.
- `mouse_input` sets `pressed` when a press occurs inside the rect and
  clears `pressed` on any release.

## Rendering

There are two ways to draw a button:

1. **`Widget` API** – add the button to a `Ui`/`Canvas`; its
   `collect` implementation emits a `RenderCommand::Quad` with the
   appropriate colour and radii.
2. **Manual batch** – call `button.draw(&mut gui_batch)` to push the
   quad directly to a `GuiBatch`.

The visual colour is determined by the `hovered`/`pressed` flags:

- normal: `color`
- hovered: tint towards green
- pressed: tint towards red

## Example

```rust
let mut ui = ferrous_gui::Ui::new();
let b = Button::new(20.0, 20.0, 100.0, 30.0)
    .with_radius(3.0);
ui.add(b);

// later, after input processing:
if b.pressed {
    println!("button was clicked");
}
```

## Notes

- The button does not provide built-in callback mechanisms; applications
  observe the `pressed` field directly or wrap the widget in their own
  logic.
- The hit test ignores corner radii; if you require pixel-perfect
  rounded-corner detection, perform your own test using the `rect` and
  `radii` fields.
