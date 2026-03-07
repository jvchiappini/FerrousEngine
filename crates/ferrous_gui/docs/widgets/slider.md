# Slider

`Slider` is a horizontal drag control with a configurable value range. `value`
always stores the **real** value within `[min, max]` — no manual mapping needed.

## Fields

```rust
pub struct Slider {
    pub rect:        [f32; 4],   // [x, y, width, height]
    pub value:       f32,        // real value in [min, max]
    pub min:         f32,        // range minimum (default 0.0)
    pub max:         f32,        // range maximum (default 1.0)
    pub dragging:    bool,
    pub thumb_color: [f32; 4],
    pub track_color: [f32; 4],
    pub tooltip:     Option<String>,
    // on_change: Box<dyn Fn(f32)>  (set via .on_change(|v|{…}))
}
```

## Construction

```rust
// Default range 0.0–1.0, initial value 0.5
let s = Slider::new(20.0, 60.0, 300.0, 24.0, 0.5);

// Custom range — value is stored as the real value
let volume = Slider::new(20.0, 60.0, 200.0, 20.0, 80.0)
    .range(0.0, 100.0);          // value == 80.0

// With on_change callback
let freq = Slider::new(20.0, 90.0, 200.0, 20.0, 440.0)
    .range(20.0, 20_000.0)
    .on_change(|v| set_frequency(v));

// With tooltip
let s = Slider::new(20.0, 60.0, 200.0, 20.0, 0.5)
    .with_tooltip("Master volume");

// Customise colours (fields are pub)
let mut s = Slider::new(20.0, 60.0, 300.0, 24.0, 0.0);
s.thumb_color = [0.9, 0.6, 0.1, 1.0];
s.track_color = [0.3, 0.3, 0.3, 1.0];
```

## Builder API

| Method | Description |
|--------|-------------|
| `range(min, max)` | Set value range; clamps current value |
| `with_value(v)` | Set initial value within current range |
| `on_change(fn)` | Callback `fn(f32)` fired on every drag update |
| `with_tooltip(text)` | Tooltip returned via `Widget::tooltip()` |

## Reading the value

`slider.value` is always the real value — no scaling required:

```rust
// Range 0–100 slider: value is already in Hz-equivalent units
let volume_pct = self.panel.sliders[0].borrow().value; // e.g. 73.5
```

## Callbacks vs polling

```rust
// Polling (in update())
let v = slider_handle.borrow().value;

// Callback (fires during drag)
Slider::new(…).range(0.0, 100.0).on_change(|v| apply_volume(v));
```

## Rendering

Two quads: the full-width track and a narrower thumb. Thumb width is 10 % of
total width. The thumb position is computed from the normalised ratio
`(value - min) / (max - min)`.

```rust
// Manual draw into a GuiBatch
self.slider.draw(&mut dc.gui);
```

## Notes

- Dragging begins only when the cursor is over the thumb, not the track.
- `update_value(mx)` recalculates `value` from an X coordinate and fires
  `on_change` — called automatically during drag, exposed for manual use.
- `Slider` is not `Clone`/`Debug` (closures). Use `Rc<RefCell<Slider>>` for
  shared access — the type alias `SliderHandle` is exported from `panel`.
