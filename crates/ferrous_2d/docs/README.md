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
| Immediate | `Renderer::draw_2d_shape` / `draw_sprite_2d` / `draw_2d_path`, called from `draw_ui` or `update` | Live |
| ECS | `ferrous_2d::systems::{prepare_shapes_system, prepare_sprites_system, prepare_paths_system}` | **Deprecated / dead** — no scheduler calls these; will be removed |

If you spawn `Shape2d` / `Sprite` / `Path2d` components on a `World`, nothing
renders: the `ferrous_2d` systems are not part of the engine's
`StagedScheduler`. Draw through the renderer façade instead.

The immediate batchers are **append-only across frames**. Call
`Renderer::clear_2d()` at the start of every frame (or the shapes accumulate
and "ghost"). `RenderContext` currently exposes neither `clear_2d` nor
`enable_pure_2d`; reach them through `RenderContext::renderer_mut()`.

## Text

MSDF text drawing, styled layout (tracking, line height, `\n`), block
measurement, per-glyph layouts and GPOS pair kerning are documented in
[text.md](text.md).

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

## Compositing: premultiplied alpha

Pure2D renders in **linear light** with **premultiplied alpha** end to end:

- `ShapeInstance::color`, `SpriteInstance::color` and `TextInstance::color` stay
  **straight** linear RGBA in the public API.
- Every Pure2D fragment shader premultiplies on output (`vec4(rgb * a, a)`).
- Every Pure2D pipeline uses `wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING`
  (or the per-mode state below). Over an opaque destination this is
  mathematically identical to straight alpha; over a transparent target it is
  the correct compositing result.
- Sprite textures are uploaded as straight-alpha `Rgba8UnormSrgb` by default.
  Bilinear filtering of straight texels produces dark fringes at translucent
  edges; `Renderer::register_sprite_texture_premultiplied` accepts data whose
  RGB is already multiplied by alpha (in linear light, then sRGB-encoded for
  storage) and the shader skips the second premultiply.

## Blend modes per draw

`BlendMode2d` is a per-draw property. In premultiplied linear light
(`Cs`/`As` source, `Cd`/`Ad` destination):

| mode | colour | alpha | wgpu colour factors |
|---|---|---|---|
| `Normal` | `Cs + Cd*(1-As)` | `As + Ad*(1-As)` | `One`, `OneMinusSrcAlpha` |
| `Add` | `Cs + Cd` | `As + Ad*(1-As)` | `One`, `One` |
| `Screen` | `Cs + Cd*(1-Cs)` | `As + Ad*(1-As)` | `One`, `OneMinusSrc` |
| `Multiply` | `Cs*Cd + Cd*(1-As)` | `As + Ad*(1-As)` | `Dst`, `OneMinusSrcAlpha` |

`Multiply` is exact over an opaque backdrop; over a transparent one it omits
the `Cs*(1-Ad)` term. Backdrop-aware modes (overlay, darken, lighten) are
available on layers through `CompositeMode2d`; see [`blend.md`](blend.md).

The API is a state: `Renderer::set_blend_mode_2d(mode)` applies to every
subsequent 2D draw until changed, and `Renderer::clear_2d` (called at the start
of every frame) resets it to `Normal`. A mode change between two draws of the
same kind splits their coalesced run into a second draw call but never reorders
them: painter order is `z`, then shape/sprite/text, then submission/blend.

## Offscreen layers

```rust
renderer.push_layer_2d(Layer2d::new(0.5, CompositeMode2d::Normal, 2.0))?;
// draws go into the layer, z-sorted among themselves
renderer.pop_layer_2d()?;
```

- The layer content is rendered into a transparent target-sized texture and
  composited into its parent as one command at `depth`, with group `opacity`
  (all four premultiplied channels scaled) and `blend`.
- Group opacity differs from per-child opacity: two overlapping opaque children
  inside a 50 % layer show no internal overlap.
- Nesting is supported up to `MAX_LAYER_DEPTH_2D` (8). Deeper pushes return
  `LayerError::TooDeep`. An unbalanced push at frame end is reported once as
  `LayerError::Unbalanced`, closed automatically and logged; the frame still
  renders.
- Layer textures prefer `Rgba16Float` (linear, premultiplied, more precision
  than 8-bit sRGB). If the adapter cannot blend that format, the target format
  is used instead. Textures are pooled per target size and reused across
  frames, so a steady-state 3-layer frame creates no textures; resizing the
  target invalidates the pool.
- Encoding order is post-order: inner layers first, each in its own render
  pass, then the parent pass. A frame without layers keeps the original single
  pass and creates no textures.
- A layer may carry an effect chain (`push_layer_2d_with_effects`): separable
  Gaussian blur, drop shadow and glow, applied to the finished texture before
  it composites. See [`effects.md`](effects.md); the kernel is a pure function
  of the radius and the chain is deterministic.
- A layer may be masked (push_layer_2d_masked plus egin_mask_2d /
  end_mask_2d): alpha or luma coverage from a second offscreen layer
  multiplies the finished, effected texture. See [masks.md](masks.md).