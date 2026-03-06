# Container

`Container` is a grouping widget that holds other widgets. It has an optional
background colour and forwards all input events to its children. Children
manage their own positions — no automatic layout is applied.

## Struct

```rust
pub struct Container {
    pub rect:     [f32; 4],            // [x, y, w, h]
    pub bg_color: Option<[f32; 4]>,    // optional filled background
    canvas:       Canvas,              // owns child widgets
}
```

- Pass `0.0` for `w` or `h` to auto-size that dimension to enclose children.
- `canvas` is internal; use the methods below to interact with children.

## Construction

```rust
// Fixed size, no background
let container = Container::new(x, y, 300.0, 200.0);

// Auto height, dark background
let container = Container::new(x, y, 300.0, 0.0)
    .with_background([0.08, 0.08, 0.08, 0.92]);
```

## Methods

```rust
impl Container {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self;
    pub fn with_background(self, color: [f32; 4]) -> Self;
    pub fn add(&mut self, widget: impl Widget + 'static);
    pub fn mouse_move(&mut self, mx: f64, my: f64);
    pub fn mouse_input(&mut self, mx: f64, my: f64, pressed: bool);
    pub fn keyboard_input(&mut self, text: Option<&str>,
                          key: Option<KeyCode>, pressed: bool);
}
```

- `add` takes ownership of the widget; call it in `configure_ui`.
- Mouse events are only forwarded when the cursor is inside `rect`.
- Keyboard input is always forwarded to the focused child.

## Usage pattern

```rust
impl FerrousApp for MyApp {
    fn configure_ui(&mut self, ui: &mut Ui) {
        let mut panel = Container::new(40.0, 40.0, 140.0, 0.0)
            .with_background([0.1, 0.1, 0.1, 0.9]);
        panel.add(Button::new(10.0, 10.0, 120.0, 28.0).with_label("Apply"));
        panel.add(Slider::new(10.0, 50.0, 120.0, 20.0));
        ui.add(panel);
    }

    fn draw_ui(&mut self, gui: &mut GuiBatch, text: &mut TextBatch,
               font: Option<&Font>, _ctx: &mut AppContext) {
        self.panel.draw(gui, text, font);
    }
}
```

## Notes

- Children are **not** clipped to `rect`; drawing outside the bounding box
  will still appear on screen.
- Background quads are drawn before children, so children render on top.
- `Container` itself implements `Widget` and can be nested inside another
  `Container`.
