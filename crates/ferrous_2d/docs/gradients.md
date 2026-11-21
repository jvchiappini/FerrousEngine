# Gradients in Pure2D

`ferrous_2d` supports per-instance linear and radial colour ramps for shapes,
sprites (through their instance colour path) and text.  The shader evaluates the
ramp per fragment in linear light and dithers it to hide banding.

## Authoring a frame

1. Build a [`GradientTable2d`] once per frame.  Push each gradient; the returned
   `base` is a slot index.
2. Upload the table with `Renderer2d::set_gradient_table` (or
   `Renderer::set_2d_gradient_table` on the façade).
3. Point an instance at a gradient with `ShapeInstance::with_gradient(base)` or
   `TextInstance::with_gradient(base)` / `Renderer::draw_text_2d_with_gradient`.

```rust,ignore
use ferrous_2d::render::{Gradient2d, GradientTable2d};
use glam::Vec2;

let mut table = GradientTable2d::new();
let base = table.push(&Gradient2d::linear(
    Vec2::new(0.0, 0.0),
    Vec2::new(320.0, 0.0),
    &[(0.0, [1.0, 0.0, 0.0, 1.0]), (1.0, [0.0, 0.0, 1.0, 1.0])],
)).expect("fits");

renderer.set_2d_gradient_table(table.slots());
renderer.draw_2d_shape(ShapeInstance::rect(center, size, [1.0; 4]).with_gradient(base));
```

`0` is reserved for "flat colour", so the instance stores `base + 1`.

## Geometry

- **Linear**: `t` is the projection of the fragment position onto the
  `start` → `end` axis, normalised to `0..=1`.
- **Radial**: `t = distance(center) / radius`.

Coordinates are **world space**, so a gradient stays glued to the scene while
the instance moves.  Two stops at the same offset produce a hard edge.

## Stops

Each stop is `(offset, [r, g, b, a])` in **straight linear RGBA**.  The table
builder clamps offsets to `0..=1` and sorts them (stably).  A gradient needs two
to `MAX_GRADIENT_STOPS` (16) stops.

Dithering happens in encoded space, a quarter of an 8-bit code, and fades to
zero exactly at a stop so authored colours survive untouched.

## Capacity

The camera uniform carries `GRADIENT_TABLE_SLOTS` (512) `vec4` slots.  A
two-stop gradient uses six, so a table holds at least 85 minimal gradients.
`push` returns `None` when the next gradient does not fit; such an instance
keeps its solid colour.

## Blend mode compatibility

Gradients compose with every `BlendMode2d` and with the layer composite modes;
the fragment output is premultiplied exactly like a solid colour.

[`GradientTable2d`]: https://docs.rs/ferrous_2d/latest/ferrous_2d/render/struct.GradientTable2d.html
