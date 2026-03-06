# Slider

`Slider` is a horizontal drag control. The user clicks and drags the thumb to
select a normalised value in `[0.0, 1.0]`.

## Struct

```rust
#[derive(Debug, Clone)]
pub struct Slider {
    pub rect:        [f32; 4],   // [x, y, width, height]
    pub value:       f32,        // normalised position, clamped to 0.0-1.0
    pub dragging:    bool,
    pub thumb_color: [f32; 4],
    pub track_color: [f32; 4],
}
```

## Construction

```rust
// Slider::new(x, y, width, height, initial_value)
let slider = Slider::new(20.0, 60.0, 300.0, 24.0, 0.5);

// Change colours directly (fields are pub)
let mut s = Slider::new(20.0, 60.0, 300.0, 24.0, 0.0);
s.thumb_color = [0.9, 0.6, 0.1, 1.0];
s.track_color = [0.3, 0.3, 0.3, 1.0];
```

## Usage pattern

```rust
struct MyApp {
    volume: Slider,
}

impl Default for MyApp {
    fn default() -> Self {
        Self {
            volume: Slider::new(20.0, 60.0, 200.0, 20.0, 0.8),
        }
    }
}

impl FerrousApp for MyApp {
    fn configure_ui(&mut self, ui: &mut Ui) {
        ui.add(self.volume.clone());
    }

    fn update(&mut self, _ctx: &mut AppContext) {
        // self.volume.value is already updated by the Ui input routing
        set_audio_volume(self.volume.value);
    }

    fn draw_ui(&mut self, gui: &mut GuiBatch, ..) {
        self.volume.draw(gui);
    }
}
```

## Mapping to a real range

`value` is always `0.0`-`1.0`. Scale it in code:

```rust
let frequency_hz = 200.0 + self.freq_slider.value * 1800.0; // 200-2000 Hz
let brightness    = (self.bright_slider.value * 255.0) as u8;
```

## Rendering

Two quads are drawn: the full-width track and a narrower thumb. The thumb's
left edge is positioned at `value * (track_width - thumb_width)`. Thumb width
is fixed at 10 % of total width.

## Notes

- Dragging begins only when the cursor is over the thumb, not the track.
- The full track rect is used for focus/hit purposes.
- `update_value(mx)` recalculates `value` from an X coordinate — called
  automatically during drag, exposed for manual use.


# Slider widget

The Slider control presents a horizontal track with a movable thumb,
allowing the user to select a value within the normalized range
`[0.0, 1.0]`. It is suitable for settings panels, audio controls, or
anywhere a continuous parameter is needed.

## Data structure

```rust
#[derive(Debug, Clone)]
pub struct Slider {
    pub rect: [f32; 4],
    pub value: f32,
    pub dragging: bool,
    pub thumb_color: [f32; 4],
    pub track_color: [f32; 4],
}
```

- **`rect`** – x, y, width, height of the control.
- **`value`** – normalized position of the thumb; automatically clamped
  to `[0,1]` whenever it is modified.
- **`dragging`** – true while the thumb is being manipulated.
- **`thumb_color`**, **`track_color`** – colour of the thumb and the
  underlying track, respectively.

## API

```rust
impl Slider {
    pub fn new(x: f32, y: f32, w: f32, h: f32, value: f32) -> Self;
    pub fn thumb_hit(&self, mx: f64, my: f64) -> bool;
    pub fn update_value(&mut self, mx: f64);
    pub fn draw(&self, batch: &mut crate::renderer::GuiBatch);
}
```

- `thumb_hit` performs a hit test against the thumb itself (not the
  entire track). Useful for beginning a drag.
- `update_value` recalculates `self.value` based on an X coordinate; it
  is typically called while `dragging` is true.

As with other widgets, `Slider` implements `Widget` so that it can be
added to a `Ui`/`Canvas`. Its `mouse_move` and `mouse_input` methods
manage `dragging` and update `value` accordingly.

## Rendering behaviour

Drawing is handled by the `draw` helper or via `collect` when used as a
widget. The control renders two quads: a full-width track with
`track_color`, and a narrower thumb whose left edge is positioned based
on `value`.

Example of manual rendering:

```rust
let mut batch = GuiBatch::new();
slider.draw(&mut batch);
```

## Example integration

```rust
let mut ui = ferrous_gui::Ui::new();
let mut slider = Slider::new(50.0, 100.0, 300.0, 20.0, 0.25);
ui.add(slider.clone());

// in event loop:
slider.value = ...; // read or modify
```

To respond to changes, inspect `slider.value` after processing input.

## Notes

- The thumb width is hardcoded as 10% of the total width; this may be
  adjusted by modifying the source if a different proportion is desired.
- The widget treats the entire control as hittable for focus purposes,
  but dragging only begins when the mouse is over the thumb.
