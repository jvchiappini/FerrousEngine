# ferrous_2d — 2D Rendering Subsystem

`ferrous_2d` holds the data types and GPU pipelines for FerrousEngine's 2D
render path: batched untextured SDF shapes (`ShapeInstance`), batched sprites
(`SpriteInstance`), and the 2D ECS components (`Transform2d`, `Sprite`,
`Shape2d`, `Camera2d`, `Path2d`).

Most applications do **not** use this crate directly. They draw through the
user-facing facade on `RenderContext`:

```rust,ignore
ctx.render.draw_2d_shape(ShapeInstance::rect(
    Vec2::ZERO,
    Vec2::new(64.0, 64.0),
    [1.0, 0.0, 0.0, 1.0],
));
```

## Immediate mode vs ECS mode

There are two ways to feed the 2D pipeline, and only one of them is wired up
today:

| Mode | Entry point | Status |
|------|-------------|--------|
| Immediate | `Renderer::draw_2d_shape` / `draw_sprite_2d` / `draw_2d_path`, called from `draw_ui` or `update` | Live — used by Koncreta |
| ECS | `ferrous_2d::systems::{prepare_shapes_system, prepare_sprites_system, prepare_paths_system}` | **Deprecated / dead** — no scheduler calls these; will be removed |

If you spawn `Shape2d` / `Sprite` / `Path2d` components on a `World`, nothing
renders: the `ferrous_2d` systems are not part of the engine's
`StagedScheduler`. Draw through the renderer façade instead.

The immediate batchers are **append-only across frames**. Call
`Renderer::clear_2d()` at the start of every frame (or the shapes accumulate
and "ghost"). `RenderContext` currently exposes neither `clear_2d` nor
`enable_pure_2d`; reach them through `RenderContext::renderer_mut()`.

## Shapes

`ShapeInstance` is raw GPU instance data (four transform columns, a colour, and
a `params` vector `[border_thickness, corner_radius, smoothing, is_filled]`).
The ergonomic constructors are the intended API:

| Constructor | Shape |
|-------------|-------|
| `circle` / `circle_z` | filled disc |
| `rect` / `rect_z` | filled axis-aligned rectangle |
| `rounded_rect` | filled rounded rectangle |
| `rect_outline` | rectangle border only |
| `rect_rotated` | filled rectangle rotated by an angle |
| `line` / `line_with_cap` | stroked segment (`cap_style`: `0` Flat, `1` Round, `2` Square) |

The underlying shader implements a single rounded-rectangle SDF, so `circle` is
just `corner_radius = radius`. **Arbitrary polygons are not supported by the
shape shader** — see the audit report for the CAD polygon-fill gap.

## Paths

`Path2d` + `PathBuilder` describe polylines with cap/join styles. The immediate
entry point `Renderer::draw_2d_path` currently ignores `cap_style`,
`join_style`, `is_filled` and `fill_color`, and always draws round-capped
segments. Use `ShapeInstance::line_with_cap` for precise CAD joins until this
is unified.

## Sprites

`Renderer::register_sprite_texture` (nearest) / `register_sprite_texture_linear`
return a `u32` id consumed by `draw_sprite_2d`. One texture means one bind group
and one draw call; there is no atlas packer yet, and `free_sprite_texture` does
not release the batcher's per-texture bucket.
